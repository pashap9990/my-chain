#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![feature(min_specialization)]

//! My Chain EVM configuration.

use alloy_consensus::Header;
use alloy_eips::eip2718::Encodable2718;
use reth_evm::{
    Evm,
    block::{BlockExecutionError, BlockExecutor},
    execute::{BlockAssembler, BlockBuilder, BlockBuilderOutcome},
};
use reth_node_api::{FullNodeTypes, NodeTypes};
use reth_node_builder::{BuilderContext, components::ExecutorBuilder};
pub use reth_optimism_evm::{
    L1BlockInfoError, OpBlockAssembler, OpBlockExecutionCtx, OpBlockExecutionError, OpEvm,
    OpEvmConfig, OpEvmFactory, OpNextBlockEnvAttributes, OpRethReceiptBuilder, OpTx, revm_spec,
    revm_spec_by_timestamp_after_bedrock,
};
use reth_optimism_primitives::OpPrimitives;
use reth_provider::{BlockReader, HeaderProvider, StateProvider, StateProviderFactory};
use revm_database::BundleState;
use my_chain_chainspec::MyChainSpec;

mod cache;
mod collector;
pub mod execution;
pub mod factory;
pub mod metrics;
pub mod utils;

pub use cache::{DEFAULT_WITNESS_CAP, WitnessCache};
pub use collector::spawn_witness_collector;
pub use metrics::{FlashblockExecutionMetrics, PayloadBuildStage};

use std::sync::Arc;

/// Thread-safe handle to the shared [`WitnessCache`].
pub type ExecutionWitnessHandle = Arc<WitnessCache>;

use crossbeam_channel::Sender;
use reth_evm::{
    ConfigureEngineEvm, ConfigureEvm, Database, EvmEnvFor, ExecutableTxIterator, ExecutionCtxFor,
};
use reth_optimism_evm::{ConfigurePostExecEvm, PostExecExecutorExt, PostExecMode};
use reth_optimism_payload_builder::OpExecData;
use reth_primitives_traits::{Block, BlockBody, NodePrimitives, SealedBlock, SealedHeader};
use reth_revm::{State, witness::ExecutionWitnessRecord};

use crate::factory::MyChainBlockExecutorFactory;

/// The provider capabilities required by this crate's internals (e.g. witness collection).
///
/// A single blanket-implemented alias trait so the verbose provider bound is written once and used
/// everywhere a provider is needed inside the EVM crate.
pub trait ProviderBounds:
    StateProviderFactory
    + HeaderProvider<Header = Header>
    + BlockReader<Block: Block<Body: BlockBody<Transaction: Encodable2718>>>
    + Clone
    + Send
    + Sync
    + 'static
{
}

impl<T> ProviderBounds for T where
    T: StateProviderFactory
        + HeaderProvider<Header = Header>
        + BlockReader<Block: Block<Body: BlockBody<Transaction: Encodable2718>>>
        + Clone
        + Send
        + Sync
        + 'static
{
}

/// A captured execution witness for a single block.
#[derive(Debug, Clone)]
pub struct BlockExecutionWitness {
    /// Number of the block this witness was captured for.
    pub block_number: u64,
    /// The recorded execution witness.
    pub record: ExecutionWitnessRecord,
}

/// The underlying OP EVM configuration that [`MyChainEvmConfig`] defaults to, fixed to World
/// Chain's production primitives.
pub(crate) type OpConfig =
    OpEvmConfig<MyChainSpec, OpPrimitives, OpRethReceiptBuilder, OpEvmFactory<OpTx>>;

/// The bounds an inner EVM config `E` must satisfy to be wrapped by [`MyChainEvmConfig`].
///
/// Bundles the requirements of the wrapper's [`ConfigureEvm`] impl into a single alias trait: the
/// inner block-executor factory must be cloneable/debuggable and thread-safe, and the inner
/// assembler must assemble blocks for the wrapping [`MyChainBlockExecutorFactory`]. Any
/// `ConfigureEvm` that meets these (e.g. [`OpConfig`]) is blanket-implemented.
pub trait EvmBounds:
    ConfigureEvm<
        BlockExecutorFactory: Clone + core::fmt::Debug + Send + Sync + Unpin,
        BlockAssembler: BlockAssembler<
            MyChainBlockExecutorFactory<Self>,
            Block = <Self::Primitives as NodePrimitives>::Block,
        >,
    > + 'static
{
}

impl<E> EvmBounds for E where
    E: ConfigureEvm<
            BlockExecutorFactory: Clone + core::fmt::Debug + Send + Sync + Unpin,
            BlockAssembler: BlockAssembler<
                MyChainBlockExecutorFactory<E>,
                Block = <E::Primitives as NodePrimitives>::Block,
            >,
        > + 'static
{
}

/// My Chain EVM configuration.
pub struct MyChainEvmConfig<E: EvmBounds = OpConfig> {
    /// The underlying EVM configuration.
    inner: E,
    /// The [`MyChainBlockExecutorFactory`] built from `inner`'s factory.
    factory: MyChainBlockExecutorFactory<E>,
}

impl<E: EvmBounds> Clone for MyChainEvmConfig<E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            factory: self.factory.clone(),
        }
    }
}

impl<E: EvmBounds> core::fmt::Debug for MyChainEvmConfig<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MyChainEvmConfig")
            .field("inner", &self.inner)
            .field("factory", &self.factory)
            .finish()
    }
}

impl MyChainEvmConfig<OpConfig> {
    /// Creates a new configuration with the given chain spec and receipt builder.
    ///
    /// Witness capture is disabled; arm it with [`with_witness_sender`](Self::with_witness_sender).
    pub fn new(chain_spec: Arc<MyChainSpec>, receipt_builder: OpRethReceiptBuilder) -> Self {
        let inner = OpConfig::new(chain_spec, receipt_builder);
        let factory = MyChainBlockExecutorFactory::<OpConfig>::new(
            inner.block_executor_factory().clone(),
            None,
        );
        Self { inner, factory }
    }

    /// Creates a new configuration for OP chains with the default receipt builder.
    ///
    /// Witness capture is disabled; arm it with [`with_witness_sender`](Self::with_witness_sender).
    pub fn optimism(chain_spec: Arc<MyChainSpec>) -> Self {
        Self::new(chain_spec, OpRethReceiptBuilder::default())
    }

    /// Returns the chain spec associated with this configuration.
    pub fn chain_spec(&self) -> &Arc<MyChainSpec> {
        self.inner.chain_spec()
    }

    /// Arms (or disarms) witness capture.
    ///
    /// When `sender` is [`Some`], every executed block's [`BlockExecutionWitness`] is forwarded over the
    /// channel; [`None`] leaves the config a pure passthrough.
    pub fn with_witness_sender(mut self, sender: Option<Sender<BlockExecutionWitness>>) -> Self {
        self.factory = MyChainBlockExecutorFactory::<OpConfig>::new(
            self.inner.block_executor_factory().clone(),
            sender,
        );
        self
    }
}

impl<E: EvmBounds> ConfigureEvm for MyChainEvmConfig<E> {
    type Primitives = E::Primitives;
    type Error = E::Error;
    type NextBlockEnvCtx = E::NextBlockEnvCtx;
    type BlockExecutorFactory = MyChainBlockExecutorFactory<E>;
    type BlockAssembler = E::BlockAssembler;

    fn block_executor_factory(&self) -> &Self::BlockExecutorFactory {
        &self.factory
    }

    fn block_assembler(&self) -> &Self::BlockAssembler {
        self.inner.block_assembler()
    }

    fn evm_env(
        &self,
        header: &<Self::Primitives as NodePrimitives>::BlockHeader,
    ) -> Result<EvmEnvFor<Self>, Self::Error> {
        self.inner.evm_env(header)
    }

    fn next_evm_env(
        &self,
        parent: &<Self::Primitives as NodePrimitives>::BlockHeader,
        attributes: &Self::NextBlockEnvCtx,
    ) -> Result<EvmEnvFor<Self>, Self::Error> {
        self.inner.next_evm_env(parent, attributes)
    }

    fn context_for_block<'a>(
        &self,
        block: &'a SealedBlock<<Self::Primitives as NodePrimitives>::Block>,
    ) -> Result<ExecutionCtxFor<'a, Self>, Self::Error> {
        self.inner.context_for_block(block)
    }

    fn context_for_next_block(
        &self,
        parent: &SealedHeader<<Self::Primitives as NodePrimitives>::BlockHeader>,
        attributes: Self::NextBlockEnvCtx,
    ) -> Result<ExecutionCtxFor<'_, Self>, Self::Error> {
        self.inner.context_for_next_block(parent, attributes)
    }
}

impl<E: EvmBounds + ConfigurePostExecEvm> ConfigurePostExecEvm for MyChainEvmConfig<E> {
    type Snapshot = E::Snapshot;

    fn post_exec_executor_for_block<'a, DB: Database>(
        &'a self,
        db: &'a mut State<DB>,
        block: &'a SealedBlock<<Self::Primitives as NodePrimitives>::Block>,
        post_exec_mode: PostExecMode,
    ) -> Result<
        impl BlockExecutor<
            Transaction = <Self::Primitives as NodePrimitives>::SignedTx,
            Receipt = <Self::Primitives as NodePrimitives>::Receipt,
        > + PostExecExecutorExt<Snapshot = Self::Snapshot>
        + 'a,
        Self::Error,
    > {
        self.inner
            .post_exec_executor_for_block(db, block, post_exec_mode)
    }

    fn post_exec_builder_for_next_block<'a, DB: Database + 'a>(
        &'a self,
        db: &'a mut State<DB>,
        parent: &'a SealedHeader<<Self::Primitives as NodePrimitives>::BlockHeader>,
        attributes: Self::NextBlockEnvCtx,
        post_exec_mode: PostExecMode,
    ) -> Result<
        impl BlockBuilder<
            Primitives = Self::Primitives,
            Executor: PostExecExecutorExt<Snapshot = Self::Snapshot>
                          + BlockExecutor<
                Evm: Evm<DB: core::ops::DerefMut<Target = State<DB>>>,
                Result: reth_optimism_evm::PreRefundGasUsed,
            >,
        > + 'a,
        Self::Error,
    > {
        self.inner
            .post_exec_builder_for_next_block(db, parent, attributes, post_exec_mode)
    }
}

impl<E: EvmBounds + ConfigureEngineEvm<OpExecData>> ConfigureEngineEvm<OpExecData>
    for MyChainEvmConfig<E>
{
    fn evm_env_for_payload(&self, payload: &OpExecData) -> Result<EvmEnvFor<Self>, Self::Error> {
        self.inner.evm_env_for_payload(payload)
    }

    fn context_for_payload<'a>(
        &self,
        payload: &'a OpExecData,
    ) -> Result<ExecutionCtxFor<'a, Self>, Self::Error> {
        self.inner.context_for_payload(payload)
    }

    fn tx_iterator_for_payload(
        &self,
        payload: &OpExecData,
    ) -> Result<impl ExecutableTxIterator<Self>, Self::Error> {
        self.inner.tx_iterator_for_payload(payload)
    }
}

pub trait BlockBuilderExt: BlockBuilder {
    /// Completes block building and returns the [`BlockBuilderOutcome`] plus the accumulated
    /// [`BundleState`].
    fn finish_with_bundle(
        self,
        state_provider: impl StateProvider,
        metrics: impl FlashblockExecutionMetrics,
    ) -> Result<(BlockBuilderOutcome<Self::Primitives>, BundleState), BlockExecutionError>;
}

/// Executor builder that constructs the [`MyChainEvmConfig`].
///
/// When a witness sender is provided, the resulting EVM captures each block's execution witness
/// during import and forwards it over the channel. With no sender (the [`Default`]) the config is a
/// pure passthrough and the EVM behaves identically to the underlying OP EVM config.
#[derive(Debug, Clone, Default)]
pub struct MyChainExecutorBuilder(Option<crossbeam_channel::Sender<BlockExecutionWitness>>);

impl MyChainExecutorBuilder {
    /// Creates a builder that forwards captured block witnesses over `witness_sender`.
    pub const fn new(
        witness_sender: Option<crossbeam_channel::Sender<BlockExecutionWitness>>,
    ) -> Self {
        Self(witness_sender)
    }
}

impl<Node> ExecutorBuilder<Node> for MyChainExecutorBuilder
where
    Node: FullNodeTypes<Types: NodeTypes<ChainSpec = MyChainSpec, Primitives = OpPrimitives>>,
{
    type EVM = MyChainEvmConfig;

    async fn build_evm(self, ctx: &BuilderContext<Node>) -> eyre::Result<Self::EVM> {
        Ok(
            MyChainEvmConfig::new(ctx.chain_spec(), OpRethReceiptBuilder::default())
                .with_witness_sender(self.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn my_chain_config_passthrough_construction() {
        let chain_spec = MyChainSpec::dev();
        let config = MyChainEvmConfig::optimism(chain_spec.clone());

        assert!(Arc::ptr_eq(config.chain_spec(), &chain_spec));
        // The config exposes its own witness-capturing factory even when capture is disarmed.
        let _factory: &MyChainBlockExecutorFactory<OpConfig> = config.block_executor_factory();
    }

    #[test]
    fn my_chain_config_defaults_to_my_chain_spec() {
        let chain_spec = MyChainSpec::dev();
        let evm_config = MyChainEvmConfig::optimism(chain_spec.clone());

        assert!(std::sync::Arc::ptr_eq(evm_config.chain_spec(), &chain_spec));
    }
}
