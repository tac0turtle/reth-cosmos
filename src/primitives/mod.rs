//! Chain-owned primitives; standard Ethereum envelopes remain upstream types.
mod cosmos;
mod envelope;
mod storage;
mod tx_type;
pub use cosmos::CosmosSigned;
pub use envelope::CosmosEnvelope;
pub use tx_type::CosmosTxType;
pub type Transaction =
    CosmosEnvelope<alloy_consensus::EthereumTxEnvelope<alloy_consensus::TxEip4844>>;
pub type PooledTransaction = CosmosEnvelope<
    alloy_consensus::EthereumTxEnvelope<
        alloy_consensus::TxEip4844WithSidecar<alloy_eips::eip7594::BlobTransactionSidecarVariant>,
    >,
>;
pub type Block = alloy_consensus::Block<Transaction>;
pub type Receipt = alloy_consensus::EthereumReceipt<CosmosTxType>;
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CosmosPrimitives;
impl reth_primitives_traits::NodePrimitives for CosmosPrimitives {
    type Block = Block;
    type BlockHeader = alloy_consensus::Header;
    type BlockBody = alloy_consensus::BlockBody<Transaction>;
    type SignedTx = Transaction;
    type Receipt = Receipt;
}
