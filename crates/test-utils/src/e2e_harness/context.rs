//! Example on how to build a custom context for the my chain node.

use alloy_consensus::{Eip658Value, Header, Receipt};
use alloy_evm::{Evm, eth::receipt_builder::ReceiptBuilderCtx};
use alloy_op_evm::block::receipt_builder::OpReceiptBuilder;
use alloy_rpc_types_engine::{ExecutionPayloadEnvelopeV2, ExecutionPayloadV1};
use op_alloy_consensus::{OpDepositReceipt, OpTxEnvelope, OpTxType};
use op_alloy_rpc_types::{OpTransactionReceipt, OpTransactionRequest};
use op_alloy_rpc_types_engine::{
    OpExecutionData, OpExecutionPayload, OpExecutionPayloadEnvelopeV3, OpExecutionPayloadEnvelopeV4,
};
use reth_node_api::{
    BuiltPayload, EngineTypes, FullNodeTypes, NodePrimitives, NodeTypes, PayloadTypes,
};
use reth_node_builder::{
    BuilderContext, NodeAdapter, NodeComponentsBuilder,
    components::{
        BasicPayloadServiceBuilder, ComponentsBuilder, ExecutorBuilder, PayloadServiceBuilder,
    },
    rpc::{BasicEngineValidatorBuilder, RpcAddOns},
};
use reth_optimism_node::{
    OpAddOns, OpBuiltPayload, OpConsensusBuilder, OpEngineValidatorBuilder, node::OpPayloadBuilder,
    rpc::OpEthApiBuilder, txpool::OpPooledTransaction,
};
use reth_optimism_payload_builder::{OpExecData, OpPayloadAttrs};
use reth_optimism_primitives::{OpBlock, OpBlockBody, OpReceipt, OpTransactionSigned};
use reth_primitives_traits::{Block as _, BlockTy, SealedBlock};
use reth_rpc_api::eth::RpcTypes;
use my_chain_chainspec::MyChainSpec;
use my_chain_cli::{MyChainArgs, MyChainNodeConfig};
use my_chain_evm::{OpEvmConfig, OpRethReceiptBuilder};
use my_chain_node::{
    context::{FlashblocksComponentsContext, MyChainNetworkBuilder},
    engine::FlashblocksEngineApiBuilder,
    node::{MyChainNode, MyChainNodeContext, MyChainNodePrimitiveTypes},
    pool::MyChainPoolBuilder,
};
use my_chain_pool::BasicMyChainPool;

/// Node primitives for my chain.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WorldPrimitives;

impl NodePrimitives for WorldPrimitives {
    type Block = OpBlock;
    type BlockHeader = Header;
    type BlockBody = OpBlockBody;
    type SignedTx = OpTransactionSigned;
    type Receipt = OpReceipt;
}

/// Engine payload type set for my chain.
#[derive(Debug, Default, Clone, serde::Deserialize, serde::Serialize)]
pub struct WorldEngineTypes;

impl PayloadTypes for WorldEngineTypes {
    type ExecutionData = OpExecData;
    type BuiltPayload = OpBuiltPayload<WorldPrimitives>;
    type PayloadAttributes = OpPayloadAttrs;

    fn block_to_payload(
        block: SealedBlock<BlockTy<<Self::BuiltPayload as BuiltPayload>::Primitives>>,
        _bal: Option<alloy_primitives::Bytes>,
    ) -> Self::ExecutionData {
        let (payload, sidecar) = OpExecutionPayload::from_block_unchecked(
            block.hash(),
            &block.into_block().into_ethereum_block(),
        );
        OpExecData::from(OpExecutionData::new(payload, sidecar))
    }
}

impl EngineTypes for WorldEngineTypes {
    type ExecutionPayloadEnvelopeV1 = ExecutionPayloadV1;
    type ExecutionPayloadEnvelopeV2 = ExecutionPayloadEnvelopeV2;
    type ExecutionPayloadEnvelopeV3 = OpExecutionPayloadEnvelopeV3;
    type ExecutionPayloadEnvelopeV4 = OpExecutionPayloadEnvelopeV4;
    type ExecutionPayloadEnvelopeV5 = OpExecutionPayloadEnvelopeV4;
    type ExecutionPayloadEnvelopeV6 = OpExecutionPayloadEnvelopeV4;
}

/// Receipt builder for my chain.
#[derive(Debug, Default, Clone, Copy)]
pub struct WorldReceiptBuilder;

impl OpReceiptBuilder for WorldReceiptBuilder {
    type Transaction = OpTransactionSigned;
    type Receipt = OpReceipt;

    fn build_receipt<'a, E: Evm>(
        &self,
        ctx: ReceiptBuilderCtx<'a, OpTxType, E>,
    ) -> Result<Self::Receipt, ReceiptBuilderCtx<'a, OpTxType, E>> {
        match ctx.tx_type {
            OpTxType::Deposit => Err(ctx),
            ty => {
                let receipt = Receipt {
                    // Success flag was added in `EIP-658: Embedding transaction status code in
                    // receipts`.
                    status: Eip658Value::Eip658(ctx.result.is_success()),
                    cumulative_gas_used: ctx.cumulative_gas_used,
                    logs: ctx.result.into_logs(),
                };

                Ok(match ty {
                    OpTxType::Legacy => OpReceipt::Legacy(receipt),
                    OpTxType::Eip1559 => OpReceipt::Eip1559(receipt),
                    OpTxType::Eip2930 => OpReceipt::Eip2930(receipt),
                    OpTxType::Eip7702 => OpReceipt::Eip7702(receipt),
                    OpTxType::PostExec => OpReceipt::PostExec(receipt),
                    OpTxType::Deposit => unreachable!(),
                })
            }
        }
    }

    fn build_deposit_receipt(&self, inner: OpDepositReceipt) -> Self::Receipt {
        OpReceipt::Deposit(inner)
    }

    fn strip_deposit_nonce(&self, receipt: &mut Self::Receipt) {
        OpRethReceiptBuilder::default().strip_deposit_nonce(receipt);
    }
}

/// Executor builder for my chain.
#[derive(Debug, Default, Clone, Copy)]
pub struct WorldExecutorBuilder;

impl<Node> ExecutorBuilder<Node> for WorldExecutorBuilder
where
    Node: FullNodeTypes<Types: NodeTypes<ChainSpec = MyChainSpec, Primitives = WorldPrimitives>>,
{
    type EVM = OpEvmConfig<MyChainSpec, WorldPrimitives, WorldReceiptBuilder>;

    async fn build_evm(self, ctx: &BuilderContext<Node>) -> eyre::Result<Self::EVM> {
        Ok(OpEvmConfig::new(ctx.chain_spec(), WorldReceiptBuilder))
    }
}

/// RPC type adapter for my chain.
#[derive(Debug, Clone, Copy, Default)]
pub struct WorldRpcTypes;

impl RpcTypes for WorldRpcTypes {
    type Header = alloy_rpc_types_eth::Header<Header>;
    type Receipt = OpTransactionReceipt;
    type TransactionRequest = OpTransactionRequest;
    type TransactionResponse = op_alloy_rpc_types::Transaction<OpTxEnvelope>;
}

/// My Chain e2e context that launches a my chain node.
#[derive(Clone, Debug)]
pub struct WorldNodeContext {
    config: MyChainNodeConfig,
    components_context: Option<FlashblocksComponentsContext>,
}

impl WorldNodeContext {
    /// Returns the node configuration captured when the context was created.
    pub fn config(&self) -> &MyChainNodeConfig {
        &self.config
    }
}

impl From<MyChainNodeConfig> for WorldNodeContext {
    fn from(config: MyChainNodeConfig) -> Self {
        let components_context = config
            .args
            .flashblocks
            .as_ref()
            .map(|_| config.clone().into());

        Self {
            config,
            components_context,
        }
    }
}

impl MyChainNodePrimitiveTypes for WorldNodeContext {
    type Primitives = WorldPrimitives;
    type Payload = WorldEngineTypes;
    type ChainSpec = MyChainSpec;
}

impl<N> MyChainNodeContext<N> for WorldNodeContext
where
    N: FullNodeTypes<Types = MyChainNode<Self>>,
    BasicPayloadServiceBuilder<OpPayloadBuilder>: PayloadServiceBuilder<
            N,
            BasicMyChainPool<
                N,
                OpPooledTransaction,
                OpEvmConfig<MyChainSpec, WorldPrimitives, WorldReceiptBuilder>,
            >,
            OpEvmConfig<MyChainSpec, WorldPrimitives, WorldReceiptBuilder>,
        >,
{
    type Evm = OpEvmConfig<MyChainSpec, WorldPrimitives, WorldReceiptBuilder>;
    type Pool = BasicMyChainPool<N, OpPooledTransaction, Self::Evm>;
    type Net = MyChainNetworkBuilder;
    type PayloadServiceBuilder = BasicPayloadServiceBuilder<OpPayloadBuilder>;
    type ComponentsBuilder = ComponentsBuilder<
        N,
        MyChainPoolBuilder<OpPooledTransaction>,
        Self::PayloadServiceBuilder,
        Self::Net,
        WorldExecutorBuilder,
        OpConsensusBuilder,
    >;
    type AddOns = OpAddOns<
        NodeAdapter<N, <Self::ComponentsBuilder as NodeComponentsBuilder<N>>::Components>,
        OpEthApiBuilder<WorldRpcTypes>,
        OpEngineValidatorBuilder,
        FlashblocksEngineApiBuilder<OpEngineValidatorBuilder>,
        BasicEngineValidatorBuilder<OpEngineValidatorBuilder>,
    >;
    type ExtContext = Option<FlashblocksComponentsContext>;

    fn components(&self) -> Self::ComponentsBuilder {
        let Self {
            config:
                MyChainNodeConfig {
                    args:
                        MyChainArgs {
                            rollup,
                            pbh,
                            tx_peers,
                            ..
                        },
                    ..
                },
            components_context,
        } = self.clone();

        let reth_optimism_node::args::RollupArgs {
            disable_txpool_gossip,
            compute_pending_block,
            discovery_v4,
            ..
        } = rollup;

        let wc_network_builder = MyChainNetworkBuilder::new(
            disable_txpool_gossip,
            !discovery_v4,
            tx_peers,
            components_context
                .as_ref()
                .map(|flashblocks_components_ctx| {
                    flashblocks_components_ctx.flashblocks_handle.clone()
                }),
        );

        ComponentsBuilder::default()
            .node_types::<N>()
            .pool(MyChainPoolBuilder::new(
                pbh.entrypoint,
                pbh.signature_aggregator,
                pbh.world_id,
            ))
            .executor(WorldExecutorBuilder)
            .payload(BasicPayloadServiceBuilder::new(OpPayloadBuilder::new(
                compute_pending_block,
            )))
            .network(wc_network_builder)
            .consensus(OpConsensusBuilder::default())
    }

    fn add_ons(&self) -> Self::AddOns {
        let engine_api_builder = FlashblocksEngineApiBuilder {
            engine_validator_builder: Default::default(),
            flashblocks_handle: self.components_context.as_ref().map(
                |flashblocks_components_ctx| flashblocks_components_ctx.flashblocks_handle.clone(),
            ),
            to_jobs_generator: self
                .components_context
                .as_ref()
                .map(|flashblocks_components_ctx| {
                    flashblocks_components_ctx.to_jobs_generator.clone()
                }),
            authorizer_vk: self
                .components_context
                .as_ref()
                .map(|flashblocks_components_ctx| flashblocks_components_ctx.authorizer_vk),
            flashblocks_state: self
                .components_context
                .as_ref()
                .map(|flashblocks_components_ctx| {
                    flashblocks_components_ctx.flashblocks_state.clone()
                }),
        };
        let op_eth_api_builder = OpEthApiBuilder::<WorldRpcTypes>::default()
            .with_sequencer(self.config.args.rollup.sequencer.clone());

        let engine_validator_builder =
            BasicEngineValidatorBuilder::<OpEngineValidatorBuilder>::default();

        let rpc_add_ons = RpcAddOns::new(
            op_eth_api_builder,
            Default::default(),
            engine_api_builder,
            engine_validator_builder,
            Default::default(),
            Default::default(),
        );

        OpAddOns::new(
            rpc_add_ons,
            self.config.builder_config.inner.da_config.clone(),
            self.config.builder_config.inner.gas_limit_config.clone(),
            Default::default(),
            self.config.args.rollup.sequencer.clone(),
            Default::default(),
            Default::default(),
            false,
            1_000_000,
        )
    }

    fn ext_context(&self) -> Self::ExtContext {
        self.components_context.clone()
    }
}
