use std::sync::Arc;

use reth_cli::chainspec::{ChainSpecParser, parse_genesis};
use reth_optimism_chainspec::SUPPORTED_CHAINS;
use my_chain_chainspec::MyChainSpec;

/// My Chain chain specification parser.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct MyChainSpecParser;

impl ChainSpecParser for MyChainSpecParser {
    type ChainSpec = MyChainSpec;

    const SUPPORTED_CHAINS: &'static [&'static str] = SUPPORTED_CHAINS;

    fn parse(s: &str) -> eyre::Result<Arc<Self::ChainSpec>> {
        chain_value_parser(s)
    }
}

/// Clap value parser for [`MyChainSpec`]s.
///
/// Matches either a known OP stack chain, a path to a genesis JSON file, or an in-memory genesis
/// JSON string.
pub fn chain_value_parser(s: &str) -> eyre::Result<Arc<MyChainSpec>> {
    if let Some(my_chain_spec) = MyChainSpec::parse_chain(s) {
        Ok(my_chain_spec)
    } else {
        Ok(Arc::new(parse_genesis(s)?.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_known_chain_spec() {
        for &chain in MyChainSpecParser::SUPPORTED_CHAINS {
            assert!(
                <MyChainSpecParser as ChainSpecParser>::parse(chain).is_ok(),
                "Failed to parse {chain}"
            );
        }
    }
}
