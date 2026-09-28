use clap::Parser;
use eyre::config::HookBuilder;
use reth_optimism_consensus::OpBeaconConsensus;
use reth_tracing::tracing::info;
use std::sync::Arc;
use my_chain_chainspec::MyChainSpec;
use my_chain_cli::{
    Cli, MyChainArgs, MyChainNodeConfig, MyChainRpcModuleValidator, MyChainSpecParser,
};
use my_chain_evm::MyChainEvmConfig;
use my_chain_node::{context::MyChainDefaultContext, node::MyChainNode, proof_history};

#[cfg(all(feature = "jemalloc", unix))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn main() {
    dotenvy::dotenv().ok();

    reth_cli_util::sigsegv_handler::install();

    HookBuilder::default()
        .theme(eyre::config::Theme::new())
        .install()
        .expect("failed to install error handler");

    // Enable backtraces unless a RUST_BACKTRACE value has already been explicitly provided.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        unsafe {
            std::env::set_var("RUST_BACKTRACE", "1");
        }
    }

    my_chain_node::init_version_metadata();

    let result = Cli::<MyChainSpecParser, MyChainArgs, MyChainRpcModuleValidator>::parse()
        .run::<MyChainNode<MyChainDefaultContext>, _, _, _>(
            |mut builder, args| async move {
                info!(target: "reth::cli", "Launching node");
                let config: MyChainNodeConfig = args.into_config(builder.config_mut())?;

                info!(target: "reth::cli", "Starting in Flashblocks mode");
                proof_history::launch_node(builder, config).await
            },
            |chain_spec: Arc<MyChainSpec>| {
                (
                    MyChainEvmConfig::optimism(chain_spec.clone()),
                    Arc::new(OpBeaconConsensus::new(chain_spec)),
                )
            },
        );

    if let Err(err) = result {
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
}
