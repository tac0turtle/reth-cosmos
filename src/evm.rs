//! EVM adapter for chain-owned transactions. Ethereum execution remains upstream.
use crate::primitives::{
    Block, CosmosEnvelope, CosmosPrimitives, CosmosTxType, Receipt, Transaction,
};
use alloy_consensus::Header;
use alloy_eips::Decodable2718;
use alloy_evm::{
    EthEvmFactory, FromRecoveredTx, FromTxWithEncoded,
    eth::{
        EthBlockExecutionCtx, EthBlockExecutorFactory,
        receipt_builder::{ReceiptBuilder, ReceiptBuilderCtx},
    },
};
use alloy_primitives::{Address, Bytes, U256};
use alloy_rpc_types_engine::ExecutionData;
use reth_chainspec::{ChainSpec, EthChainSpec, EthereumHardforks};
use reth_evm::{
    ConfigureEngineEvm, ConfigureEvm, Evm, EvmEnv, EvmEnvFor, ExecutableTxIterator,
    ExecutionCtxFor, NextBlockEnvAttributes,
};
use reth_evm_ethereum::{EthBlockAssembler, EthEvmConfig, revm_spec_by_timestamp_and_block_number};
use reth_primitives_traits::{
    SealedBlock, SealedHeader, SignedTransaction, TxTy, constants::MAX_TX_GAS_LIMIT_OSAKA,
};
use reth_storage_errors::any::AnyError;
use revm::{
    context::{BlockEnv, CfgEnv, TxEnv},
    context_interface::block::BlobExcessGasAndPrice,
    primitives::hardfork::SpecId,
};
use std::{borrow::Cow, convert::Infallible, sync::Arc};

#[derive(Clone, Debug)]
pub struct CosmosEvmConfig {
    ethereum: EthEvmConfig,
    factory: EthBlockExecutorFactory<CosmosReceiptBuilder, Arc<ChainSpec>>,
    assembler: EthBlockAssembler,
}
impl CosmosEvmConfig {
    pub fn new(chain: Arc<ChainSpec>) -> Self {
        Self {
            ethereum: EthEvmConfig::new(chain.clone()),
            factory: EthBlockExecutorFactory::new(
                CosmosReceiptBuilder,
                chain.clone(),
                EthEvmFactory::default(),
            ),
            assembler: EthBlockAssembler::new(chain),
        }
    }
    pub fn chain_spec(&self) -> &Arc<ChainSpec> {
        self.ethereum.chain_spec()
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct CosmosReceiptBuilder;
impl ReceiptBuilder for CosmosReceiptBuilder {
    type Transaction = Transaction;
    type Receipt = Receipt;
    fn build_receipt<E: Evm>(&self, ctx: ReceiptBuilderCtx<'_, CosmosTxType, E>) -> Receipt {
        Receipt {
            tx_type: ctx.tx_type,
            success: ctx.result.is_success(),
            cumulative_gas_used: ctx.cumulative_gas_used,
            logs: ctx.result.into_logs(),
        }
    }
}
impl FromRecoveredTx<Transaction> for TxEnv {
    fn from_recovered_tx(tx: &Transaction, caller: Address) -> Self {
        match tx {
            CosmosEnvelope::Ethereum(tx) => Self::from_recovered_tx(tx, caller),
            CosmosEnvelope::Cosmos(tx) => {
                let op = tx.operation();
                assert_eq!(
                    caller, op.sender,
                    "recovered Cosmos caller must match authorization"
                );
                Self {
                    tx_type: 2,
                    caller,
                    gas_limit: op.gas_limit,
                    gas_price: op.max_fee_per_gas,
                    kind: alloy_primitives::TxKind::Call(op.to),
                    value: op.value,
                    data: op.input.clone(),
                    nonce: op.nonce,
                    chain_id: Some(op.chain_id),
                    gas_priority_fee: Some(op.max_priority_fee_per_gas),
                    ..Default::default()
                }
            }
        }
    }
}
impl FromTxWithEncoded<Transaction> for TxEnv {
    fn from_encoded_tx(tx: &Transaction, caller: Address, encoded: Bytes) -> Self {
        match tx {
            CosmosEnvelope::Ethereum(tx) => Self::from_encoded_tx(tx, caller, encoded),
            CosmosEnvelope::Cosmos(_) => Self::from_recovered_tx(tx, caller),
        }
    }
}
impl ConfigureEvm for CosmosEvmConfig {
    type Primitives = CosmosPrimitives;
    type Error = Infallible;
    type NextBlockEnvCtx = NextBlockEnvAttributes;
    type BlockExecutorFactory = EthBlockExecutorFactory<CosmosReceiptBuilder, Arc<ChainSpec>>;
    type BlockAssembler = EthBlockAssembler;
    fn block_executor_factory(&self) -> &Self::BlockExecutorFactory {
        &self.factory
    }
    fn block_assembler(&self) -> &Self::BlockAssembler {
        &self.assembler
    }
    fn evm_env(&self, header: &Header) -> Result<EvmEnv, Self::Error> {
        self.ethereum.evm_env(header)
    }
    fn next_evm_env(
        &self,
        parent: &Header,
        attrs: &NextBlockEnvAttributes,
    ) -> Result<EvmEnv, Self::Error> {
        self.ethereum.next_evm_env(parent, attrs)
    }
    fn context_for_block<'a>(
        &self,
        block: &'a SealedBlock<Block>,
    ) -> Result<EthBlockExecutionCtx<'a>, Self::Error> {
        Ok(EthBlockExecutionCtx {
            tx_count_hint: Some(block.transaction_count()),
            parent_hash: block.header().parent_hash,
            parent_beacon_block_root: block.header().parent_beacon_block_root,
            ommers: &block.body().ommers,
            withdrawals: block
                .body()
                .withdrawals
                .as_ref()
                .map(|w| Cow::Borrowed(w.as_slice())),
            extra_data: block.header().extra_data.clone(),
            slot_number: block.header().slot_number,
        })
    }
    fn context_for_next_block(
        &self,
        parent: &SealedHeader,
        attributes: Self::NextBlockEnvCtx,
    ) -> Result<EthBlockExecutionCtx<'_>, Self::Error> {
        self.ethereum.context_for_next_block(parent, attributes)
    }
}
impl ConfigureEngineEvm<ExecutionData> for CosmosEvmConfig {
    fn evm_env_for_payload(&self, payload: &ExecutionData) -> Result<EvmEnvFor<Self>, Self::Error> {
        let timestamp = payload.payload.timestamp();
        let block_number = payload.payload.block_number();

        let blob_params = self.chain_spec().blob_params_at_timestamp(timestamp);
        let spec =
            revm_spec_by_timestamp_and_block_number(self.chain_spec(), timestamp, block_number);

        // configure evm env based on parent block
        let mut cfg_env = CfgEnv::new()
            .with_chain_id(self.chain_spec().chain().id())
            .with_spec_and_mainnet_gas_params(spec);

        if let Some(blob_params) = &blob_params {
            cfg_env.set_max_blobs_per_tx(blob_params.max_blobs_per_tx);
        }

        if self.chain_spec().is_osaka_active_at_timestamp(timestamp) {
            cfg_env.tx_gas_limit_cap = Some(MAX_TX_GAS_LIMIT_OSAKA);
        }

        // derive the EIP-4844 blob fees from the header's `excess_blob_gas` and the current
        // blobparams
        let blob_excess_gas_and_price =
            payload
                .payload
                .excess_blob_gas()
                .zip(blob_params)
                .map(|(excess_blob_gas, params)| {
                    let blob_gasprice = params.calc_blob_fee(excess_blob_gas);
                    BlobExcessGasAndPrice {
                        excess_blob_gas,
                        blob_gasprice,
                    }
                });

        let block_env = BlockEnv {
            number: U256::from(block_number),
            beneficiary: payload.payload.fee_recipient(),
            timestamp: U256::from(timestamp),
            difficulty: if spec >= SpecId::MERGE {
                U256::ZERO
            } else {
                payload.payload.as_v1().prev_randao.into()
            },
            prevrandao: (spec >= SpecId::MERGE).then(|| payload.payload.as_v1().prev_randao),
            gas_limit: payload.payload.gas_limit(),
            basefee: payload.payload.saturated_base_fee_per_gas(),
            blob_excess_gas_and_price,
            slot_num: payload
                .payload
                .as_v4()
                .map(|v4| v4.slot_number)
                .unwrap_or_default(),
        };

        Ok(EvmEnv { cfg_env, block_env })
    }

    fn context_for_payload<'a>(
        &self,
        payload: &'a ExecutionData,
    ) -> Result<ExecutionCtxFor<'a, Self>, Self::Error> {
        Ok(EthBlockExecutionCtx {
            tx_count_hint: Some(payload.payload.transactions().len()),
            parent_hash: payload.parent_hash(),
            parent_beacon_block_root: payload.sidecar.parent_beacon_block_root(),
            ommers: &[],
            withdrawals: payload
                .payload
                .withdrawals()
                .map(|w| Cow::Borrowed(w.as_slice())),
            extra_data: payload.payload.as_v1().extra_data.clone(),
            slot_number: payload.payload.as_v4().map(|v4| v4.slot_number),
        })
    }

    fn tx_iterator_for_payload(
        &self,
        payload: &ExecutionData,
    ) -> Result<impl ExecutableTxIterator<Self>, Self::Error> {
        let txs = payload.payload.transactions().clone();
        let convert = |tx: Bytes| {
            let tx =
                TxTy::<Self::Primitives>::decode_2718_exact(tx.as_ref()).map_err(AnyError::new)?;
            let signer = tx.try_recover().map_err(AnyError::new)?;
            Ok::<_, AnyError>(tx.with_signer(signer))
        };

        Ok((txs, convert))
    }
}
