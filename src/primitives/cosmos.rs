use alloy_consensus::{InMemorySize, Transaction, crypto::RecoveryError};
use alloy_eips::{Typed2718, eip2930::AccessList, eip7702::SignedAuthorization};
use alloy_primitives::{Address, B256, Bytes, ChainId, Signature, TxKind, U256};
use cosmos_auth::{AuthError, GENESIS_HASH, Operation};
use serde::{Deserialize, Serialize};

/// Immutable authorization. Never exposes Alloy's Ethereum signer-recovery machinery.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CosmosSigned {
    operation: Operation,
    public_key: Bytes,
    signature: Signature,
    hash: B256,
    raw: Bytes,
}
impl CosmosSigned {
    pub fn new(
        operation: Operation,
        public_key: Bytes,
        signature: Bytes,
    ) -> Result<Self, AuthError> {
        operation.validate_limits()?;
        if public_key.len() != 33 {
            return Err(AuthError::PublicKey);
        }
        if signature.len() != 64 {
            return Err(AuthError::Signature);
        }
        let raw = cosmos_auth::encode_wire(&operation, &public_key, &signature);
        let (operation, _, sig) = cosmos_auth::decode_wire(&raw)?;
        Ok(Self {
            operation,
            public_key,
            signature: Signature::new(
                U256::from_be_slice(&sig[..32]),
                U256::from_be_slice(&sig[32..]),
                false,
            ),
            hash: cosmos_auth::transaction_hash(&raw),
            raw: raw.into(),
        })
    }
    pub fn operation(&self) -> &Operation {
        &self.operation
    }
    pub fn public_key(&self) -> &Bytes {
        &self.public_key
    }
    pub fn signature(&self) -> &Signature {
        &self.signature
    }
    pub fn hash(&self) -> &B256 {
        &self.hash
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn verify(&self) -> Result<Address, RecoveryError> {
        self.operation
            .verify(
                &self.public_key,
                &self.signature.as_bytes()[..64],
                GENESIS_HASH,
            )
            .map_err(|_| RecoveryError::new())
    }
}
impl Typed2718 for CosmosSigned {
    fn ty(&self) -> u8 {
        cosmos_auth::TX_TYPE
    }
}
impl InMemorySize for CosmosSigned {
    fn size(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.operation.input.len()
            + self.public_key.len()
            + self.raw.len()
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rpc {
    #[serde(rename = "type")]
    tx_type: super::CosmosTxType,
    #[serde(flatten)]
    operation: Operation,
    public_key: Bytes,
    cosmos_signature: Bytes,
    hash: B256,
}
impl Serialize for CosmosSigned {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        Rpc {
            tx_type: super::CosmosTxType::COSMOS,
            operation: self.operation.clone(),
            public_key: self.public_key.clone(),
            cosmos_signature: Bytes::copy_from_slice(&self.signature.as_bytes()[..64]),
            hash: self.hash,
        }
        .serialize(s)
    }
}
impl<'de> Deserialize<'de> for CosmosSigned {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let rpc = Rpc::deserialize(d)?;
        let tx = Self::new(rpc.operation, rpc.public_key, rpc.cosmos_signature)
            .map_err(serde::de::Error::custom)?;
        if rpc.tx_type != super::CosmosTxType::COSMOS || tx.hash != rpc.hash {
            return Err(serde::de::Error::custom("Cosmos type or hash mismatch"));
        }
        Ok(tx)
    }
}
impl Transaction for CosmosSigned {
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
