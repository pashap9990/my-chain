use super::tx::MyChainPoolTransaction;
use reth_transaction_pool::{CoinbaseTipOrdering, Priority, TransactionOrdering};
use revm_primitives::U256;

/// Default ordering for the pool.
///
/// The transactions are ordered by their coinbase tip.
/// The higher the coinbase tip is, the higher the priority of the transaction.
#[derive(Debug)]
pub struct MyChainOrdering<T> {
    inner: CoinbaseTipOrdering<T>,
}

/// Ordering is automatically derived.
///
/// The ordering of fields here is important.
#[derive(Debug, Default, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct MyChainPriority {
    is_pbh: bool,
    effective_tip_per_gas: Option<U256>,
}

impl<T> TransactionOrdering for MyChainOrdering<T>
where
    T: MyChainPoolTransaction + 'static,
{
    type PriorityValue = MyChainPriority;
    type Transaction = T;

    fn priority(
        &self,
        transaction: &Self::Transaction,
        base_fee: u64,
    ) -> Priority<Self::PriorityValue> {
        let effective_tip_per_gas = transaction.effective_tip_per_gas(base_fee).map(U256::from);

        Some(MyChainPriority {
            is_pbh: transaction.pbh_payload().is_some(),
            effective_tip_per_gas,
        })
        .into()
    }
}

impl<T> Clone for MyChainOrdering<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Default for MyChainOrdering<T> {
    fn default() -> Self {
        Self {
            inner: CoinbaseTipOrdering::default(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use test_case::test_case;

    #[test]
    fn pbh_has_priority() {
        let pbh = MyChainPriority {
            is_pbh: true,
            effective_tip_per_gas: Some(U256::from(100u64)),
        };

        let no_pbh = MyChainPriority {
            is_pbh: false,
            effective_tip_per_gas: Some(U256::from(10000u64)),
        };

        assert!(pbh > no_pbh);
    }

    #[test_case(true)]
    #[test_case(false)]
    fn higher_tip_has_priority(is_pbh: bool) {
        let lower_tip = MyChainPriority {
            is_pbh,
            effective_tip_per_gas: Some(U256::from(100u64)),
        };

        let higher_tip = MyChainPriority {
            is_pbh,
            effective_tip_per_gas: Some(U256::from(10000u64)),
        };

        assert!(higher_tip > lower_tip);
    }
}
