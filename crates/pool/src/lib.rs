#![warn(unused_crate_dependencies)]

use ordering::MyChainOrdering;
use reth_node_api::FullNodeTypes;
use reth_transaction_pool::{
    Pool, TransactionValidationTaskExecutor, blobstore::DiskFileBlobStore,
};
use tx::MyChainPooledTransaction;
use validator::MyChainTransactionValidator;
use my_chain_evm::MyChainEvmConfig;

pub mod bindings;
pub mod eip4337;
pub mod error;
pub mod noop;
pub mod ordering;
pub mod root;
pub mod tx;
pub mod validator;

/// Type alias for My Chain transaction pool
pub type MyChainTransactionPool<
    Client,
    S,
    T = MyChainPooledTransaction,
    Evm = MyChainEvmConfig,
> = Pool<
    TransactionValidationTaskExecutor<MyChainTransactionValidator<Client, T, Evm>>,
    MyChainOrdering<T>,
    S,
>;

/// A wrapper type with sensible defaults for the My Chain transaction pool.
pub type BasicMyChainPool<N, T = MyChainPooledTransaction, Evm = MyChainEvmConfig> =
    MyChainTransactionPool<<N as FullNodeTypes>::Provider, DiskFileBlobStore, T, Evm>;
