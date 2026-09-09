//! Cosmos ADR-036 typed transactions for the fixed local chain.
use super::{RlpEcdsaDecodableTx, RlpEcdsaEncodableTx};
use crate::{SignableTransaction, Signed, Transaction, TxType};
use alloc::vec::Vec;
use alloy_eips::{
    Typed2718, eip2718::IsTyped2718, eip2930::AccessList, eip7702::SignedAuthorization,
};
use alloy_primitives::{Address, B256, Bytes, ChainId, Signature, TxKind, U256};
use alloy_rlp::{BufMut, Decodable, Encodable, Header};
pub use cosmos_auth::{CHAIN_ID, GENESIS_HASH, TX_TYPE};
use cosmos_auth::{MAX_RAW_TX, Operation};

/// Original Cosmos public key and operation. Signature resides in `Signed` as raw r/s;
/// parity is always false and is neither encoded nor exposed as Ethereum authorization.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxCosmos {
    /// Authorized operation including chain and genesis binding.
    #[serde(flatten)]
    pub operation: Operation,
    /// Canonical compressed SEC1 secp256k1 public key.
    pub public_key: Bytes,
}

impl TxCosmos {
    /// Transaction kind.
    pub const fn tx_type() -> TxType {
        TxType::Cosmos
    }
    /// Bounded in-memory size estimate.
    pub fn size(&self) -> usize {
        core::mem::size_of::<Self>() + self.operation.input.len() + self.public_key.len()
    }
    /// Verifies the original authorization, including low-S and the fixed chain domain.
    pub fn verify(&self, signature: &Signature) -> Result<Address, crate::crypto::RecoveryError> {
        if signature.v() {
            return Err(crate::crypto::RecoveryError::new());
        }
        self.operation
            .verify(&self.public_key, &signature.as_bytes()[..64], GENESIS_HASH)
            .map_err(|_| crate::crypto::RecoveryError::new())
    }
}

impl Typed2718 for TxCosmos {
    fn ty(&self) -> u8 {
        TX_TYPE
    }
}
impl IsTyped2718 for TxCosmos {
    fn is_type(ty: u8) -> bool {
        ty == TX_TYPE
    }
}

impl RlpEcdsaEncodableTx for TxCosmos {
    fn rlp_encoded_fields_length(&self) -> usize {
        self.operation.payload().as_slice().length() + self.public_key.length()
    }
    fn rlp_encode_fields(&self, out: &mut dyn BufMut) {
        self.operation.payload().as_slice().encode(out);
        self.public_key.encode(out);
    }
    fn rlp_header_signed(&self, _: &Signature) -> Header {
        Header {
            list: true,
            payload_length: self.rlp_encoded_fields_length() + 66,
        }
    }
    fn rlp_encode_signed(&self, sig: &Signature, out: &mut dyn BufMut) {
        self.rlp_header_signed(sig).encode(out);
        self.rlp_encode_fields(out);
        sig.as_bytes()[..64].encode(out);
    }
}
impl RlpEcdsaDecodableTx for TxCosmos {
    const DEFAULT_TX_TYPE: u8 = TX_TYPE;
    fn rlp_decode_fields(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        // Inspect declared lengths before allocating any transaction fields.
        let mut probe = *buf;
        let h = Header::decode(&mut probe)?;
        if h.list || h.payload_length > MAX_RAW_TX {
            return Err(alloy_rlp::Error::Custom("oversized Cosmos payload"));
        }
        let payload: Bytes = Decodable::decode(buf)?;
        let operation = Operation::decode_payload(&payload)
            .map_err(|_| alloy_rlp::Error::Custom("invalid Cosmos payload"))?;
        let mut probe = *buf;
        let h = Header::decode(&mut probe)?;
        if h.list || h.payload_length != 33 {
            return Err(alloy_rlp::Error::Custom("invalid Cosmos public key length"));
        }
        let public_key: Bytes = Decodable::decode(buf)?;
        Ok(Self {
            operation,
            public_key,
        })
    }
    fn rlp_decode_with_signature(buf: &mut &[u8]) -> alloy_rlp::Result<(Self, Signature)> {
        let header = Header::decode(buf)?;
        if !header.list || header.payload_length > MAX_RAW_TX || header.payload_length > buf.len() {
            return Err(alloy_rlp::Error::Custom(
                "invalid Cosmos transaction length",
            ));
        }
        let (body, rest) = buf.split_at(header.payload_length);
        let mut inner = body;
        let tx = Self::rlp_decode_fields(&mut inner)?;
        let mut probe = inner;
        let h = Header::decode(&mut probe)?;
        if h.list || h.payload_length != 64 {
            return Err(alloy_rlp::Error::Custom("invalid Cosmos signature length"));
        }
        let signature: Bytes = Decodable::decode(&mut inner)?;
        if !inner.is_empty() {
            return Err(alloy_rlp::Error::UnexpectedLength);
        }
        let sig = Signature::new(
            U256::from_be_slice(&signature[..32]),
            U256::from_be_slice(&signature[32..]),
            false,
        );
        let mut canonical = Vec::new();
        tx.rlp_encode_signed(&sig, &mut canonical);
        if canonical.len() != header.length() + body.len() || &canonical[header.length()..] != body
        {
            return Err(alloy_rlp::Error::Custom("noncanonical Cosmos transaction"));
        }
        *buf = rest;
        Ok((tx, sig))
    }
}
impl SignableTransaction<Signature> for TxCosmos {
    fn custom_recover_signer(
        &self,
        signature: &Signature,
    ) -> Option<Result<Address, crate::crypto::RecoveryError>> {
        Some(self.verify(signature))
    }
    fn signature_hash(&self) -> B256 {
        self.operation.sign_hash()
    }
    fn set_chain_id(&mut self, chain_id: ChainId) {
        self.operation.chain_id = chain_id;
    }
    fn encode_for_signing(&self, out: &mut dyn BufMut) {
        out.put_slice(&self.operation.sign_bytes());
    }
    fn payload_len_for_signature(&self) -> usize {
        self.operation.sign_bytes().len()
    }
}
impl Encodable for TxCosmos {
    fn encode(&self, out: &mut dyn BufMut) {
        self.rlp_encode(out);
    }
    fn length(&self) -> usize {
        self.rlp_encoded_length()
    }
}
impl Decodable for TxCosmos {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        Self::rlp_decode(buf)
    }
}

impl Transaction for TxCosmos {
    #[inline]
    fn chain_id(&self) -> Option<ChainId> {
        Some(self.operation.chain_id)
    }

    #[inline]
    fn nonce(&self) -> u64 {
        self.operation.nonce
    }

    #[inline]
    fn gas_limit(&self) -> u64 {
        self.operation.gas_limit
    }

    #[inline]
    fn gas_price(&self) -> Option<u128> {
        None
    }

    #[inline]
    fn max_fee_per_gas(&self) -> u128 {
        self.operation.max_fee_per_gas
    }

    #[inline]
    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        Some(self.operation.max_priority_fee_per_gas)
    }

    #[inline]
    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        None
    }

    #[inline]
    fn priority_fee_or_price(&self) -> u128 {
        self.operation.max_priority_fee_per_gas
    }

    fn effective_gas_price(&self, base_fee: Option<u64>) -> u128 {
        alloy_eips::eip1559::calc_effective_gas_price(
            self.operation.max_fee_per_gas,
            self.operation.max_priority_fee_per_gas,
            base_fee,
        )
    }

    #[inline]
    fn is_dynamic_fee(&self) -> bool {
        true
    }

    #[inline]
    fn kind(&self) -> TxKind {
        TxKind::Call(self.operation.to)
    }

    #[inline]
    fn is_create(&self) -> bool {
        false
    }

    #[inline]
    fn value(&self) -> U256 {
        self.operation.value
    }

    #[inline]
    fn input(&self) -> &Bytes {
        &self.operation.input
    }

    #[inline]
    fn access_list(&self) -> Option<&AccessList> {
        None
    }

    #[inline]
    fn blob_versioned_hashes(&self) -> Option<&[B256]> {
        None
    }

    #[inline]
    fn authorization_list(&self) -> Option<&[SignedAuthorization]> {
        None
    }
}

/// JSON representation preserving the Cosmos signature without Ethereum v/r/s fields.
#[cfg(feature = "serde")]
pub mod signed_serde {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Rpc {
        #[serde(flatten)]
        tx: TxCosmos,
        cosmos_signature: Bytes,
        hash: B256,
    }
    /// Serialize a Cosmos transaction explicitly.
    pub fn serialize<S: Serializer>(tx: &Signed<TxCosmos>, s: S) -> Result<S::Ok, S::Error> {
        Rpc {
            tx: tx.tx().clone(),
            cosmos_signature: Bytes::copy_from_slice(&tx.signature().as_bytes()[..64]),
            hash: *tx.hash(),
        }
        .serialize(s)
    }
    /// Deserialize and require the canonical hash.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Signed<TxCosmos>, D::Error> {
        let rpc = Rpc::deserialize(d)?;
        let bytes = rpc.cosmos_signature;
        if bytes.len() != 64 {
            return Err(serde::de::Error::custom(
                "expected 64-byte Cosmos signature",
            ));
        }
        let sig = Signature::new(
            U256::from_be_slice(&bytes[..32]),
            U256::from_be_slice(&bytes[32..]),
            false,
        );
        let tx = Signed::new_unhashed(rpc.tx, sig);
        if tx.hash() != &rpc.hash {
            return Err(serde::de::Error::custom("Cosmos hash mismatch"));
        }
        Ok(tx)
    }
}
