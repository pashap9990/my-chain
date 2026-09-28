use crossbeam_channel::Sender;
use reth_evm::{
    ConfigureEvm, EvmFactory,
    block::{BlockExecutorFactory, BlockExecutorFor, StateDB},
};
use revm::Inspector;

use crate::{BlockExecutionWitness, execution::MyChainBlockExecutor};

/// A [`BlockExecutorFactory`] that wraps the executors produced by `E`'s block executor factory in
/// a [`MyChainBlockExecutor`], threading through an optional capture channel.
#[derive(Debug, Clone)]
pub struct MyChainBlockExecutorFactory<E: ConfigureEvm + 'static> {
    /// The inner factory whose executors are wrapped.
    pub(crate) inner: E::BlockExecutorFactory,
    /// Optional channel that receives a [`BlockExecutionWitness`] for every executed block.
    pub(crate) sender: Option<Sender<BlockExecutionWitness>>,
}

impl<E: ConfigureEvm + 'static> MyChainBlockExecutorFactory<E> {
    /// Creates a new [`MyChainBlockExecutorFactory`] over the given inner factory.
    pub const fn new(
        inner: E::BlockExecutorFactory,
        sender: Option<Sender<BlockExecutionWitness>>,
    ) -> Self {
        Self { inner, sender }
    }
}

impl<E: ConfigureEvm + 'static> BlockExecutorFactory for MyChainBlockExecutorFactory<E> {
    type EvmFactory = <E::BlockExecutorFactory as BlockExecutorFactory>::EvmFactory;
    type TxExecutionResult = <E::BlockExecutorFactory as BlockExecutorFactory>::TxExecutionResult;
    type ExecutionCtx<'a> = <E::BlockExecutorFactory as BlockExecutorFactory>::ExecutionCtx<'a>;
    type Transaction = <E::BlockExecutorFactory as BlockExecutorFactory>::Transaction;
    type Receipt = <E::BlockExecutorFactory as BlockExecutorFactory>::Receipt;

    type Executor<'a, DB, I>
        = MyChainBlockExecutor<BlockExecutorFor<'a, E::BlockExecutorFactory, DB, I>>
    where
        DB: StateDB,
        I: Inspector<<Self::EvmFactory as EvmFactory>::Context<DB>>;

    fn evm_factory(&self) -> &Self::EvmFactory {
        self.inner.evm_factory()
    }

    fn create_executor<'a, DB, I>(
        &'a self,
        evm: <Self::EvmFactory as EvmFactory>::Evm<DB, I>,
        ctx: Self::ExecutionCtx<'a>,
    ) -> Self::Executor<'a, DB, I>
    where
        DB: StateDB,
        I: Inspector<<Self::EvmFactory as EvmFactory>::Context<DB>>,
    {
        MyChainBlockExecutor {
            inner: self.inner.create_executor(evm, ctx),
            sender: self.sender.clone(),
        }
    }
}
