// Adapted from Reth v2.4.1 ethereum/node engine adapter (MIT/Apache-2.0).

use crate::primitives::Block;
use alloy_rpc_types_engine::ExecutionData;
pub use alloy_rpc_types_engine::{
    ExecutionPayloadEnvelopeV2, ExecutionPayloadEnvelopeV3, ExecutionPayloadEnvelopeV4,
    ExecutionPayloadV1, PayloadAttributes as EthPayloadAttributes,
};
use reth_chainspec::{EthChainSpec, EthereumHardforks};
use reth_engine_primitives::{EngineApiValidator, PayloadValidator};
use reth_ethereum_payload_builder::EthereumExecutionPayloadValidator;
use reth_node_api::PayloadTypes;
use reth_payload_primitives::{
    EngineApiMessageVersion, EngineObjectValidationError, NewPayloadError, PayloadOrAttributes,
    validate_execution_requests, validate_version_specific_fields,
};
use reth_primitives_traits::SealedBlock;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CosmosEngineValidator<ChainSpec = reth_chainspec::ChainSpec> {
    inner: EthereumExecutionPayloadValidator<ChainSpec>,
}

impl<ChainSpec> CosmosEngineValidator<ChainSpec> {
    pub const fn new(chain_spec: Arc<ChainSpec>) -> Self {
        Self {
            inner: EthereumExecutionPayloadValidator::new(chain_spec),
        }
    }

    #[inline]
    fn chain_spec(&self) -> &ChainSpec {
        self.inner.chain_spec()
    }
}

impl<ChainSpec, Types> PayloadValidator<Types> for CosmosEngineValidator<ChainSpec>
where
    ChainSpec: EthChainSpec + EthereumHardforks + 'static,
    Types: PayloadTypes<ExecutionData = ExecutionData>,
{
    type Block = Block;

    fn convert_payload_to_block(
        &self,
        payload: ExecutionData,
    ) -> Result<SealedBlock<Self::Block>, NewPayloadError> {
        self.inner
            .ensure_well_formed_payload(payload)
            .map_err(Into::into)
    }
}

impl<ChainSpec, Types> EngineApiValidator<Types> for CosmosEngineValidator<ChainSpec>
where
    ChainSpec: EthChainSpec + EthereumHardforks + 'static,
    Types: PayloadTypes<PayloadAttributes = EthPayloadAttributes, ExecutionData = ExecutionData>,
{
    fn validate_version_specific_fields(
        &self,
        version: EngineApiMessageVersion,
        payload_or_attrs: PayloadOrAttributes<'_, Types::ExecutionData, EthPayloadAttributes>,
    ) -> Result<(), EngineObjectValidationError> {
        payload_or_attrs
            .execution_requests()
            .map(|requests| validate_execution_requests(requests))
            .transpose()?;

        validate_version_specific_fields(self.chain_spec(), version, payload_or_attrs)
    }

    fn ensure_well_formed_attributes(
        &self,
        version: EngineApiMessageVersion,
        attributes: &EthPayloadAttributes,
    ) -> Result<(), EngineObjectValidationError> {
        validate_version_specific_fields(
            self.chain_spec(),
            version,
            PayloadOrAttributes::<Types::ExecutionData, EthPayloadAttributes>::PayloadAttributes(
                attributes,
            ),
        )
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CosmosEngineTypes;
impl reth_payload_primitives::PayloadTypes for CosmosEngineTypes {
    type ExecutionData = ExecutionData;
    type BuiltPayload = super::CosmosBuiltPayload;
    type PayloadAttributes = EthPayloadAttributes;
    fn block_to_payload(
        block: SealedBlock<Block>,
        bal: Option<alloy_primitives::Bytes>,
    ) -> ExecutionData {
        let (payload, sidecar) =
            alloy_rpc_types_engine::ExecutionPayload::from_block_unchecked_with_extras(
                block.hash(),
                &block.into_block(),
                bal,
            );
        ExecutionData { payload, sidecar }
    }
}
impl reth_engine_primitives::EngineTypes for CosmosEngineTypes {
    type ExecutionPayloadEnvelopeV1 = ExecutionPayloadV1;
    type ExecutionPayloadEnvelopeV2 = ExecutionPayloadEnvelopeV2;
    type ExecutionPayloadEnvelopeV3 = ExecutionPayloadEnvelopeV3;
    type ExecutionPayloadEnvelopeV4 = ExecutionPayloadEnvelopeV4;
    type ExecutionPayloadEnvelopeV5 = alloy_rpc_types_engine::ExecutionPayloadEnvelopeV5;
    type ExecutionPayloadEnvelopeV6 = alloy_rpc_types_engine::ExecutionPayloadEnvelopeV6;
}
