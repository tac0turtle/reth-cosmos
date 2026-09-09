//! Cosmos authorization is a consensus rule, independent of pool admission.
use crate::primitives::{Block, CosmosEnvelope, CosmosPrimitives, Receipt};
use alloy_consensus::{BlockBody, Header};
use alloy_primitives::B256;
use reth_chainspec::{ChainSpec, EthChainSpec};
use reth_consensus::{
    Consensus, ConsensusError, FullConsensus, HeaderValidator, ReceiptRootBloom, TransactionRoot,
};
use reth_ethereum_consensus::EthBeaconConsensus;
use reth_execution_types::BlockExecutionResult;
use reth_primitives_traits::{RecoveredBlock, SealedBlock, SealedHeader};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CosmosConsensus(EthBeaconConsensus<ChainSpec>);
impl CosmosConsensus {
    pub fn new(chain: Arc<ChainSpec>) -> Self {
        Self(EthBeaconConsensus::new(chain))
    }
    fn verify_authorizations(&self, block: &SealedBlock<Block>) -> Result<(), ConsensusError> {
        if self.0.chain_spec().genesis_hash() != cosmos_auth::GENESIS_HASH {
            return Err(ConsensusError::TransactionSignerRecoveryError);
        }
        for tx in &block.body().transactions {
            if let CosmosEnvelope::Cosmos(tx) = tx {
                tx.verify()
                    .map_err(|_| ConsensusError::TransactionSignerRecoveryError)?;
            }
        }
        Ok(())
    }
}
impl HeaderValidator for CosmosConsensus {
    fn validate_header(&self, header: &SealedHeader) -> Result<(), ConsensusError> {
        self.0.validate_header(header)
    }
    fn validate_header_against_parent(
        &self,
        header: &SealedHeader,
        parent: &SealedHeader,
    ) -> Result<(), ConsensusError> {
        self.0.validate_header_against_parent(header, parent)
    }
}
impl Consensus<Block> for CosmosConsensus {
    fn validate_body_against_header(
        &self,
        body: &BlockBody<crate::primitives::Transaction>,
        header: &SealedHeader<Header>,
    ) -> Result<(), ConsensusError> {
        <EthBeaconConsensus<ChainSpec> as Consensus<Block>>::validate_body_against_header(
            &self.0, body, header,
        )
    }
    fn validate_block_pre_execution(
        &self,
        block: &SealedBlock<Block>,
    ) -> Result<(), ConsensusError> {
        self.validate_block_pre_execution_with_tx_root(block, None)
    }
    fn validate_block_pre_execution_with_tx_root(
        &self,
        block: &SealedBlock<Block>,
        root: Option<TransactionRoot>,
    ) -> Result<(), ConsensusError> {
        self.verify_authorizations(block)?;
        self.0
            .validate_block_pre_execution_with_tx_root(block, root)
    }
}
impl FullConsensus<CosmosPrimitives> for CosmosConsensus {
    fn validate_block_post_execution(
        &self,
        block: &RecoveredBlock<Block>,
        result: &BlockExecutionResult<Receipt>,
        bloom: Option<ReceiptRootBloom>,
        bal: Option<B256>,
    ) -> Result<(), ConsensusError> {
        <EthBeaconConsensus<ChainSpec> as FullConsensus<CosmosPrimitives>>::validate_block_post_execution(&self.0, block, result, bloom, bal)
    }
}
