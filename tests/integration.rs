use alloy_consensus::{Block, BlockBody, Header, TxReceipt, transaction::SignerRecoverable};
use alloy_eips::{Decodable2718, Encodable2718};
use alloy_primitives::{Bytes, U256, hex};
use cosmos_auth::GENESIS_HASH;
use k256::ecdsa::{Signature as CosmosSignature, SigningKey, signature::Signer};
use reth_codecs::Compact;
use reth_consensus::Consensus;
use reth_cosmos_dev::{
    consensus::CosmosConsensus,
    primitives::{CosmosEnvelope, CosmosSigned, CosmosTxType, Transaction as TransactionSigned},
};
use reth_ethereum::{chainspec::ChainSpec, primitives::SealedBlock};
use std::sync::Arc;

fn signed() -> CosmosSigned {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/adr036-v1.json")).unwrap();
    let raw = hex::decode(fixture["raw"].as_str().unwrap()).unwrap();
    let (mut op, key, _) = cosmos_auth::decode_wire(&raw).unwrap();
    op.genesis_hash = GENESIS_HASH;
    let signing = SigningKey::from_slice(&[vec![0; 31], vec![1]].concat()).unwrap();
    let sig: CosmosSignature = signing.sign(&op.sign_bytes());
    CosmosSigned::new(
        op,
        Bytes::copy_from_slice(&key),
        Bytes::copy_from_slice(&sig.to_bytes()),
    )
    .unwrap()
}
fn tampered() -> CosmosSigned {
    let signed = signed();
    let mut op = signed.operation().clone();
    op.value += U256::from(1);
    CosmosSigned::new(
        op,
        signed.public_key().clone(),
        Bytes::copy_from_slice(&signed.signature().as_bytes()[..64]),
    )
    .unwrap()
}

#[test]
fn envelope_storage_and_rpc_preserve_original_authorization() {
    let tx: TransactionSigned = CosmosEnvelope::Cosmos(signed());
    let raw = tx.encoded_2718();
    assert_eq!(tx.encode_2718_len(), raw.len());
    let decoded = TransactionSigned::decode_2718_exact(&raw).unwrap();
    assert_eq!(decoded, tx);
    let mut compact = vec![];
    let len = tx.to_compact(&mut compact);
    let (stored, rest) = TransactionSigned::from_compact(&compact, len);
    assert!(rest.is_empty());
    assert_eq!(stored.encoded_2718(), raw);
    assert_eq!(
        stored.recover_signer().unwrap(),
        signed().operation().sender
    );
    let json = serde_json::to_value(&stored).unwrap();
    assert_eq!(json["type"], "0x7e");
    assert!(json.get("cosmosSignature").is_some());
    for field in ["v", "r", "s", "yParity"] {
        assert!(json.get(field).is_none());
    }
    let roundtrip: TransactionSigned = serde_json::from_value(json).unwrap();
    assert_eq!(roundtrip.encoded_2718(), raw);
    let signed = signed();
    assert_eq!(signed.verify().unwrap(), signed.operation().sender);
}

#[test]
fn all_recovery_paths_reject_tampering_including_unchecked() {
    let signed = tampered();
    assert!(signed.verify().is_err());
    let tx: TransactionSigned = CosmosEnvelope::Cosmos(signed);
    let mut buf = vec![];
    assert!(tx.recover_signer().is_err());
    assert!(tx.recover_signer_unchecked().is_err());
    assert!(tx.recover_with_buf(&mut buf).is_err());
    assert!(tx.recover_unchecked_with_buf(&mut buf).is_err());
}

#[test]
fn envelope_decoder_bounds_untrusted_input_and_preserves_network_framing() {
    let tx: TransactionSigned = CosmosEnvelope::Cosmos(signed());
    let raw = tx.encoded_2718();
    for length in 0..raw.len() {
        assert!(TransactionSigned::decode_2718_exact(&raw[..length]).is_err());
    }
    let mut trailing = raw.clone();
    trailing.push(0);
    assert!(TransactionSigned::decode_2718_exact(&trailing).is_err());
    let mut oversized = vec![cosmos_auth::TX_TYPE];
    alloy_rlp::Header {
        list: true,
        payload_length: cosmos_auth::MAX_RAW_TX,
    }
    .encode(&mut oversized);
    oversized.resize(oversized.len() + cosmos_auth::MAX_RAW_TX, 0);
    for malformed in [
        vec![0x7e, 0x80],
        vec![0x7e, 0xc0],
        oversized,
        vec![0x7e, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
    ] {
        assert!(TransactionSigned::decode_2718_exact(&malformed).is_err());
    }
    let mut network = vec![];
    tx.network_encode(&mut network);
    assert_eq!(tx.network_len(), network.len());
    network.push(0x80);
    let mut remaining = network.as_slice();
    assert_eq!(
        TransactionSigned::network_decode(&mut remaining).unwrap(),
        tx
    );
    assert_eq!(remaining, &[0x80]);
}

fn block(tx: TransactionSigned) -> SealedBlock<Block<TransactionSigned>> {
    let body = BlockBody {
        transactions: vec![tx],
        withdrawals: Some(Default::default()),
        ..Default::default()
    };
    let header = Header {
        number: 1,
        timestamp: 1,
        parent_hash: GENESIS_HASH,
        transactions_root: alloy_consensus::proofs::calculate_transaction_root(&body.transactions),
        withdrawals_root: Some(alloy_consensus::proofs::calculate_withdrawals_root(
            body.withdrawals.as_ref().unwrap(),
        )),
        ..Default::default()
    };
    SealedBlock::seal_slow(Block { header, body })
}

#[test]
fn import_validation_rejects_invalid_signatures_without_pool() {
    let genesis: alloy_genesis::Genesis =
        serde_json::from_str(include_str!("../genesis.json")).unwrap();
    let chain: ChainSpec = genesis.into();
    assert_eq!(chain.genesis_hash(), GENESIS_HASH);
    let tx: TransactionSigned = CosmosEnvelope::Cosmos(signed());
    let valid = block(tx);
    let consensus = CosmosConsensus::new(Arc::new(chain));
    consensus.validate_block_pre_execution(&valid).unwrap();
    let invalid = block(CosmosEnvelope::Cosmos(tampered()));
    assert!(consensus.validate_block_pre_execution(&invalid).is_err());
    assert!(
        consensus
            .validate_block_pre_execution_with_tx_root(
                &invalid,
                Some(invalid.header().transactions_root)
            )
            .is_err()
    );
}

#[test]
fn cosmos_receipt_has_distinct_wire_type() {
    let receipt = alloy_consensus::EthereumReceipt {
        tx_type: CosmosTxType::COSMOS,
        success: true,
        cumulative_gas_used: 21000,
        logs: vec![],
    };
    let envelope = receipt.with_bloom_ref();
    assert_eq!(envelope.encoded_2718()[0], cosmos_auth::TX_TYPE);
}

#[test]
fn pre_migration_storage_and_wire_fixtures_are_unchanged() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/pre-migration-storage.json")).unwrap();
    for case in fixture["transactions"].as_array().unwrap() {
        let raw = hex::decode(case["raw"].as_str().unwrap()).unwrap();
        let compact = hex::decode(case["compact"].as_str().unwrap()).unwrap();
        let tx = TransactionSigned::decode_2718_exact(&raw).unwrap();
        tx.recover_signer().unwrap();
        let (stored, _) = TransactionSigned::from_compact(&compact, compact.len());
        assert_eq!(stored.encoded_2718(), raw);
        let mut encoded = vec![];
        tx.to_compact(&mut encoded);
        assert_eq!(encoded, compact);
        assert_eq!(tx.encode_2718_len(), raw.len());
    }
    let compact = hex::decode(fixture["receipt"]["compact"].as_str().unwrap()).unwrap();
    let (receipt, _) = reth_cosmos_dev::primitives::Receipt::from_compact(&compact, compact.len());
    assert_eq!(
        hex::encode_prefixed(receipt.with_bloom_ref().encoded_2718()),
        fixture["receipt"]["raw"]
    );
    let mut encoded = vec![];
    receipt.to_compact(&mut encoded);
    assert_eq!(encoded, compact);
}

#[test]
fn ethereum_variants_delegate_encoding_storage_and_recovery() {
    use alloy_consensus::{
        EthereumTxEnvelope as Envelope, EthereumTypedTransaction as Unsigned, SignableTransaction,
        TxEip4844,
    };
    use alloy_primitives::Signature;
    let key = SigningKey::from_slice(&[vec![0; 31], vec![1]].concat()).unwrap();
    let variants: [Unsigned<TxEip4844>; 5] = [
        Unsigned::Legacy(Default::default()),
        Unsigned::Eip2930(Default::default()),
        Unsigned::Eip1559(Default::default()),
        Unsigned::Eip4844(Default::default()),
        Unsigned::Eip7702(Default::default()),
    ];
    for unsigned in variants {
        let (signature, recovery) = key
            .sign_prehash_recoverable(unsigned.signature_hash().as_slice())
            .unwrap();
        let sig = Signature::from_signature_and_parity(signature, recovery.is_y_odd());
        let eth: Envelope<TxEip4844> = unsigned.into_signed(sig).into();
        let tx: TransactionSigned = CosmosEnvelope::Ethereum(eth.clone());
        assert_eq!(tx.encoded_2718(), eth.encoded_2718());
        assert_eq!(tx.recover_signer().unwrap(), eth.recover_signer().unwrap());
        assert_eq!(
            TransactionSigned::decode_2718_exact(&eth.encoded_2718()).unwrap(),
            tx
        );
        let mut original = vec![];
        eth.to_compact(&mut original);
        let mut local = vec![];
        tx.to_compact(&mut local);
        assert_eq!(original, local);
        assert_eq!(
            TransactionSigned::from_compact(&original, original.len()).0,
            tx
        );
        let json = serde_json::to_value(&tx).unwrap();
        assert_eq!(json, serde_json::to_value(&eth).unwrap());
        assert_eq!(
            serde_json::from_value::<TransactionSigned>(json).unwrap(),
            tx
        );
    }
}
