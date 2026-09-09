// Adapted from Reth v2.4.1 ethereum/payload (MIT/Apache-2.0).
use super::built::CosmosBuiltPayload as EthBuiltPayload;
use crate::evm::CosmosEvmConfig as EthEvmConfig;
use crate::primitives::{CosmosPrimitives as EthPrimitives, Transaction as TransactionSigned};
use alloy_consensus::{BlockHeader, Transaction, transaction::TxHashRef};
use alloy_primitives::{Bytes, U256};
use alloy_rlp::Encodable;
use alloy_rpc_types_engine::PayloadAttributes as EthPayloadAttributes;
use reth_basic_payload_builder::{
    BuildArguments, BuildOutcome, MissingPayloadBehaviour, PayloadBuilder, PayloadConfig,
    is_better_payload,
};
use reth_chainspec::{ChainSpecProvider, EthChainSpec, EthereumHardforks};
use reth_consensus_common::validation::MAX_RLP_BLOCK_SIZE;
use reth_errors::{BlockExecutionError, BlockValidationError, ConsensusError};
use reth_evm::{
    ConfigureEvm, Evm, NextBlockEnvAttributes,
    block::TxResult,
    execute::{BlockBuilder, BlockBuilderOutcome},
};
use reth_execution_cache::{CachedStateMetrics, CachedStateMetricsSource, CachedStateProvider};
use reth_payload_builder::BlobSidecars;
use reth_payload_builder_primitives::PayloadBuilderError;
use reth_payload_primitives::PayloadAttributes;
use reth_primitives_traits::transaction::error::InvalidTransactionError;
use reth_revm::{database::StateProviderDatabase, db::State};
use reth_storage_api::StateProviderFactory;
use reth_transaction_pool::{
    BestTransactions, BestTransactionsAttributes, PoolTransaction, TransactionPool,
    ValidPoolTransaction,
    error::{Eip4844PoolTransactionError, InvalidPoolTransactionError},
};
use revm::context_interface::{Block as _, Cfg as _};
use std::sync::Arc;
use tracing::{debug, trace, warn};

use reth_ethereum_payload_builder::EthereumBuilderConfig;

type BestTransactionsIter<Pool> = Box<
    dyn BestTransactions<Item = Arc<ValidPoolTransaction<<Pool as TransactionPool>::Transaction>>>,
>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CosmosPayloadBuilder<Pool, Client, EvmConfig = EthEvmConfig> {
    client: Client,
    pool: Pool,
    evm_config: EvmConfig,
    builder_config: EthereumBuilderConfig,
}

impl<Pool, Client, EvmConfig> CosmosPayloadBuilder<Pool, Client, EvmConfig> {
    pub const fn new(
        client: Client,
        pool: Pool,
        evm_config: EvmConfig,
        builder_config: EthereumBuilderConfig,
    ) -> Self {
        Self {
            client,
            pool,
            evm_config,
            builder_config,
        }
    }
}

impl<Pool, Client, EvmConfig> PayloadBuilder for CosmosPayloadBuilder<Pool, Client, EvmConfig>
where
    EvmConfig: ConfigureEvm<Primitives = EthPrimitives, NextBlockEnvCtx = NextBlockEnvAttributes>,
    Client: StateProviderFactory + ChainSpecProvider<ChainSpec: EthereumHardforks> + Clone,
    Pool: TransactionPool<Transaction: PoolTransaction<Consensus = TransactionSigned>>,
{
    type Attributes = EthPayloadAttributes;
    type BuiltPayload = EthBuiltPayload;

    fn try_build(
        &self,
        args: BuildArguments<EthPayloadAttributes, EthBuiltPayload>,
    ) -> Result<BuildOutcome<EthBuiltPayload>, PayloadBuilderError> {
        default_ethereum_payload(
            self.evm_config.clone(),
            self.client.clone(),
            self.pool.clone(),
            self.builder_config.clone(),
            args,
            |attributes| self.pool.best_transactions_with_attributes(attributes),
        )
    }

    fn on_missing_payload(
        &self,
        _args: BuildArguments<Self::Attributes, Self::BuiltPayload>,
    ) -> MissingPayloadBehaviour<Self::BuiltPayload> {
        if self.builder_config.await_payload_on_missing {
            MissingPayloadBehaviour::AwaitInProgress
        } else {
            MissingPayloadBehaviour::RaceEmptyPayload
        }
    }

    fn build_empty_payload(
        &self,
        config: PayloadConfig<Self::Attributes>,
    ) -> Result<EthBuiltPayload, PayloadBuilderError> {
        let args = BuildArguments::new(
            Default::default(),
            Default::default(),
            None,
            config,
            Default::default(),
            None,
        );

        default_ethereum_payload(
            self.evm_config.clone(),
            self.client.clone(),
            self.pool.clone(),
            self.builder_config.clone(),
            args,
            |_| -> BestTransactionsIter<Pool> { Box::new(std::iter::empty()) },
        )?
        .into_payload()
        .ok_or_else(|| PayloadBuilderError::MissingPayload)
    }
}

#[inline]
pub fn default_ethereum_payload<EvmConfig, Client, Pool, F>(
    evm_config: EvmConfig,
    client: Client,
    pool: Pool,
    builder_config: EthereumBuilderConfig,
    args: BuildArguments<EthPayloadAttributes, EthBuiltPayload>,
    best_txs: F,
) -> Result<BuildOutcome<EthBuiltPayload>, PayloadBuilderError>
where
    EvmConfig: ConfigureEvm<Primitives = EthPrimitives, NextBlockEnvCtx = NextBlockEnvAttributes>,
    Client: StateProviderFactory + ChainSpecProvider<ChainSpec: EthereumHardforks>,
    Pool: TransactionPool<Transaction: PoolTransaction<Consensus = TransactionSigned>>,
    F: FnOnce(BestTransactionsAttributes) -> BestTransactionsIter<Pool>,
{
    let BuildArguments {
        mut cached_reads,
        execution_cache,
        mut state_root_handle,
        config,
        cancel,
        best_payload,
    } = args;
    let PayloadConfig {
        parent_header,
        attributes,
        payload_id,
        ..
    } = config;
    let skip_state_root = builder_config.skip_state_root;

    let mut state_provider = client.state_by_block_hash(parent_header.hash())?;
    if let Some(execution_cache) = execution_cache {
        state_provider = Box::new(CachedStateProvider::new(
            state_provider,
            execution_cache.cache().clone(),
            Some(CachedStateMetrics::zeroed(
                CachedStateMetricsSource::Builder,
            )),
        ));
    }
    let state = StateProviderDatabase::new(state_provider.as_ref());
    let chain_spec = client.chain_spec();
    let is_amsterdam = chain_spec.is_amsterdam_active_at_timestamp(attributes.timestamp());
    let mut db = State::builder()
        .with_database(cached_reads.as_db_mut(state))
        .with_bundle_update()
        .with_bal_builder_if(is_amsterdam)
        .build();

    let evm_config = evm_config.with_jit_support();
    let mut builder = evm_config
        .builder_for_next_block(
            &mut db,
            &parent_header,
            NextBlockEnvAttributes {
                timestamp: attributes.timestamp(),
                suggested_fee_recipient: attributes.suggested_fee_recipient,
                prev_randao: attributes.prev_randao,
                gas_limit: builder_config
                    .gas_limit_with_target(parent_header.gas_limit, attributes.target_gas_limit()),
                parent_beacon_block_root: attributes.parent_beacon_block_root(),
                withdrawals: attributes.withdrawals.clone().map(Into::into),
                extra_data: builder_config.extra_data.clone(),
                slot_number: attributes.slot_number(),
            },
        )
        .map_err(PayloadBuilderError::other)?;

    debug!(target: "payload_builder", id=%payload_id, parent_header = ?parent_header.hash(), parent_number = parent_header.number, "building new payload");
    let mut cumulative_tx_gas_used = 0;
    let mut block_regular_gas_used = 0;
    let mut block_state_gas_used = 0;
    let block_gas_limit: u64 = builder.evm_mut().block().gas_limit();
    let tx_gas_limit_cap = builder.evm_mut().cfg_env().tx_gas_limit_cap();
    let base_fee = builder.evm_mut().block().basefee();

    let mut best_txs = best_txs(BestTransactionsAttributes::new(
        base_fee,
        builder
            .evm_mut()
            .block()
            .blob_gasprice()
            .map(|gasprice| gasprice as u64),
    ));
    let mut total_fees = U256::ZERO;

    if let Some(task) = state_root_handle.as_mut() {
        builder
            .evm_mut()
            .db_mut()
            .set_state_hook(Some(Box::new(task.take_state_hook())));
    }

    builder.apply_pre_execution_changes().map_err(|err| {
        warn!(target: "payload_builder", %err, "failed to apply pre-execution changes");
        PayloadBuilderError::Internal(err.into())
    })?;

    let mut blob_sidecars = BlobSidecars::Empty;

    let mut block_blob_count = 0;
    let mut block_transactions_rlp_length = 0;

    let blob_params = chain_spec.blob_params_at_timestamp(attributes.timestamp);
    let protocol_max_blob_count = blob_params
        .as_ref()
        .map(|params| params.max_blob_count)
        .unwrap_or_else(Default::default);

    let max_blob_count = builder_config
        .max_blobs_per_block
        .map(|user_limit| std::cmp::min(user_limit, protocol_max_blob_count).max(1))
        .unwrap_or(protocol_max_blob_count);

    let is_osaka = chain_spec.is_osaka_active_at_timestamp(attributes.timestamp);

    let withdrawals_rlp_length = attributes
        .withdrawals
        .as_ref()
        .map(|withdrawals| withdrawals.length())
        .unwrap_or(0);

    while let Some(pool_tx) = best_txs.next() {
        let exceeds_gas_limit = if is_amsterdam {
            let regular_available_gas = block_gas_limit.saturating_sub(block_regular_gas_used);
            let state_available_gas = block_gas_limit.saturating_sub(block_state_gas_used);
            let regular_tx_gas_limit = pool_tx.gas_limit().min(tx_gas_limit_cap);

            if regular_tx_gas_limit > regular_available_gas {
                Some((regular_tx_gas_limit, regular_available_gas))
            } else if pool_tx.gas_limit() > state_available_gas {
                Some((pool_tx.gas_limit(), state_available_gas))
            } else {
                None
            }
        } else {
            let block_available_gas = block_gas_limit.saturating_sub(cumulative_tx_gas_used);
            (pool_tx.gas_limit() > block_available_gas)
                .then_some((pool_tx.gas_limit(), block_available_gas))
        };

        if let Some((transaction_gas_limit, block_available_gas)) = exceeds_gas_limit {
            best_txs.mark_invalid(
                &pool_tx,
                InvalidPoolTransactionError::ExceedsGasLimit(
                    transaction_gas_limit,
                    block_available_gas,
                ),
            );
            continue;
        }

        if cancel.is_cancelled() {
            return Ok(BuildOutcome::Cancelled);
        }

        let tx = pool_tx.to_consensus();

        let tx_rlp_len = tx.inner().length();

        let estimated_block_size_with_tx =
            block_transactions_rlp_length + tx_rlp_len + withdrawals_rlp_length + 1024; // 1Kb of overhead for the block header

        if is_osaka && estimated_block_size_with_tx > MAX_RLP_BLOCK_SIZE {
            best_txs.mark_invalid(
                &pool_tx,
                InvalidPoolTransactionError::OversizedData {
                    size: estimated_block_size_with_tx,
                    limit: MAX_RLP_BLOCK_SIZE,
                },
            );
            continue;
        }

        let mut blob_tx_sidecar = None;
        let tx_blob_count = tx.blob_count();

        if let Some(tx_blob_count) = tx_blob_count {
            if block_blob_count + tx_blob_count > max_blob_count {
                trace!(target: "payload_builder", tx=?tx.tx_hash(), ?block_blob_count, "skipping blob transaction because it would exceed the max blob count per block");
                best_txs.mark_invalid(
                    &pool_tx,
                    InvalidPoolTransactionError::Eip4844(
                        Eip4844PoolTransactionError::TooManyEip4844Blobs {
                            have: block_blob_count + tx_blob_count,
                            permitted: max_blob_count,
                        },
                    ),
                );
                continue;
            }

            let blob_sidecar_result = 'sidecar: {
                let Some(sidecar) = pool
                    .get_blob(*tx.tx_hash())
                    .map_err(PayloadBuilderError::other)?
                else {
                    break 'sidecar Err(Eip4844PoolTransactionError::MissingEip4844BlobSidecar);
                };

                if is_osaka {
                    if sidecar.is_eip7594() {
                        Ok(sidecar)
                    } else {
                        Err(Eip4844PoolTransactionError::UnexpectedEip4844SidecarAfterOsaka)
                    }
                } else if sidecar.is_eip4844() {
                    Ok(sidecar)
                } else {
                    Err(Eip4844PoolTransactionError::UnexpectedEip7594SidecarBeforeOsaka)
                }
            };

            blob_tx_sidecar = match blob_sidecar_result {
                Ok(sidecar) => Some(sidecar),
                Err(error) => {
                    best_txs.mark_invalid(&pool_tx, InvalidPoolTransactionError::Eip4844(error));
                    continue;
                }
            };
        }

        let miner_fee = tx.effective_tip_per_gas(base_fee);
        let tx_hash = *tx.tx_hash();

        let mut tx_regular_gas_used = 0;
        let gas_output = match builder.execute_transaction_with_result_closure(tx, |result| {
            tx_regular_gas_used = result.result().result.gas().block_regular_gas_used();
        }) {
            Ok(gas_output) => gas_output,
            Err(BlockExecutionError::Validation(BlockValidationError::InvalidTx {
                error, ..
            })) => {
                if error.is_nonce_too_low() {
                    trace!(target: "payload_builder", %error, ?tx_hash, "skipping nonce too low transaction");
                } else {
                    trace!(target: "payload_builder", %error, ?tx_hash, "skipping invalid transaction and its descendants");
                    best_txs.mark_invalid(
                        &pool_tx,
                        InvalidPoolTransactionError::Consensus(
                            InvalidTransactionError::TxTypeNotSupported,
                        ),
                    );
                }
                continue;
            }
            Err(BlockExecutionError::Validation(
                BlockValidationError::TransactionGasLimitMoreThanAvailableBlockGas {
                    transaction_gas_limit,
                    block_available_gas,
                },
            )) => {
                trace!(target: "payload_builder", %transaction_gas_limit, %block_available_gas, ?tx_hash, "skipping transaction exceeding block gas limit");
                best_txs.mark_invalid(
                    &pool_tx,
                    InvalidPoolTransactionError::ExceedsGasLimit(
                        transaction_gas_limit,
                        block_available_gas,
                    ),
                );
                continue;
            }
            Err(err) => return Err(PayloadBuilderError::evm(err)),
        };

        if let Some(blob_count) = tx_blob_count {
            block_blob_count += blob_count;

            if block_blob_count == max_blob_count {
                best_txs.skip_blobs();
            }
        }

        block_transactions_rlp_length += tx_rlp_len;

        let gas_used = gas_output.tx_gas_used();
        let miner_fee = miner_fee.expect("fee is always valid; execution succeeded");
        total_fees += U256::from(miner_fee) * U256::from(gas_used);
        cumulative_tx_gas_used += gas_used;
        block_regular_gas_used += tx_regular_gas_used;
        block_state_gas_used += gas_output.state_gas_used();

        if let Some(sidecar) = blob_tx_sidecar {
            blob_sidecars.push_sidecar_variant(sidecar.as_ref().clone());
        }
    }

    if !is_better_payload(best_payload.as_ref(), total_fees) {
        drop(builder);
        return Ok(BuildOutcome::Aborted {
            fees: total_fees,
            cached_reads,
        });
    }

    let BlockBuilderOutcome {
        execution_result,
        block,
        block_access_list,
        ..
    } = if skip_state_root {
        debug!(
            target: "payload_builder",
            id = %payload_id,
            state_root = ?parent_header.state_root(),
            "skipping payload state-root computation"
        );
        builder.finish(
            state_provider.as_ref(),
            Some((parent_header.state_root(), Default::default())),
        )?
    } else if let Some(mut task) = state_root_handle {
        builder.evm_mut().db_mut().set_state_hook(None);

        match task.state_root() {
            Ok(outcome) => {
                debug!(target: "payload_builder", id=%payload_id, state_root=?outcome.state_root, job = task.name(), "received state root from state-root job");
                builder.finish(
                    state_provider.as_ref(),
                    Some((
                        outcome.state_root,
                        Arc::unwrap_or_clone(outcome.trie_updates),
                    )),
                )?
            }
            Err(err) => {
                warn!(target: "payload_builder", id=%payload_id, %err, "state-root job failed, falling back to sync state root");
                builder.finish(state_provider.as_ref(), None)?
            }
        }
    } else {
        builder.finish(state_provider.as_ref(), None)?
    };

    let requests = chain_spec
        .is_prague_active_at_timestamp(attributes.timestamp)
        .then_some(execution_result.requests);

    debug!(target: "payload_builder", id=%payload_id, sealed_block_header = ?block.sealed_header(), "sealed built block");

    if is_osaka && block.rlp_length() > MAX_RLP_BLOCK_SIZE {
        return Err(PayloadBuilderError::other(ConsensusError::BlockTooLarge {
            rlp_length: block.rlp_length(),
            max_rlp_length: MAX_RLP_BLOCK_SIZE,
        }));
    }

    let block_access_list: Option<Bytes> =
        block_access_list.map(|block_access_list| alloy_rlp::encode(&block_access_list).into());
    let payload = EthBuiltPayload::new(Arc::new(block), total_fees, requests, block_access_list)
        .with_sidecars(blob_sidecars);

    Ok(BuildOutcome::Better {
        payload,
        cached_reads,
    })
}
