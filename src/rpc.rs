//! RPC types use upstream generic containers with the chain-owned transaction envelope.
use crate::primitives::{CosmosEnvelope, CosmosTxType, Receipt, Transaction};
use alloy_consensus::{EthereumReceipt, ReceiptWithBloom, TxReceipt, error::ValueError};
use alloy_network::TxSigner;
use alloy_primitives::Signature;
use alloy_rpc_types_eth::{Log, TransactionRequest};
use reth_chainspec::ChainSpec;
use reth_primitives_traits::TransactionMeta;
use reth_rpc_convert::{
    RpcConverter, RpcTypes, SignTxRequestError, SignableTxRequest, TryIntoSimTx,
};
use reth_rpc_eth_types::receipt::EthReceiptConverter;

#[derive(Clone, Debug)]
pub struct CosmosRpcTypes;
pub type RpcTransaction = alloy_rpc_types_eth::Transaction<Transaction>;
pub type RpcReceiptInner = ReceiptWithBloom<EthereumReceipt<CosmosTxType, Log>>;
impl RpcTypes for CosmosRpcTypes {
    type Log = Log;
    type Header = alloy_rpc_types_eth::Header;
    type Receipt = alloy_rpc_types_eth::TransactionReceipt<RpcReceiptInner>;
    type TransactionResponse = RpcTransaction;
    type TransactionRequest = TransactionRequest;
}
pub type ReceiptConverter =
    EthReceiptConverter<ChainSpec, fn(Receipt, usize, TransactionMeta) -> RpcReceiptInner>;
pub type Converter = RpcConverter<CosmosRpcTypes, crate::evm::CosmosEvmConfig, ReceiptConverter>;

pub fn receipt_converter(
    receipt: Receipt,
    first_log: usize,
    meta: TransactionMeta,
) -> RpcReceiptInner {
    let bloom = receipt.bloom();
    let mut next_log = first_log;
    ReceiptWithBloom {
        logs_bloom: bloom,
        receipt: receipt.map_logs(|log| {
            let index = next_log;
            next_log += 1;
            Log {
                inner: log,
                block_hash: Some(meta.block_hash),
                block_number: Some(meta.block_number),
                block_timestamp: Some(meta.timestamp),
                transaction_hash: Some(meta.tx_hash),
                transaction_index: Some(meta.index),
                log_index: Some(index as u64),
                removed: false,
            }
        }),
    }
}
impl SignableTxRequest<Transaction> for TransactionRequest {
    async fn try_build_and_sign(
        self,
        signer: impl TxSigner<Signature> + Send,
    ) -> Result<Transaction, SignTxRequestError> {
        if self.transaction_type == Some(cosmos_auth::TX_TYPE) {
            return Err(SignTxRequestError::InvalidTransactionRequest);
        }
        <Self as SignableTxRequest<
            alloy_consensus::EthereumTxEnvelope<alloy_consensus::TxEip4844>,
        >>::try_build_and_sign(self, signer)
        .await
        .map(CosmosEnvelope::Ethereum)
    }
}
impl TryIntoSimTx<Transaction> for TransactionRequest {
    fn try_into_sim_tx(self) -> Result<Transaction, ValueError<Self>> {
        if self.transaction_type == Some(cosmos_auth::TX_TYPE) {
            return Err(ValueError::new_static(
                self,
                "Cosmos authorization requires a signed raw transaction",
            ));
        }
        self.build_typed_simulate_transaction()
            .map(CosmosEnvelope::Ethereum)
    }
}
