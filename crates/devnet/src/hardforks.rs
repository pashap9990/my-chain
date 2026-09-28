use std::collections::BTreeSet;

use eyre::eyre::{Result, bail};
use reth_chainspec::{EthereumHardfork, ForkCondition};
use my_chain_chainspec::{MyChainHardfork, MyChainSpec};

/// Canonical hardfork order for local My Chain devnets.
pub const MY_CHAIN_DEVNET_HARDFORK_ORDER: [MyChainHardfork; 12] = [
    MyChainHardfork::Bedrock,
    MyChainHardfork::Regolith,
    MyChainHardfork::Canyon,
    MyChainHardfork::Ecotone,
    MyChainHardfork::Fjord,
    MyChainHardfork::Granite,
    MyChainHardfork::Holocene,
    MyChainHardfork::Isthmus,
    MyChainHardfork::Jovian,
    MyChainHardfork::Karst,
    MyChainHardfork::Tropo,
    MyChainHardfork::Strato,
];

/// Typed My Chain hardfork selection for local devnets.
///
/// Defaults match the post-Karst local-dev baseline: all OP/World hardforks
/// through Karst are active at genesis, while the World-specific Tropo and
/// Strato forks are disabled until explicitly selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MyChainHardforkConfig {
    active: BTreeSet<MyChainHardfork>,
}

impl Default for MyChainHardforkConfig {
    fn default() -> Self {
        Self::through(MyChainHardfork::Karst)
    }
}

impl MyChainHardforkConfig {
    /// Enable all forks up to and including `latest`.
    pub fn through(latest: MyChainHardfork) -> Self {
        let active = MY_CHAIN_DEVNET_HARDFORK_ORDER
            .into_iter()
            .take_while(|fork| fork.idx() <= latest.idx())
            .collect();
        Self { active }
    }

    /// Return true if `fork` is active at genesis.
    pub fn is_active(&self, fork: MyChainHardfork) -> bool {
        self.active.contains(&fork)
    }

    /// Enable an individual hardfork.
    pub fn enable(mut self, fork: MyChainHardfork) -> Self {
        self.active.insert(fork);
        self
    }

    /// Disable an individual hardfork.
    pub fn disable(mut self, fork: MyChainHardfork) -> Self {
        self.active.remove(&fork);
        self
    }

    /// Active hardforks in canonical order.
    pub fn active(&self) -> impl Iterator<Item = MyChainHardfork> + '_ {
        MY_CHAIN_DEVNET_HARDFORK_ORDER
            .into_iter()
            .filter(|fork| self.active.contains(fork))
    }

    /// Validate that the selected hardforks form a prefix of the canonical order.
    pub fn validate(&self) -> Result<()> {
        let mut seen_inactive = false;
        for fork in MY_CHAIN_DEVNET_HARDFORK_ORDER {
            if self.active.contains(&fork) {
                if seen_inactive {
                    bail!(
                        "invalid My Chain hardfork selection: {} is active after an earlier fork was disabled",
                        fork.name()
                    );
                }
            } else {
                seen_inactive = true;
            }
        }
        Ok(())
    }

    /// Apply this selection to a chain spec.
    pub fn apply_to(&self, mut spec: MyChainSpec) -> MyChainSpec {
        for fork in MY_CHAIN_DEVNET_HARDFORK_ORDER {
            let condition = if self.active.contains(&fork) {
                match fork {
                    MyChainHardfork::Bedrock => ForkCondition::Block(0),
                    _ => ForkCondition::Timestamp(0),
                }
            } else {
                ForkCondition::Never
            };
            spec.set_fork(fork, condition);
        }

        spec.set_fork(
            EthereumHardfork::Shanghai,
            if self.active.contains(&MyChainHardfork::Canyon) {
                ForkCondition::Timestamp(0)
            } else {
                ForkCondition::Never
            },
        );
        spec.set_fork(
            EthereumHardfork::Cancun,
            if self.active.contains(&MyChainHardfork::Ecotone) {
                ForkCondition::Timestamp(0)
            } else {
                ForkCondition::Never
            },
        );
        spec.set_fork(
            EthereumHardfork::Prague,
            if self.active.contains(&MyChainHardfork::Isthmus) {
                ForkCondition::Timestamp(0)
            } else {
                ForkCondition::Never
            },
        );
        spec.set_fork(
            EthereumHardfork::Osaka,
            if self.active.contains(&MyChainHardfork::Karst) {
                ForkCondition::Timestamp(0)
            } else {
                ForkCondition::Never
            },
        );

        spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reth_chainspec::Hardforks;

    #[test]
    fn default_local_devnet_hardforks_stop_at_karst() {
        let hardforks = MyChainHardforkConfig::default();

        assert!(hardforks.is_active(MyChainHardfork::Jovian));
        assert!(hardforks.is_active(MyChainHardfork::Karst));
        assert!(!hardforks.is_active(MyChainHardfork::Tropo));
        assert!(!hardforks.is_active(MyChainHardfork::Strato));
    }

    #[test]
    fn karst_local_devnet_activates_osaka_without_world_specific_forks() {
        let hardforks = MyChainHardforkConfig::through(MyChainHardfork::Karst);
        let spec = hardforks.apply_to(MyChainSpec::dev().as_ref().clone());

        assert!(hardforks.is_active(MyChainHardfork::Karst));
        assert!(!hardforks.is_active(MyChainHardfork::Tropo));
        assert_eq!(
            spec.fork(MyChainHardfork::Karst),
            ForkCondition::Timestamp(0)
        );
        assert_eq!(
            spec.fork(EthereumHardfork::Osaka),
            ForkCondition::Timestamp(0)
        );
    }

    #[test]
    fn validates_prefix_only_hardfork_selection() {
        let valid = MyChainHardforkConfig::through(MyChainHardfork::Ecotone);
        assert!(valid.validate().is_ok());

        let invalid = valid.enable(MyChainHardfork::Jovian);
        assert!(invalid.validate().is_err());
    }
}
