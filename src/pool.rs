use crate::primitives::{CosmosEnvelope, PooledTransaction, Transaction as CosmosTransaction};
use alloy_consensus::{
    InMemorySize, Transaction,
    error::ValueError,
    transaction::{Recovered, TxHashRef},
};
use alloy_eips::{
    Encodable2718, Typed2718,
    eip2930::AccessList,
    eip4844::{BlobTransactionValidationError, env_settings::KzgSettings},
    eip7594::BlobTransactionSidecarVariant,
    eip7702::SignedAuthorization,
};
use alloy_primitives::{Address, B256, Bytes, ChainId, TxKind, U256};
use reth_transaction_pool::{
    EthBlobTransactionSidecar, EthPoolTransaction, EthPooledTransaction, PoolTransaction,
};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct CosmosPoolTransaction(EthPooledTransaction<CosmosTransaction>);
impl PoolTransaction for CosmosPoolTransaction {
    type TryFromConsensusError = ValueError<CosmosTransaction>;
    type Consensus = CosmosTransaction;
    type Pooled = PooledTransaction;
    fn consensus_ref(&self) -> Recovered<&Self::Consensus> {
        self.0.transaction.as_recovered_ref()
    }
    fn into_consensus(self) -> Recovered<Self::Consensus> {
        self.0.transaction
    }
    fn from_pooled(tx: Recovered<Self::Pooled>) -> Self {
        let len = tx.encode_2718_len();
        let (tx, signer) = tx.into_parts();
        match tx {
            CosmosEnvelope::Ethereum(tx) => {
                let eth = EthPooledTransaction::from_pooled(Recovered::new_unchecked(tx, signer));
                let mut result =
                    EthPooledTransaction::new(eth.transaction.map(CosmosEnvelope::Ethereum), len);
                result.blob_sidecar = eth.blob_sidecar;
                Self(result)
            }
            CosmosEnvelope::Cosmos(tx) => Self(EthPooledTransaction::new(
                Recovered::new_unchecked(CosmosEnvelope::Cosmos(tx), signer),
                len,
            )),
        }
    }
    fn hash(&self) -> &B256 {
        self.0.transaction.tx_hash()
    }
    fn sender(&self) -> Address {
        self.0.transaction.signer()
    }
    fn sender_ref(&self) -> &Address {
        self.0.transaction.signer_ref()
    }
    fn cost(&self) -> &U256 {
        &self.0.cost
    }
    fn encoded_length(&self) -> usize {
        self.0.encoded_length
    }
}
impl EthPoolTransaction for CosmosPoolTransaction {
    fn take_blob(&mut self) -> EthBlobTransactionSidecar {
        if self.is_eip4844() {
            std::mem::replace(&mut self.0.blob_sidecar, EthBlobTransactionSidecar::Missing)
        } else {
            EthBlobTransactionSidecar::None
        }
    }
    fn try_into_pooled_eip4844(
        self,
        sidecar: Arc<BlobTransactionSidecarVariant>,
    ) -> Option<Recovered<Self::Pooled>> {
        let (tx, signer) = self.0.transaction.into_parts();
        let CosmosEnvelope::Ethereum(tx) = tx else {
            return None;
        };
        Some(Recovered::new_unchecked(
            CosmosEnvelope::Ethereum(
                tx.try_into_pooled_eip4844(Arc::unwrap_or_clone(sidecar))
                    .ok()?,
            ),
            signer,
        ))
    }
    fn try_from_eip4844(
        tx: Recovered<Self::Consensus>,
        sidecar: BlobTransactionSidecarVariant,
    ) -> Option<Self> {
        let (tx, signer) = tx.into_parts();
        let CosmosEnvelope::Ethereum(tx) = tx else {
            return None;
        };
        let tx = tx.try_into_pooled_eip4844(sidecar).ok()?;
        Some(Self::from_pooled(Recovered::new_unchecked(
            CosmosEnvelope::Ethereum(tx),
            signer,
        )))
    }
    fn validate_blob(
        &self,
        sidecar: &BlobTransactionSidecarVariant,
        settings: &KzgSettings,
    ) -> Result<(), BlobTransactionValidationError> {
        let CosmosEnvelope::Ethereum(tx) = &*self.0.transaction else {
            return Err(BlobTransactionValidationError::NotBlobTransaction(
                self.ty(),
            ));
        };
        match tx.as_eip4844() {
            Some(tx) => tx.tx().validate_blob(sidecar, settings),
            None => Err(BlobTransactionValidationError::NotBlobTransaction(
                self.ty(),
            )),
        }
    }
}
impl Typed2718 for CosmosPoolTransaction {
    fn ty(&self) -> u8 {
        self.0.ty()
    }
}
impl InMemorySize for CosmosPoolTransaction {
    fn size(&self) -> usize {
        self.0.size()
    }
}
impl Transaction for CosmosPoolTransaction {
    fn chain_id(&self) -> Option<ChainId> {
        self.0.chain_id()
    }
    fn nonce(&self) -> u64 {
        self.0.nonce()
    }
    fn gas_limit(&self) -> u64 {
        self.0.gas_limit()
    }
    fn gas_price(&self) -> Option<u128> {
        self.0.gas_price()
    }
    fn max_fee_per_gas(&self) -> u128 {
        self.0.max_fee_per_gas()
    }
    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        self.0.max_priority_fee_per_gas()
    }
    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        self.0.max_fee_per_blob_gas()
    }
    fn priority_fee_or_price(&self) -> u128 {
        self.0.priority_fee_or_price()
    }
    fn effective_gas_price(&self, base_fee: Option<u64>) -> u128 {
        self.0.effective_gas_price(base_fee)
    }
    fn is_dynamic_fee(&self) -> bool {
        self.0.is_dynamic_fee()
    }
    fn kind(&self) -> TxKind {
        self.0.kind()
    }
    fn is_create(&self) -> bool {
        self.0.is_create()
    }
    fn value(&self) -> U256 {
        self.0.value()
    }
    fn input(&self) -> &Bytes {
        self.0.input()
    }
    fn access_list(&self) -> Option<&AccessList> {
        self.0.access_list()
    }
    fn blob_versioned_hashes(&self) -> Option<&[B256]> {
        self.0.blob_versioned_hashes()
    }
    fn authorization_list(&self) -> Option<&[SignedAuthorization]> {
        self.0.authorization_list()
    }
}
