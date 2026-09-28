#![warn(unused_crate_dependencies)]

mod builder;
mod hardfork;
mod spec;

pub use builder::MyChainSpecBuilder;
pub use hardfork::{MyChainHardfork, MyChainHardforks};
pub use spec::{
    JOVIAN_UPGRADE_TIMESTAMP_MAINNET, JOVIAN_UPGRADE_TIMESTAMP_SEPOLIA,
    KARST_UPGRADE_TIMESTAMP_MAINNET, KARST_UPGRADE_TIMESTAMP_SEPOLIA, MyChainSpec,
};
