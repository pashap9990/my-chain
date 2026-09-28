use reth_chainspec::{EthereumHardforks, ForkCondition, hardfork};

hardfork!(
    /// The name of a My Chain hardfork.
    ///
    /// My Chain follows the OP Stack upgrade sequence through Karst, then uses
    /// My Chain specific upgrade names as the canonical schedule diverges.
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Default)]
    MyChainHardfork {
        /// Bedrock: OP Stack Bedrock upgrade.
        Bedrock,
        /// Regolith: OP Stack Regolith upgrade.
        Regolith,
        /// Canyon: OP Stack Canyon upgrade.
        Canyon,
        /// Ecotone: OP Stack Ecotone upgrade.
        Ecotone,
        /// Fjord: OP Stack Fjord upgrade.
        Fjord,
        /// Granite: OP Stack Granite upgrade.
        Granite,
        /// Holocene: OP Stack Holocene upgrade.
        Holocene,
        /// Isthmus: OP Stack Isthmus upgrade.
        Isthmus,
        /// Jovian: OP Stack Jovian upgrade. My Chain is already on this hardfork.
        #[default]
        Jovian,
        /// Karst: OP Stack Karst upgrade.
        Karst,
        /// Tropo: the first My Chain specific hardfork after Karst.
        Tropo,
        /// Strato: the second My Chain specific hardfork after Karst.
        Strato,
    }
);

impl MyChainHardfork {
    /// Returns index of `self` in the canonical My Chain hardfork order.
    pub const fn idx(&self) -> usize {
        *self as usize
    }
}

/// Extends [`EthereumHardforks`] with My Chain hardfork helper methods.
#[auto_impl::auto_impl(&, Arc)]
pub trait MyChainHardforks: EthereumHardforks {
    /// Retrieves [`ForkCondition`] by a [`MyChainHardfork`]. If `fork` is not present,
    /// returns [`ForkCondition::Never`].
    fn my_chain_fork_activation(&self, fork: MyChainHardfork) -> ForkCondition;

    /// Returns `true` if [`Bedrock`](MyChainHardfork::Bedrock) is active at the block number.
    fn is_bedrock_active_at_block(&self, block_number: u64) -> bool {
        self.my_chain_fork_activation(MyChainHardfork::Bedrock)
            .active_at_block(block_number)
    }

    /// Returns `true` if [`Regolith`](MyChainHardfork::Regolith) is active.
    fn is_regolith_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.my_chain_fork_activation(MyChainHardfork::Regolith)
            .active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Canyon`](MyChainHardfork::Canyon) is active.
    fn is_canyon_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.my_chain_fork_activation(MyChainHardfork::Canyon)
            .active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Ecotone`](MyChainHardfork::Ecotone) is active.
    fn is_ecotone_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.my_chain_fork_activation(MyChainHardfork::Ecotone)
            .active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Fjord`](MyChainHardfork::Fjord) is active.
    fn is_fjord_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.my_chain_fork_activation(MyChainHardfork::Fjord)
            .active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Granite`](MyChainHardfork::Granite) is active.
    fn is_granite_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.my_chain_fork_activation(MyChainHardfork::Granite)
            .active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Holocene`](MyChainHardfork::Holocene) is active.
    fn is_holocene_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.my_chain_fork_activation(MyChainHardfork::Holocene)
            .active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Isthmus`](MyChainHardfork::Isthmus) is active.
    fn is_isthmus_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.my_chain_fork_activation(MyChainHardfork::Isthmus)
            .active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Jovian`](MyChainHardfork::Jovian) is active.
    fn is_jovian_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.my_chain_fork_activation(MyChainHardfork::Jovian)
            .active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Karst`](MyChainHardfork::Karst) is active.
    fn is_karst_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.my_chain_fork_activation(MyChainHardfork::Karst)
            .active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Tropo`](MyChainHardfork::Tropo) is active.
    fn is_tropo_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.my_chain_fork_activation(MyChainHardfork::Tropo)
            .active_at_timestamp(timestamp)
    }

    /// Returns `true` if [`Strato`](MyChainHardfork::Strato) is active.
    fn is_strato_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.my_chain_fork_activation(MyChainHardfork::Strato)
            .active_at_timestamp(timestamp)
    }
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use super::*;

    #[test]
    fn parses_case_insensitive_hardfork_names() {
        assert_eq!(
            MyChainHardfork::from_str("kArSt").unwrap(),
            MyChainHardfork::Karst
        );
        assert_eq!(
            MyChainHardfork::from_str("tRoPo").unwrap(),
            MyChainHardfork::Tropo
        );
        assert_eq!(
            MyChainHardfork::from_str("sTrAtO").unwrap(),
            MyChainHardfork::Strato
        );
    }
}
