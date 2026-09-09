use alloy_primitives::{B256, Bytes};
use clap::Parser;
use cosmos_auth::{GENESIS_HASH, MAX_RAW_TX};
use jsonrpsee::{RpcModule, types::ErrorObjectOwned};
use reth_cosmos_dev::{consensus::CosmosConsensus, evm::CosmosEvmConfig, node::CosmosNode};
use reth_ethereum::{
    cli::{chainspec::EthereumChainSpecParser, interface::Cli},
    pool::TransactionPool,
    rpc::api::eth::helpers::EthTransactions,
};
use std::sync::Arc;
use tokio::sync::Semaphore;

fn main() -> eyre::Result<()> {
    Cli::<EthereumChainSpecParser>::parse().run_with_components::<CosmosNode>(
        |chain| {
            (
                CosmosEvmConfig::new(chain.clone()),
                Arc::new(CosmosConsensus::new(chain)),
            )
        },
        async move |builder, _| {
            eyre::ensure!(
                builder.config().chain.genesis_hash() == GENESIS_HASH,
                "use the experiment's genesis.json; other chains are unsupported"
            );
            let handle = builder
                .node(CosmosNode)
                .extend_rpc_modules(|ctx| {
                    let eth = ctx.registry.eth_api().clone();
                    let pool = ctx.pool().clone();
                    let permits = Arc::new(Semaphore::new(8));
                    let mut module = RpcModule::new(());
                    module.register_async_method(
                        "cosmos_sendRawTransaction",
                        move |params, _, _| {
                            let eth = eth.clone();
                            let pool = pool.clone();
                            let permits = permits.clone();
                            async move {
                                let _permit = permits
                                    .try_acquire_owned()
                                    .map_err(|_| rpc_error("authorization capacity reached"))?;
                                let raw: String = params.one()?;
                                if raw.len() > 2 + 2 * MAX_RAW_TX || !raw.starts_with("0x7e") {
                                    return Err(rpc_error(
                                        "invalid Cosmos transaction type or size",
                                    ));
                                }
                                if pool.pool_size().total >= 3072 {
                                    return Err(rpc_error("transaction pool is full"));
                                }
                                let raw: Bytes = raw
                                    .parse()
                                    .map_err(|_| rpc_error("invalid transaction hex"))?;
                                cosmos_auth::decode_wire(&raw)
                                    .map_err(|e| rpc_error(&e.to_string()))?;
                                let hash: B256 = eth
                                    .send_raw_transaction(raw)
                                    .await
                                    .map_err(|e| rpc_error(&e.to_string()))?;
                                Ok::<_, ErrorObjectOwned>(hash)
                            }
                        },
                    )?;
                    ctx.modules.merge_configured(module)?;
                    Ok(())
                })
                .launch_with_debug_capabilities()
                .await?;
            handle.wait_for_node_exit().await
        },
    )
}

fn rpc_error(message: &str) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32000, message, None::<()>)
}
