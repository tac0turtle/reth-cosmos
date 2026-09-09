//! Assemble Reth using chain-owned primitives and component adapters.
use crate::{
    consensus::CosmosConsensus,
    evm::CosmosEvmConfig,
    payload::{CosmosEngineTypes, CosmosEngineValidator, CosmosPayloadBuilder},
    pool::CosmosPoolTransaction,
    primitives::{Block, CosmosPrimitives, Transaction},
    rpc::{Converter, CosmosRpcTypes, ReceiptConverter, receipt_converter},
};
use reth_chainspec::{ChainSpec, ChainSpecProvider};
use reth_engine_local::LocalPayloadAttributesBuilder;
use reth_ethereum_payload_builder::EthereumBuilderConfig;
use reth_evm::{ConfigureEvm, NextBlockEnvAttributes};
use reth_node_api::{
    AddOnsContext, FullNodeComponents, FullNodeTypes, NodeTypes, PayloadAttributesBuilder,
};
use reth_node_builder::{
    BuilderContext, DebugNode, Node, NodeAdapter, PayloadBuilderConfig,
    components::{
        BasicPayloadServiceBuilder, ComponentsBuilder, ConsensusBuilder, ExecutorBuilder,
        PayloadBuilderBuilder, PoolBuilder, TxPoolBuilder,
    },
    rpc::{EthApiBuilder, EthApiCtx, PayloadValidatorBuilder, RpcAddOns},
};
use reth_node_ethereum::node::EthereumNetworkBuilder;
use reth_provider::EthStorage;
use reth_transaction_pool::{
    EthTransactionPool, PoolTransaction, TransactionPool, TransactionValidationTaskExecutor,
    blobstore::DiskFileBlobStore,
};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct CosmosNode;
impl NodeTypes for CosmosNode {
    type Primitives = CosmosPrimitives;
    type ChainSpec = ChainSpec;
    type Storage = EthStorage<Transaction>;
    type Payload = CosmosEngineTypes;
}
impl<N: FullNodeTypes<Types = Self>> Node<N> for CosmosNode {
    type ComponentsBuilder = ComponentsBuilder<
        N,
        CosmosPoolBuilder,
        BasicPayloadServiceBuilder<CosmosPayloadBuilderBuilder>,
        EthereumNetworkBuilder,
        CosmosExecutorBuilder,
        CosmosConsensusBuilder,
    >;
    type AddOns = RpcAddOns<NodeAdapter<N>, CosmosEthApiBuilder, CosmosEngineValidatorBuilder>;
    fn components_builder(&self) -> Self::ComponentsBuilder {
        ComponentsBuilder::default()
            .node_types::<N>()
            .pool(CosmosPoolBuilder)
            .executor(CosmosExecutorBuilder)
            .payload(BasicPayloadServiceBuilder::default())
            .network(EthereumNetworkBuilder::default())
            .consensus(CosmosConsensusBuilder)
    }
    fn add_ons(&self) -> Self::AddOns {
        Self::AddOns::default()
    }
}
impl<N: FullNodeComponents<Types = Self>> DebugNode<N> for CosmosNode {
    type RpcBlock = alloy_rpc_types_eth::Block<crate::rpc::RpcTransaction>;
    fn rpc_to_primitive_block(block: Self::RpcBlock) -> Block {
        block
            .into_consensus()
            .map_transactions(|tx| tx.inner.into_inner())
    }
    fn local_payload_attributes_builder(
        chain: &ChainSpec,
    ) -> impl PayloadAttributesBuilder<alloy_rpc_types_engine::PayloadAttributes> {
        LocalPayloadAttributesBuilder::new(Arc::new(chain.clone()))
    }
}
#[derive(Clone, Debug, Default)]
pub struct CosmosExecutorBuilder;
impl<N: FullNodeTypes<Types = CosmosNode>> ExecutorBuilder<N> for CosmosExecutorBuilder {
    type EVM = CosmosEvmConfig;
    async fn build_evm(self, ctx: &BuilderContext<N>) -> eyre::Result<Self::EVM> {
        eyre::ensure!(
            !ctx.config().jit.enabled,
            "JIT is not configured for this experiment"
        );
        Ok(CosmosEvmConfig::new(ctx.chain_spec()))
    }
}
#[derive(Clone, Debug, Default)]
pub struct CosmosConsensusBuilder;
impl<N: FullNodeTypes<Types = CosmosNode>> ConsensusBuilder<N> for CosmosConsensusBuilder {
    type Consensus = Arc<CosmosConsensus>;
    async fn build_consensus(self, ctx: &BuilderContext<N>) -> eyre::Result<Self::Consensus> {
        Ok(Arc::new(CosmosConsensus::new(ctx.chain_spec())))
    }
}
#[derive(Clone, Debug, Default)]
pub struct CosmosPoolBuilder;
impl<N, Evm> PoolBuilder<N, Evm> for CosmosPoolBuilder
where
    N: FullNodeTypes<Types = CosmosNode>,
    Evm: ConfigureEvm<Primitives = CosmosPrimitives> + 'static,
{
    type Pool = EthTransactionPool<N::Provider, DiskFileBlobStore, Evm, CosmosPoolTransaction>;
    async fn build_pool(self, ctx: &BuilderContext<N>, evm: Evm) -> eyre::Result<Self::Pool> {
        let config = ctx.pool_config();
        let blobs_disabled = ctx.config().txpool.disable_blobs_support
            || ctx.config().txpool.blobpool_max_count == 0;
        let blobs = reth_node_builder::components::create_blob_store_with_cache(
            ctx,
            config.blob_cache_size,
        )?;
        let validator = TransactionValidationTaskExecutor::eth_builder(ctx.provider().clone(), evm)
            .with_custom_tx_type(cosmos_auth::TX_TYPE)
            .set_eip4844(!blobs_disabled)
            .kzg_settings(ctx.kzg_settings()?)
            .with_max_tx_input_bytes(ctx.config().txpool.max_tx_input_bytes)
            .with_local_transactions_config(config.local_transactions_config.clone())
            .set_tx_fee_cap(ctx.config().rpc.rpc_tx_fee_cap)
            .with_max_tx_gas_limit(ctx.config().txpool.max_tx_gas_limit)
            .with_minimum_priority_fee(ctx.config().txpool.minimum_priority_fee)
            .with_additional_tasks(ctx.config().txpool.additional_validation_tasks)
            .build_with_tasks(ctx.task_executor().clone(), blobs.clone());
        TxPoolBuilder::new(ctx)
            .with_validator(validator)
            .build_and_spawn_maintenance_task(blobs, config)
    }
}
#[derive(Clone, Debug, Default)]
pub struct CosmosPayloadBuilderBuilder;
impl<N, Pool, Evm> PayloadBuilderBuilder<N, Pool, Evm> for CosmosPayloadBuilderBuilder
where
    N: FullNodeTypes<Types = CosmosNode>,
    Pool: TransactionPool<Transaction: PoolTransaction<Consensus = Transaction>> + Unpin + 'static,
    Evm: ConfigureEvm<Primitives = CosmosPrimitives, NextBlockEnvCtx = NextBlockEnvAttributes>
        + 'static,
{
    type PayloadBuilder = CosmosPayloadBuilder<Pool, N::Provider, Evm>;
    async fn build_payload_builder(
        self,
        ctx: &BuilderContext<N>,
        pool: Pool,
        evm: Evm,
    ) -> eyre::Result<Self::PayloadBuilder> {
        Ok(CosmosPayloadBuilder::new(
            ctx.provider().clone(),
            pool,
            evm,
            EthereumBuilderConfig::new().with_extra_data(ctx.payload_builder_config().extra_data()),
        ))
    }
}
#[derive(Clone, Debug, Default)]
pub struct CosmosEthApiBuilder;
impl<N: FullNodeComponents<Types = CosmosNode, Evm = CosmosEvmConfig>> EthApiBuilder<N>
    for CosmosEthApiBuilder
{
    type EthApi = reth_rpc::EthApi<N, Converter>;
    async fn build_eth_api(self, ctx: EthApiCtx<'_, N>) -> eyre::Result<Self::EthApi> {
        let chain = ctx.components.provider().chain_spec();
        let converter = reth_rpc_convert::RpcConverter::<
            CosmosRpcTypes,
            CosmosEvmConfig,
            ReceiptConverter,
        >::new(
            reth_rpc_eth_types::receipt::EthReceiptConverter::new(chain)
                .with_builder(receipt_converter as fn(_, _, _) -> _),
        );
        Ok(ctx.eth_api_builder().map_converter(|_| converter).build())
    }
}
#[derive(Clone, Debug, Default)]
pub struct CosmosEngineValidatorBuilder;
impl<N: FullNodeComponents<Types = CosmosNode>> PayloadValidatorBuilder<N>
    for CosmosEngineValidatorBuilder
{
    type Validator = CosmosEngineValidator;
    async fn build(self, ctx: &AddOnsContext<'_, N>) -> eyre::Result<Self::Validator> {
        Ok(CosmosEngineValidator::new(ctx.config.chain.clone()))
    }
}
