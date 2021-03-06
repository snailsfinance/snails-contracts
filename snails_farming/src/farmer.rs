//! Farmer records a farmer's
//! * all claimed reward tokens,
//! * all seeds he staked,
//! * user_rps per farm,
//! and the deposited near amount prepaid as storage fee

use crate::errors::*;
use crate::utils::MAX_ACCOUNT_LENGTH;
use crate::StorageKeys;
use crate::{FarmId, SeedId, RPS};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::{env, AccountId, Balance};
use std::collections::HashMap;
/// each entry cost MAX_ACCOUNT_LENGTH bytes,
/// amount: Balance cost 16 bytes
/// each empty hashmap cost 4 bytes
pub const MIN_FARMER_LENGTH: u128 = MAX_ACCOUNT_LENGTH + 16 + 4 * 3;

/// Account deposits information and storage cost.
#[derive(BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "test", derive(Clone))]
pub struct Farmer {
    /// Native NEAR amount sent to this contract.
    /// Used for storage.
    pub amount: Balance,
    /// Amounts of various reward tokens the farmer claimed.
    pub rewards: HashMap<AccountId, Balance>,
    /// Amounts of various seed tokens the farmer staked.
    pub seeds: HashMap<SeedId, Balance>,
    /// record user_last_rps of farms
    pub user_rps: LookupMap<FarmId, RPS>,
    pub rps_count: u32,
}

impl Farmer {
    /// Adds amount to the balance of given token
    pub(crate) fn add_reward(&mut self, token: &AccountId, amount: Balance) {
        if let Some(x) = self.rewards.get_mut(token) {
            *x = (*x).checked_add(amount).unwrap();
        } else {
            self.rewards.insert(token.clone(), amount);
        }
    }

    /// Subtract from `reward` balance.
    /// if amount == 0, subtract all reward balance.
    /// Panics if `amount` is bigger than the current balance.
    /// return actual subtract amount
    pub(crate) fn sub_reward(&mut self, token: &AccountId, amount: Balance) -> Balance {
        let value = *self.rewards.get(token).expect(TOKEN_NOT_REG);
        assert!(value >= amount, "{}", NOT_ENOUGH_TOKENS);
        if amount == 0 {
            self.rewards.remove(&token.clone());
            value
        } else {
            self.rewards
                .insert(token.clone(), value.checked_sub(amount).unwrap());
            amount
        }
    }

    pub fn add_seed(&mut self, seed_id: &SeedId, amount: Balance) {
        if amount > 0 {
            self.seeds.insert(
                seed_id.clone(),
                amount + self.seeds.get(seed_id).unwrap_or(&0_u128),
            );
        }
    }

    /// return seed remained.
    pub fn sub_seed(&mut self, seed_id: &SeedId, amount: Balance) -> Balance {
        let prev_balance = self
            .seeds
            .get(seed_id)
            .expect(&format!("{}", SEED_NOT_EXIST));
        assert!(prev_balance >= &amount, "{}", NOT_ENOUGH_SEED);
        let cur_balance = prev_balance - amount;
        if cur_balance > 0 {
            self.seeds.insert(seed_id.clone(), cur_balance);
        } else {
            self.seeds.remove(seed_id);
        }
        cur_balance
    }

    pub fn get_rps(&self, farm_id: &FarmId) -> RPS {
        self.user_rps.get(farm_id).unwrap_or(RPS::default()).clone()
    }

    pub fn set_rps(&mut self, farm_id: &FarmId, rps: RPS) {
        if !self.user_rps.contains_key(farm_id) {
            self.rps_count = self.rps_count.checked_add(1).unwrap();
        }
        self.user_rps.insert(farm_id, &rps);
    }

    pub fn remove_rps(&mut self, farm_id: &FarmId) {
        if self.user_rps.contains_key(farm_id) {
            self.user_rps.remove(farm_id);
            self.rps_count = self.rps_count.checked_sub(1).unwrap();
        }
    }

    /// Returns amount of yocto near necessary to cover storage used by this data structure.
    pub fn storage_usage(&self) -> Balance {
        (MIN_FARMER_LENGTH
            + self.rewards.len() as u128 * (4 + MAX_ACCOUNT_LENGTH + 16)
            + self.seeds.len() as u128 * (4 + MAX_ACCOUNT_LENGTH + 16)
            + self.rps_count as u128 * (4 + 1 + 2 * MAX_ACCOUNT_LENGTH + 32))
            * env::storage_byte_cost()
    }
}

/// Versioned Farmer, used for lazy upgrade.
/// Which means this structure would upgrade automatically when used.
/// To achieve that, each time the new version comes in,
/// each function of this enum should be carefully re-code!
#[derive(BorshSerialize, BorshDeserialize)]
pub enum VersionedFarmer {
    V101(Farmer),
}

impl VersionedFarmer {
    pub fn new(farmer_id: AccountId, amount: Balance) -> Self {
        VersionedFarmer::V101(Farmer {
            amount: amount,
            rewards: HashMap::new(),
            seeds: HashMap::new(),
            user_rps: LookupMap::new(StorageKeys::UserRps {
                account_id: farmer_id.clone(),
            }),
            rps_count: 0,
        })
    }

    /// Upgrades from other versions to the currently used version.
    pub fn upgrade(self) -> Self {
        match self {
            VersionedFarmer::V101(farmer) => VersionedFarmer::V101(farmer),
        }
    }

    #[inline]
    #[allow(unreachable_patterns)]
    pub fn need_upgrade(&self) -> bool {
        match self {
            VersionedFarmer::V101(_) => false,
            _ => true,
        }
    }

    #[inline]
    #[allow(unreachable_patterns)]
    pub fn get_ref(&self) -> &Farmer {
        match self {
            VersionedFarmer::V101(farmer) => farmer,
            _ => unimplemented!(),
        }
    }

    #[inline]
    #[allow(unreachable_patterns)]
    pub fn get(self) -> Farmer {
        match self {
            VersionedFarmer::V101(farmer) => farmer,
            _ => unimplemented!(),
        }
    }

    #[inline]
    #[allow(unreachable_patterns)]
    pub fn get_ref_mut(&mut self) -> &mut Farmer {
        match self {
            VersionedFarmer::V101(farmer) => farmer,
            _ => unimplemented!(),
        }
    }
}
