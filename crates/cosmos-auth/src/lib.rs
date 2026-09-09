//! Canonical ADR-036 authorization for the experimental Reth Cosmos chain.
use alloy_primitives::{Address, B256, Bytes, U256, keccak256};
use alloy_rlp::{Decodable, Encodable, Header};
use base64::{Engine, engine::general_purpose::STANDARD};
use bech32::ToBase32;
use k256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use ripemd::Ripemd160;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CHAIN_ID: u64 = 366036;
pub const GENESIS_HASH: B256 =
    alloy_primitives::b256!("09171e117af661185d9a4a4ff7cf83d29b4b99893bc95b204c0e6e0a6d6a5272");
pub const TX_TYPE: u8 = 0x7e;
pub const MAX_CALLDATA: usize = 32 * 1024;
pub const MAX_RAW_TX: usize = MAX_CALLDATA + 512;
pub const MAX_GAS: u64 = 30_000_000;
const PROTOCOL: &[u8] = b"reth-cosmos-adr036";

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    #[serde(with = "alloy_serde::quantity")]
    pub chain_id: u64,
    pub genesis_hash: B256,
    pub sender: Address,
    #[serde(with = "alloy_serde::quantity")]
    pub nonce: u64,
    pub to: Address,
    pub value: U256,
    pub input: Bytes,
    #[serde(with = "alloy_serde::quantity", rename = "gas")]
    pub gas_limit: u64,
    #[serde(with = "alloy_serde::quantity")]
    pub max_fee_per_gas: u128,
    #[serde(with = "alloy_serde::quantity")]
    pub max_priority_fee_per_gas: u128,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid or noncanonical transaction encoding")]
    Encoding,
    #[error("transaction exceeds protocol limits")]
    Limit,
    #[error("wrong chain or genesis")]
    Domain,
    #[error("invalid compressed secp256k1 public key")]
    PublicKey,
    #[error("sender does not match public key")]
    Sender,
    #[error("invalid or noncanonical Cosmos signature")]
    Signature,
}

impl From<alloy_rlp::Error> for AuthError {
    fn from(_: alloy_rlp::Error) -> Self {
        Self::Encoding
    }
}

impl Operation {
    pub fn validate_limits(&self) -> Result<(), AuthError> {
        if self.input.len() > MAX_CALLDATA
            || self.gas_limit > MAX_GAS
            || self.gas_limit < 21_000
            || self.nonce == u64::MAX
            || self.max_priority_fee_per_gas > self.max_fee_per_gas
        {
            return Err(AuthError::Limit);
        }
        Ok(())
    }

    /// Versioned RLP list. Integers use minimal unsigned big-endian RLP.
    pub fn payload(&self) -> Vec<u8> {
        let mut fields = Vec::with_capacity(self.input.len() + 192);
        PROTOCOL.encode(&mut fields);
        1u8.encode(&mut fields);
        self.chain_id.encode(&mut fields);
        self.genesis_hash.encode(&mut fields);
        self.sender.encode(&mut fields);
        self.nonce.encode(&mut fields);
        self.to.encode(&mut fields);
        self.value.encode(&mut fields);
        self.input.encode(&mut fields);
        self.gas_limit.encode(&mut fields);
        self.max_fee_per_gas.encode(&mut fields);
        self.max_priority_fee_per_gas.encode(&mut fields);
        list(&fields)
    }

    pub fn decode_payload(raw: &[u8]) -> Result<Self, AuthError> {
        if raw.len() > MAX_RAW_TX {
            return Err(AuthError::Limit);
        }
        let mut buf = list_body(raw)?;
        let protocol: Bytes = Decodable::decode(&mut buf)?;
        let version = u8::decode(&mut buf)?;
        if protocol.as_ref() != PROTOCOL || version != 1 {
            return Err(AuthError::Encoding);
        }
        let op = Self {
            chain_id: Decodable::decode(&mut buf)?,
            genesis_hash: Decodable::decode(&mut buf)?,
            sender: Decodable::decode(&mut buf)?,
            nonce: Decodable::decode(&mut buf)?,
            to: Decodable::decode(&mut buf)?,
            value: Decodable::decode(&mut buf)?,
            input: Decodable::decode(&mut buf)?,
            gas_limit: Decodable::decode(&mut buf)?,
            max_fee_per_gas: Decodable::decode(&mut buf)?,
            max_priority_fee_per_gas: Decodable::decode(&mut buf)?,
        };
        if !buf.is_empty() || op.payload() != raw {
            return Err(AuthError::Encoding);
        }
        op.validate_limits()?;
        Ok(op)
    }

    /// Sorted ASCII JSON keys, no whitespace; ADR-036 Amino sign document.
    pub fn sign_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "account_number": "0", "chain_id": "", "fee": {"amount": [], "gas": "0"},
            "memo": "", "msgs": [{"type": "sign/MsgSignData", "value": {
                "data": STANDARD.encode(self.payload()), "signer": cosmos_address(self.sender)
            }}], "sequence": "0"
        }))
        .expect("fixed JSON document is serializable")
    }

    pub fn sign_hash(&self) -> B256 {
        B256::from_slice(&Sha256::digest(self.sign_bytes()))
    }

    pub fn verify(
        &self,
        key: &[u8],
        signature: &[u8],
        genesis: B256,
    ) -> Result<Address, AuthError> {
        self.validate_limits()?;
        if self.chain_id != CHAIN_ID || self.genesis_hash != genesis {
            return Err(AuthError::Domain);
        }
        let sender = derive_sender(key)?;
        if self.sender != sender {
            return Err(AuthError::Sender);
        }
        let signature = Signature::from_slice(signature).map_err(|_| AuthError::Signature)?;
        if signature.normalize_s().is_some() {
            return Err(AuthError::Signature);
        }
        let key = VerifyingKey::from_sec1_bytes(key).map_err(|_| AuthError::PublicKey)?;
        key.verify(&self.sign_bytes(), &signature)
            .map_err(|_| AuthError::Signature)?;
        Ok(sender)
    }
}

pub fn derive_sender(key: &[u8]) -> Result<Address, AuthError> {
    if key.len() != 33 || !matches!(key[0], 2 | 3) {
        return Err(AuthError::PublicKey);
    }
    VerifyingKey::from_sec1_bytes(key).map_err(|_| AuthError::PublicKey)?;
    Ok(Address::from_slice(&Ripemd160::digest(Sha256::digest(key))))
}

pub fn cosmos_address(sender: Address) -> String {
    bech32::encode(
        "cosmos",
        sender.as_slice().to_base32(),
        bech32::Variant::Bech32,
    )
    .expect("fixed HRP and 20-byte address")
}

pub fn encode_wire(op: &Operation, key: &[u8], signature: &[u8]) -> Vec<u8> {
    let mut fields = Vec::with_capacity(op.input.len() + 300);
    op.payload().as_slice().encode(&mut fields);
    key.encode(&mut fields);
    signature.encode(&mut fields);
    let mut raw = vec![TX_TYPE];
    raw.extend(list(&fields));
    raw
}

pub fn decode_wire(raw: &[u8]) -> Result<(Operation, [u8; 33], [u8; 64]), AuthError> {
    if raw.len() > MAX_RAW_TX {
        return Err(AuthError::Limit);
    }
    if raw.first() != Some(&TX_TYPE) {
        return Err(AuthError::Encoding);
    }
    let mut buf = list_body(&raw[1..])?;
    let payload: Bytes = Decodable::decode(&mut buf)?;
    let key: Bytes = Decodable::decode(&mut buf)?;
    let signature: Bytes = Decodable::decode(&mut buf)?;
    let op = Operation::decode_payload(&payload)?;
    let key: [u8; 33] = key.as_ref().try_into().map_err(|_| AuthError::PublicKey)?;
    let signature: [u8; 64] = signature
        .as_ref()
        .try_into()
        .map_err(|_| AuthError::Signature)?;
    if !buf.is_empty() || encode_wire(&op, &key, &signature) != raw {
        return Err(AuthError::Encoding);
    }
    Ok((op, key, signature))
}

pub fn transaction_hash(raw: &[u8]) -> B256 {
    keccak256(raw)
}

fn list(fields: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(fields.len() + 4);
    Header {
        list: true,
        payload_length: fields.len(),
    }
    .encode(&mut out);
    out.extend_from_slice(fields);
    out
}

fn list_body(mut raw: &[u8]) -> Result<&[u8], AuthError> {
    let header = Header::decode(&mut raw)?;
    if !header.list || raw.len() != header.payload_length {
        return Err(AuthError::Encoding);
    }
    Ok(raw)
}
