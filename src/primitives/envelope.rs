use super::{CosmosSigned, CosmosTxType};
use alloy_consensus::{
    InMemorySize, Transaction, TransactionEnvelope,
    crypto::RecoveryError,
    transaction::{SignerRecoverable, TxHashRef},
};
use alloy_eips::{
    Decodable2718, Encodable2718, Typed2718,
    eip2718::{Eip2718Result, IsTyped2718},
    eip2930::AccessList,
    eip7702::SignedAuthorization,
};
use alloy_primitives::{Address, B256, Bytes, ChainId, TxKind, U256};
use alloy_rlp::{BufMut, Decodable, Encodable, Header};

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
// Clippy treats generic T as zero-sized. Our concrete Ethereum envelope has a
// comparable size (bounded below), so keep transactions inline without a box.
#[allow(clippy::large_enum_variant)]
pub enum CosmosEnvelope<T> {
    Ethereum(T),
    Cosmos(CosmosSigned),
}
const _: () = assert!(
    std::mem::size_of::<CosmosSigned>()
        <= std::mem::size_of::<alloy_consensus::EthereumTxEnvelope<alloy_consensus::TxEip4844>>()
            + 128
);
impl<T: Default> Default for CosmosEnvelope<T> {
    fn default() -> Self {
        Self::Ethereum(T::default())
    }
}
impl<T: Transaction> Transaction for CosmosEnvelope<T> {
    fn chain_id(&self) -> Option<ChainId> {
        match self {
            Self::Ethereum(tx) => tx.chain_id(),
            Self::Cosmos(tx) => tx.chain_id(),
        }
    }
    fn nonce(&self) -> u64 {
        match self {
            Self::Ethereum(tx) => tx.nonce(),
            Self::Cosmos(tx) => tx.nonce(),
        }
    }
    fn gas_limit(&self) -> u64 {
        match self {
            Self::Ethereum(tx) => tx.gas_limit(),
            Self::Cosmos(tx) => tx.gas_limit(),
        }
    }
    fn gas_price(&self) -> Option<u128> {
        match self {
            Self::Ethereum(tx) => tx.gas_price(),
            Self::Cosmos(tx) => tx.gas_price(),
        }
    }
    fn max_fee_per_gas(&self) -> u128 {
        match self {
            Self::Ethereum(tx) => tx.max_fee_per_gas(),
            Self::Cosmos(tx) => tx.max_fee_per_gas(),
        }
    }
    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        match self {
            Self::Ethereum(tx) => tx.max_priority_fee_per_gas(),
            Self::Cosmos(tx) => tx.max_priority_fee_per_gas(),
        }
    }
    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        match self {
            Self::Ethereum(tx) => tx.max_fee_per_blob_gas(),
            Self::Cosmos(tx) => tx.max_fee_per_blob_gas(),
        }
    }
    fn priority_fee_or_price(&self) -> u128 {
        match self {
            Self::Ethereum(tx) => tx.priority_fee_or_price(),
            Self::Cosmos(tx) => tx.priority_fee_or_price(),
        }
    }
    fn effective_gas_price(&self, base_fee: Option<u64>) -> u128 {
        match self {
            Self::Ethereum(tx) => tx.effective_gas_price(base_fee),
            Self::Cosmos(tx) => tx.effective_gas_price(base_fee),
        }
    }
    fn is_dynamic_fee(&self) -> bool {
        match self {
            Self::Ethereum(tx) => tx.is_dynamic_fee(),
            Self::Cosmos(tx) => tx.is_dynamic_fee(),
        }
    }
    fn kind(&self) -> TxKind {
        match self {
            Self::Ethereum(tx) => tx.kind(),
            Self::Cosmos(tx) => tx.kind(),
        }
    }
    fn is_create(&self) -> bool {
        match self {
            Self::Ethereum(tx) => tx.is_create(),
            Self::Cosmos(tx) => tx.is_create(),
        }
    }
    fn value(&self) -> U256 {
        match self {
            Self::Ethereum(tx) => tx.value(),
            Self::Cosmos(tx) => tx.value(),
        }
    }
    fn input(&self) -> &Bytes {
        match self {
            Self::Ethereum(tx) => tx.input(),
            Self::Cosmos(tx) => tx.input(),
        }
    }
    fn access_list(&self) -> Option<&AccessList> {
        match self {
            Self::Ethereum(tx) => tx.access_list(),
            Self::Cosmos(tx) => tx.access_list(),
        }
    }
    fn blob_versioned_hashes(&self) -> Option<&[B256]> {
        match self {
            Self::Ethereum(tx) => tx.blob_versioned_hashes(),
            Self::Cosmos(tx) => tx.blob_versioned_hashes(),
        }
    }
    fn authorization_list(&self) -> Option<&[SignedAuthorization]> {
        match self {
            Self::Ethereum(tx) => tx.authorization_list(),
            Self::Cosmos(tx) => tx.authorization_list(),
        }
    }
}
impl<T: Typed2718> Typed2718 for CosmosEnvelope<T> {
    fn ty(&self) -> u8 {
        match self {
            Self::Ethereum(t) => t.ty(),
            Self::Cosmos(t) => t.ty(),
        }
    }
}
impl<T: Transaction> TransactionEnvelope for CosmosEnvelope<T> {
    type TxType = CosmosTxType;
    fn tx_type(&self) -> CosmosTxType {
        CosmosTxType::try_from(self.ty()).expect("supported envelope type")
    }
}
impl<T: IsTyped2718> IsTyped2718 for CosmosEnvelope<T> {
    fn is_type(ty: u8) -> bool {
        ty == cosmos_auth::TX_TYPE || T::is_type(ty)
    }
}
impl<T: InMemorySize> InMemorySize for CosmosEnvelope<T> {
    fn size(&self) -> usize {
        match self {
            Self::Ethereum(t) => t.size(),
            Self::Cosmos(t) => t.size(),
        }
    }
}
impl<T: TxHashRef> TxHashRef for CosmosEnvelope<T> {
    fn tx_hash(&self) -> &B256 {
        match self {
            Self::Ethereum(t) => t.tx_hash(),
            Self::Cosmos(t) => t.hash(),
        }
    }
}
impl<T: SignerRecoverable> SignerRecoverable for CosmosEnvelope<T> {
    fn recover_signer(&self) -> Result<Address, RecoveryError> {
        match self {
            Self::Ethereum(t) => t.recover_signer(),
            Self::Cosmos(t) => t.verify(),
        }
    }
    fn recover_signer_unchecked(&self) -> Result<Address, RecoveryError> {
        match self {
            Self::Ethereum(t) => t.recover_signer_unchecked(),
            Self::Cosmos(t) => t.verify(),
        }
    }
    fn recover_with_buf(&self, buf: &mut Vec<u8>) -> Result<Address, RecoveryError> {
        match self {
            Self::Ethereum(t) => t.recover_with_buf(buf),
            Self::Cosmos(t) => t.verify(),
        }
    }
    fn recover_unchecked_with_buf(&self, buf: &mut Vec<u8>) -> Result<Address, RecoveryError> {
        match self {
            Self::Ethereum(t) => t.recover_unchecked_with_buf(buf),
            Self::Cosmos(t) => t.verify(),
        }
    }
}
impl<T: Encodable2718> Encodable2718 for CosmosEnvelope<T> {
    fn encode_2718_len(&self) -> usize {
        match self {
            Self::Ethereum(t) => t.encode_2718_len(),
            Self::Cosmos(t) => t.raw().len(),
        }
    }
    fn encode_2718(&self, out: &mut dyn BufMut) {
        match self {
            Self::Ethereum(t) => t.encode_2718(out),
            Self::Cosmos(t) => out.put_slice(t.raw()),
        }
    }
}
impl<T: Decodable2718> Decodable2718 for CosmosEnvelope<T> {
    fn typed_decode(ty: u8, buf: &mut &[u8]) -> Eip2718Result<Self> {
        if ty != cosmos_auth::TX_TYPE {
            return T::typed_decode(ty, buf).map(Self::Ethereum);
        }
        let original = *buf;
        let mut probe = original;
        let h = Header::decode(&mut probe)?;
        let len = h.length() + h.payload_length;
        if !h.list || len > cosmos_auth::MAX_RAW_TX - 1 || len > original.len() {
            return Err(alloy_rlp::Error::Custom("invalid Cosmos transaction size").into());
        }
        let mut raw = Vec::with_capacity(len + 1);
        raw.push(ty);
        raw.extend_from_slice(&original[..len]);
        let (op, key, sig) = cosmos_auth::decode_wire(&raw)
            .map_err(|_| alloy_rlp::Error::Custom("invalid Cosmos encoding"))?;
        let signed = CosmosSigned::new(
            op,
            Bytes::copy_from_slice(&key),
            Bytes::copy_from_slice(&sig),
        )
        .map_err(|_| alloy_rlp::Error::Custom("invalid Cosmos encoding"))?;
        *buf = &original[len..];
        Ok(Self::Cosmos(signed))
    }
    fn fallback_decode(buf: &mut &[u8]) -> Eip2718Result<Self> {
        T::fallback_decode(buf).map(Self::Ethereum)
    }
}
impl<T: Encodable2718> Encodable for CosmosEnvelope<T> {
    fn encode(&self, out: &mut dyn BufMut) {
        self.network_encode(out)
    }
    fn length(&self) -> usize {
        self.network_len()
    }
}
impl<T: Decodable2718> Decodable for CosmosEnvelope<T> {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        Ok(Self::network_decode(buf)?)
    }
}

impl From<super::PooledTransaction> for super::Transaction {
    fn from(tx: super::PooledTransaction) -> Self {
        match tx {
            CosmosEnvelope::Ethereum(t) => Self::Ethereum(t.into()),
            CosmosEnvelope::Cosmos(t) => Self::Cosmos(t),
        }
    }
}
impl TryFrom<super::Transaction> for super::PooledTransaction {
    type Error = alloy_consensus::error::ValueError<super::Transaction>;
    fn try_from(tx: super::Transaction) -> Result<Self, Self::Error> {
        match tx {
            CosmosEnvelope::Ethereum(t) => t
                .try_into_pooled()
                .map(Self::Ethereum)
                .map_err(|e| e.map(CosmosEnvelope::Ethereum)),
            CosmosEnvelope::Cosmos(t) => Ok(Self::Cosmos(t)),
        }
    }
}
