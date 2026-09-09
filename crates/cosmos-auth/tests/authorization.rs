use alloy_primitives::{Address, B256, Bytes, U256, hex};
use cosmos_auth::{AuthError, MAX_RAW_TX, Operation, decode_wire, derive_sender, encode_wire};
use k256::ecdsa::{Signature, SigningKey, signature::Signer};

fn fixture() -> (Operation, [u8; 33], [u8; 64]) {
    let json: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/adr036-v1.json")).unwrap();
    let raw = hex::decode(json["raw"].as_str().unwrap()).unwrap();
    let (op, key, sig) = decode_wire(&raw).unwrap();
    assert_eq!(hex::encode_prefixed(op.payload()), json["payload"]);
    assert_eq!(hex::encode_prefixed(op.sign_bytes()), json["signBytes"]);
    assert_eq!(hex::encode_prefixed(key), json["publicKey"]);
    assert_eq!(hex::encode_prefixed(sig), json["signature"]);
    assert_eq!(cosmos_auth::cosmos_address(op.sender), json["cosmos"]);
    assert_eq!(
        cosmos_auth::transaction_hash(&raw).to_string(),
        json["hash"]
    );
    (op, key, sig)
}

#[test]
fn independent_cosmjs_fixture_and_deterministic_signature() {
    let (op, key, sig) = fixture();
    assert_eq!(derive_sender(&key).unwrap(), op.sender);
    assert_eq!(op.verify(&key, &sig, op.genesis_hash).unwrap(), op.sender);
    let signing = SigningKey::from_slice(&[vec![0; 31], vec![1]].concat()).unwrap();
    let rust_sig: Signature = signing.sign(&op.sign_bytes());
    assert_eq!(rust_sig.to_bytes().as_slice(), sig);
}

#[test]
fn every_operation_field_is_bound() {
    let (op, key, sig) = fixture();
    let mutations: Vec<Operation> = vec![
        Operation {
            chain_id: op.chain_id + 1,
            ..op.clone()
        },
        Operation {
            genesis_hash: B256::ZERO,
            ..op.clone()
        },
        Operation {
            sender: Address::ZERO,
            ..op.clone()
        },
        Operation {
            nonce: op.nonce + 1,
            ..op.clone()
        },
        Operation {
            to: Address::ZERO,
            ..op.clone()
        },
        Operation {
            value: op.value + U256::from(1),
            ..op.clone()
        },
        Operation {
            input: Bytes::from_static(&[1]),
            ..op.clone()
        },
        Operation {
            gas_limit: op.gas_limit + 1,
            ..op.clone()
        },
        Operation {
            max_fee_per_gas: op.max_fee_per_gas + 1,
            ..op.clone()
        },
        Operation {
            max_priority_fee_per_gas: op.max_priority_fee_per_gas + 1,
            ..op.clone()
        },
    ];
    for mutation in mutations {
        assert!(mutation.verify(&key, &sig, op.genesis_hash).is_err());
    }
}

#[test]
fn tampering_and_noncanonical_signatures_fail() {
    let (op, key, sig) = fixture();
    for i in 0..sig.len() {
        let mut bad = sig;
        bad[i] ^= 1;
        assert!(op.verify(&key, &bad, op.genesis_hash).is_err());
    }
    let order = U256::from_be_slice(&hex!(
        "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141"
    ));
    let mut high_s = sig;
    high_s[32..].copy_from_slice(&(order - U256::from_be_slice(&sig[32..])).to_be_bytes::<32>());
    assert!(matches!(
        op.verify(&key, &high_s, op.genesis_hash),
        Err(AuthError::Signature)
    ));
    assert!(op.verify(&key, &sig[..63], op.genesis_hash).is_err());
    assert!(op.verify(&[0; 33], &sig, op.genesis_hash).is_err());
    assert!(op.verify(&key[..32], &sig, op.genesis_hash).is_err());
    assert!(op.verify(&key, &sig, B256::ZERO).is_err());
}

#[test]
fn decoder_rejects_truncation_trailing_data_and_size_abuse() {
    let (op, key, sig) = fixture();
    let raw = encode_wire(&op, &key, &sig);
    for len in 0..raw.len() {
        assert!(decode_wire(&raw[..len]).is_err());
    }
    let mut extra = raw.clone();
    extra.push(0);
    assert!(decode_wire(&extra).is_err());
    assert!(decode_wire(&vec![0; MAX_RAW_TX + 1]).is_err());
    let mut wrong_type = raw;
    wrong_type[0] = 2;
    assert!(decode_wire(&wrong_type).is_err());
    assert!(decode_wire(&[0x7e, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]).is_err());
}
