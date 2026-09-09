use alloy_consensus::{Block, BlockBody, Header, Signed, TxCosmos, transaction::SignerRecoverable};
use alloy_eips::{Decodable2718, Encodable2718};
use alloy_primitives::{Bytes, Signature, U256, hex};
use cosmos_auth::GENESIS_HASH;
use k256::ecdsa::{Signature as CosmosSignature, SigningKey, signature::Signer};
use reth_codecs::Compact;
use reth_ethereum::{TransactionSigned, chainspec::ChainSpec, primitives::SealedBlock};

fn signed() -> Signed<TxCosmos> {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/adr036-v1.json")).unwrap();
    let raw = hex::decode(fixture["raw"].as_str().unwrap()).unwrap();
    let (mut operation, key, _) = cosmos_auth::decode_wire(&raw).unwrap();
    operation.genesis_hash = GENESIS_HASH;
    let signer = SigningKey::from_slice(&[vec![0; 31], vec![1]].concat()).unwrap();
    let signature: CosmosSignature = signer.sign(&operation.sign_bytes());
    let bytes = signature.to_bytes();
    Signed::new_unhashed(
        TxCosmos {
            operation,
            public_key: Bytes::copy_from_slice(&key),
        },
        Signature::new(
            U256::from_be_slice(&bytes[..32]),
            U256::from_be_slice(&bytes[32..]),
            false,
        ),
    )
}

#[test]
fn envelope_storage_and_rpc_preserve_original_authorization() {
    let tx: TransactionSigned = signed().into();
    let raw = tx.encoded_2718();
    let typed = tx.clone().into_signed();
    assert_eq!(typed.encoded_2718(), raw);
    assert_eq!(typed.encode_2718_len(), raw.len());
    let decoded = TransactionSigned::decode_2718_exact(&raw).unwrap();
    assert_eq!(decoded, tx);
    let mut compact = vec![];
    let len = tx.to_compact(&mut compact);
    let (stored, rest) = TransactionSigned::from_compact(&compact, len);
    assert!(rest.is_empty());
    assert_eq!(stored.encoded_2718(), raw);
    assert_eq!(
        stored.recover_signer().unwrap(),
        signed().tx().operation.sender
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
    assert_eq!(signed.signature_hash(), signed.tx().operation.sign_hash());
    assert_eq!(
        signed.recover_signer().unwrap(),
        signed.tx().operation.sender
    );
}

#[test]
fn all_recovery_paths_reject_tampering_including_unchecked() {
    let mut signed = signed();
    signed.tx_mut().operation.value += U256::from(1);
    let tx: TransactionSigned = signed.clone().into();
    assert!(signed.recover_signer().is_err());
    let mut buf = vec![];
    assert!(tx.recover_signer().is_err());
    assert!(tx.recover_signer_unchecked().is_err());
    assert!(tx.recover_with_buf(&mut buf).is_err());
    assert!(tx.recover_unchecked_with_buf(&mut buf).is_err());
    let typed = tx.into_signed();
    assert!(SignerRecoverable::recover_signer(&typed).is_err());
    assert!(SignerRecoverable::recover_signer_unchecked(&typed).is_err());
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
    let tx: TransactionSigned = signed().into();
    let valid = block(tx);
    reth_ethereum::consensus::validation::validate_block_pre_execution(&valid, &chain).unwrap();
    let mut bad = signed();
    bad.tx_mut().operation.nonce += 1;
    let invalid = block(bad.into());
    assert!(
        reth_ethereum::consensus::validation::validate_block_pre_execution(&invalid, &chain)
            .is_err()
    );
    assert!(
        reth_ethereum::consensus::validation::validate_block_pre_execution_with_tx_root(
            &invalid,
            &chain,
            Some(invalid.header().transactions_root)
        )
        .is_err()
    );
}

#[test]
fn cosmos_receipt_has_distinct_wire_type() {
    let receipt = alloy_consensus::EthereumReceipt {
        tx_type: alloy_consensus::TxType::Cosmos,
        success: true,
        cumulative_gas_used: 21000,
        logs: vec![],
    };
    let envelope: alloy_consensus::ReceiptEnvelope = receipt.into();
    assert_eq!(envelope.encoded_2718()[0], cosmos_auth::TX_TYPE);
}
