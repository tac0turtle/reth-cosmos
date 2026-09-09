// Adapted from Reth v2.4.1 ethereum/engine-primitives (MIT/Apache-2.0).

use crate::primitives::CosmosPrimitives as EthPrimitives;
use alloy_eips::eip7685::Requests;
use alloy_primitives::{Bytes, U256};
use alloy_rpc_types_engine::{
    BlobsBundleV1, BlobsBundleV2, CancunPayloadFields, ExecutionData, ExecutionPayload,
    ExecutionPayloadEnvelopeV2, ExecutionPayloadEnvelopeV3, ExecutionPayloadEnvelopeV4,
    ExecutionPayloadEnvelopeV5, ExecutionPayloadEnvelopeV6, ExecutionPayloadFieldV2,
    ExecutionPayloadSidecar, ExecutionPayloadV1, ExecutionPayloadV3, ExecutionPayloadV4,
    PraguePayloadFields,
};
use reth_ethereum_engine_primitives::{BlobSidecars, BuiltPayloadConversionError};
use reth_payload_primitives::BuiltPayload;
use reth_primitives_traits::{NodePrimitives, RecoveredBlock, SealedBlock};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CosmosBuiltPayload<N: NodePrimitives = EthPrimitives> {
    pub(crate) block: Arc<RecoveredBlock<N::Block>>,
    pub(crate) fees: U256,
    pub(crate) sidecars: BlobSidecars,
    pub(crate) requests: Option<Requests>,
    pub(crate) block_access_list: Option<Bytes>,
}

impl<N: NodePrimitives> CosmosBuiltPayload<N> {
    pub const fn new(
        block: Arc<RecoveredBlock<N::Block>>,
        fees: U256,
        requests: Option<Requests>,
        block_access_list: Option<Bytes>,
    ) -> Self {
        Self {
            block,
            fees,
            requests,
            sidecars: BlobSidecars::Empty,
            block_access_list,
        }
    }

    pub fn block(&self) -> &SealedBlock<N::Block> {
        self.block.sealed_block()
    }

    pub fn recovered_block(&self) -> &RecoveredBlock<N::Block> {
        &self.block
    }

    pub const fn block_arc(&self) -> &Arc<RecoveredBlock<N::Block>> {
        &self.block
    }

    pub fn into_block_arc(self) -> Arc<RecoveredBlock<N::Block>> {
        self.block
    }

    pub const fn fees(&self) -> U256 {
        self.fees
    }

    pub const fn sidecars(&self) -> &BlobSidecars {
        &self.sidecars
    }

    pub fn with_sidecars(mut self, sidecars: impl Into<BlobSidecars>) -> Self {
        self.sidecars = sidecars.into();
        self
    }
}

impl CosmosBuiltPayload {
    pub fn try_into_v3(self) -> Result<ExecutionPayloadEnvelopeV3, BuiltPayloadConversionError> {
        let Self {
            block,
            fees,
            sidecars,
            ..
        } = self;

        let blobs_bundle = match sidecars {
            BlobSidecars::Empty => BlobsBundleV1::empty(),
            BlobSidecars::Eip4844(sidecars) => BlobsBundleV1::from(sidecars),
            BlobSidecars::Eip7594(_) => {
                return Err(BuiltPayloadConversionError::UnexpectedEip7594Sidecars);
            }
        };

        Ok(ExecutionPayloadEnvelopeV3 {
            execution_payload: ExecutionPayloadV3::from_block_unchecked(
                block.hash(),
                &Arc::unwrap_or_clone(block).into_block(),
            ),
            block_value: fees,
            should_override_builder: false,
            blobs_bundle,
        })
    }

    pub fn try_into_v4(
        mut self,
    ) -> Result<ExecutionPayloadEnvelopeV4, BuiltPayloadConversionError> {
        let execution_requests = self.requests.take().unwrap_or_default();
        Ok(ExecutionPayloadEnvelopeV4 {
            execution_requests,
            envelope_inner: self.try_into()?,
        })
    }

    pub fn try_into_v5(self) -> Result<ExecutionPayloadEnvelopeV5, BuiltPayloadConversionError> {
        let Self {
            block,
            fees,
            sidecars,
            requests,
            ..
        } = self;

        let blobs_bundle = match sidecars {
            BlobSidecars::Empty => BlobsBundleV2::empty(),
            BlobSidecars::Eip7594(sidecars) => BlobsBundleV2::from(sidecars),
            BlobSidecars::Eip4844(_) => {
                return Err(BuiltPayloadConversionError::UnexpectedEip4844Sidecars);
            }
        };

        Ok(ExecutionPayloadEnvelopeV5 {
            execution_payload: ExecutionPayloadV3::from_block_unchecked(
                block.hash(),
                &Arc::unwrap_or_clone(block).into_block(),
            ),
            block_value: fees,
            should_override_builder: false,
            blobs_bundle,
            execution_requests: requests.unwrap_or_default(),
        })
    }

    pub fn try_into_v6(self) -> Result<ExecutionPayloadEnvelopeV6, BuiltPayloadConversionError> {
        let Self {
            block,
            fees,
            sidecars,
            requests,
            block_access_list,
            ..
        } = self;

        let block_access_list =
            block_access_list.ok_or(BuiltPayloadConversionError::MissingBlockAccessList)?;

        let blobs_bundle = match sidecars {
            BlobSidecars::Empty => BlobsBundleV2::empty(),
            BlobSidecars::Eip7594(sidecars) => BlobsBundleV2::from(sidecars),
            BlobSidecars::Eip4844(_) => {
                return Err(BuiltPayloadConversionError::UnexpectedEip4844Sidecars);
            }
        };
        Ok(ExecutionPayloadEnvelopeV6 {
            execution_payload: ExecutionPayloadV4::from_block_unchecked_with_bal(
                block.hash(),
                &Arc::unwrap_or_clone(block).into_block(),
                block_access_list,
            ),
            block_value: fees,
            should_override_builder: false,
            blobs_bundle,
            execution_requests: requests.unwrap_or_default(),
        })
    }

    pub fn into_execution_data(self) -> ExecutionData {
        let Self {
            block,
            requests,
            block_access_list,
            ..
        } = self;
        let block_hash = block.hash();
        let block = Arc::unwrap_or_clone(block).into_block();

        let (payload, sidecar) = ExecutionPayload::from_block_unchecked_with_extras(
            block_hash,
            &block,
            block_access_list,
        );

        let sidecar = if let Some(requests) = requests {
            block
                .header
                .parent_beacon_block_root
                .map_or(sidecar, |parent_beacon_block_root| {
                    ExecutionPayloadSidecar::v4(
                        CancunPayloadFields {
                            parent_beacon_block_root,
                            versioned_hashes: block
                                .body
                                .blob_versioned_hashes_iter()
                                .copied()
                                .collect(),
                        },
                        PraguePayloadFields::new(requests),
                    )
                })
        } else {
            sidecar
        };

        ExecutionData::new(payload, sidecar)
    }
}

impl<N: NodePrimitives> BuiltPayload for CosmosBuiltPayload<N> {
    type Primitives = N;

    fn block(&self) -> &SealedBlock<N::Block> {
        self.block.sealed_block()
    }

    fn fees(&self) -> U256 {
        self.fees
    }

    fn block_access_list(&self) -> Option<&Bytes> {
        self.block_access_list.as_ref()
    }

    fn requests(&self) -> Option<Requests> {
        self.requests.clone()
    }
}

impl From<CosmosBuiltPayload> for ExecutionPayloadV1 {
    fn from(value: CosmosBuiltPayload) -> Self {
        Self::from_block_unchecked(
            value.block().hash(),
            &Arc::unwrap_or_clone(value.block).into_block(),
        )
    }
}

impl From<CosmosBuiltPayload> for ExecutionPayloadEnvelopeV2 {
    fn from(value: CosmosBuiltPayload) -> Self {
        let CosmosBuiltPayload { block, fees, .. } = value;

        Self {
            block_value: fees,
            execution_payload: ExecutionPayloadFieldV2::from_block_unchecked(
                block.hash(),
                &Arc::unwrap_or_clone(block).into_block(),
            ),
        }
    }
}

impl TryFrom<CosmosBuiltPayload> for ExecutionPayloadEnvelopeV3 {
    type Error = BuiltPayloadConversionError;

    fn try_from(value: CosmosBuiltPayload) -> Result<Self, Self::Error> {
        value.try_into_v3()
    }
}

impl TryFrom<CosmosBuiltPayload> for ExecutionPayloadEnvelopeV4 {
    type Error = BuiltPayloadConversionError;

    fn try_from(value: CosmosBuiltPayload) -> Result<Self, Self::Error> {
        value.try_into_v4()
    }
}

impl TryFrom<CosmosBuiltPayload> for ExecutionPayloadEnvelopeV5 {
    type Error = BuiltPayloadConversionError;

    fn try_from(value: CosmosBuiltPayload) -> Result<Self, Self::Error> {
        value.try_into_v5()
    }
}

impl TryFrom<CosmosBuiltPayload> for ExecutionPayloadEnvelopeV6 {
    type Error = BuiltPayloadConversionError;

    fn try_from(value: CosmosBuiltPayload) -> Result<Self, Self::Error> {
        value.try_into_v6()
    }
}

impl From<CosmosBuiltPayload> for ExecutionData {
    fn from(value: CosmosBuiltPayload) -> Self {
        value.into_execution_data()
    }
}

impl From<CosmosBuiltPayload> for reth_engine_primitives::BigBlockData<ExecutionData> {
    fn from(_value: CosmosBuiltPayload) -> Self {
        unreachable!("payload building is not supported for big blocks");
    }
}
