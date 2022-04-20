/*!
* Ref-Farming
*
* lib.rs is the main entry point.
*/
use crate::farm::{Farm, FarmId};
use crate::farm_seed::{SeedId, VersionedFarmSeed};
use crate::farmer::{Farmer, VersionedFarmer};
use crate::simple_farm::RPS;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, UnorderedMap};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::BorshStorageKey;
use near_sdk::{assert_one_yocto, env, near_bindgen, AccountId, Balance, PanicOnDefault};

// for simulator test
pub use crate::simple_farm::HRSimpleFarmTerms;
pub use crate::view::FarmInfo;

mod errors;
mod farm;
mod farm_seed;
mod farmer;
mod simple_farm;
mod storage_impl;
mod token_receiver;
mod utils;

mod actions_of_farm;
mod actions_of_reward;
mod actions_of_seed;
mod view;

mod owner;
use crate::errors::*;
use std::fmt;
#[derive(BorshStorageKey, BorshSerialize)]
pub enum StorageKeys {
    Seed,
    Farm,
    OutdatedFarm,
    Farmer,
    RewardInfo,
    UserRps { account_id: AccountId },
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub enum RunningState {
    Running,
    Paused,
}

impl fmt::Display for RunningState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RunningState::Running => write!(f, "Running"),
            RunningState::Paused => write!(f, "Paused"),
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct ContractData {
    // owner of this contract
    owner_id: AccountId,
    // record seeds and the farms under it.
    // seeds: UnorderedMap<SeedId, FarmSeed>,
    seeds: UnorderedMap<SeedId, VersionedFarmSeed>,

    // each farmer has a structure to describe
    // farmers: LookupMap<AccountId, Farmer>,
    farmers: LookupMap<AccountId, VersionedFarmer>,

    farms: UnorderedMap<FarmId, Farm>,
    outdated_farms: UnorderedMap<FarmId, Farm>,

    // for statistic
    farmer_count: u64,
    reward_info: UnorderedMap<AccountId, Balance>,

    /// Running state
    state: RunningState,
}

/// Versioned contract data. Allows to easily upgrade contracts.
#[derive(BorshSerialize, BorshDeserialize)]
pub enum VersionedContractData {
    Current(ContractData),
}

impl VersionedContractData {}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    data: VersionedContractData,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        Self {
            data: VersionedContractData::Current(ContractData {
                owner_id: owner_id.into(),
                farmer_count: 0,
                seeds: UnorderedMap::new(StorageKeys::Seed),
                farmers: LookupMap::new(StorageKeys::Farmer),
                farms: UnorderedMap::new(StorageKeys::Farm),
                outdated_farms: UnorderedMap::new(StorageKeys::OutdatedFarm),
                reward_info: UnorderedMap::new(StorageKeys::RewardInfo),
                state: RunningState::Running,
            }),
        }
    }

    /// Change state of contract, Only can be called by owner or guardians.
    #[payable]
    pub fn change_state(&mut self, state: RunningState) {
        assert_one_yocto();
        self.assert_owner();

        if self.data_mut().state != state {
            if state == RunningState::Running {
                // only owner can resume the contract
                self.assert_owner();
            }
            env::log_str(
                format!(
                    "Contract state changed from {} to {} by {}",
                    self.data_mut().state,
                    state,
                    env::predecessor_account_id()
                )
                .as_str(),
            );

            self.data_mut().state = state;
        }
    }
}

impl Contract {
    fn data(&self) -> &ContractData {
        match &self.data {
            VersionedContractData::Current(data) => data,
        }
    }

    fn data_mut(&mut self) -> &mut ContractData {
        match &mut self.data {
            VersionedContractData::Current(data) => data,
        }
    }

    fn assert_contract_running(&self) {
        match self.data().state {
            RunningState::Running => (),
            _ => env::panic_str(CONTRACT_PAUSED),
        };
    }
}

#[cfg(test)]
mod tests {

    use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
    use near_contract_standards::storage_management::{StorageBalance, StorageManagement};
    use near_sdk::json_types::U128;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, Balance};
    use simple_farm::HRSimpleFarmTerms;

    use super::utils::*;
    use super::*;

    fn setup_contract() -> (VMContextBuilder, Contract) {
        let mut context = VMContextBuilder::new();
        testing_env!(context.predecessor_account_id(accounts(0)).build());
        let contract = Contract::new(accounts(0));
        (context, contract)
    }

    fn create_farm(
        context: &mut VMContextBuilder,
        contract: &mut Contract,
        seed: AccountId,
        reward: AccountId,
        session_amount: Balance,
        session_interval: u32,
    ) -> FarmId {
        // storage needed: 341
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .attached_deposit(env::storage_byte_cost() * 559)
            .build());
        contract.create_simple_farm(
            HRSimpleFarmTerms {
                seed_id: seed.into(),
                reward_token: reward.into(),
                start_at: 0,
                reward_per_session: U128(session_amount),
                session_interval: session_interval,
            },
            Some(U128(10)),
        )
    }

    fn deposit_reward(
        context: &mut VMContextBuilder,
        contract: &mut Contract,
        amount: u128,
        time_stamp: u32,
    ) {
        testing_env!(context
            .predecessor_account_id(accounts(2))
            .block_timestamp(to_nano(time_stamp))
            .attached_deposit(1)
            .build());
        contract.ft_on_transfer(accounts(0), U128(amount), String::from("bob#0"));
    }

    fn register_farmer(
        context: &mut VMContextBuilder,
        contract: &mut Contract,
        farmer: AccountId,
    ) -> StorageBalance {
        testing_env!(context
            .predecessor_account_id(farmer.clone())
            .is_view(false)
            .attached_deposit(env::storage_byte_cost() * 1852)
            .build());
        contract.storage_deposit(Some(farmer), Some(true))
    }

    fn storage_withdraw(
        context: &mut VMContextBuilder,
        contract: &mut Contract,
        farmer: AccountId,
    ) -> StorageBalance {
        testing_env!(context
            .predecessor_account_id(farmer.clone())
            .is_view(false)
            .attached_deposit(1)
            .build());
        contract.storage_withdraw(None)
    }

    fn deposit_seed(
        context: &mut VMContextBuilder,
        contract: &mut Contract,
        farmer: AccountId,
        time_stamp: u32,
        amount: Balance,
    ) {
        testing_env!(context
            .predecessor_account_id(accounts(1))
            .is_view(false)
            .block_timestamp(to_nano(time_stamp))
            .attached_deposit(1)
            .build());
        contract.ft_on_transfer(farmer, U128(amount), String::from(""));
    }

    fn withdraw_seed(
        context: &mut VMContextBuilder,
        contract: &mut Contract,
        farmer: AccountId,
        time_stamp: u32,
        amount: Balance,
    ) {
        testing_env!(context
            .predecessor_account_id(farmer)
            .is_view(false)
            .block_timestamp(to_nano(time_stamp))
            .attached_deposit(1)
            .build());
        contract.withdraw_seed(accounts(1).into(), U128(amount));
    }

    fn claim_reward(
        context: &mut VMContextBuilder,
        contract: &mut Contract,
        farmer: AccountId,
        time_stamp: u32,
    ) {
        testing_env!(context
            .predecessor_account_id(farmer)
            .is_view(false)
            .block_timestamp(to_nano(time_stamp))
            .attached_deposit(1)
            .build());
        contract.claim_reward_by_farm(String::from("bob#0"));
    }

    fn claim_reward_by_seed(
        context: &mut VMContextBuilder,
        contract: &mut Contract,
        farmer: AccountId,
        time_stamp: u32,
    ) {
        testing_env!(context
            .predecessor_account_id(farmer)
            .is_view(false)
            .block_timestamp(to_nano(time_stamp))
            .attached_deposit(1)
            .build());
        contract.claim_reward_by_seed(String::from("bob"));
    }

    fn remove_farm(context: &mut VMContextBuilder, contract: &mut Contract, time_stamp: u32) {
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .is_view(false)
            .block_timestamp(to_nano(time_stamp))
            .build());
        contract.force_clean_farm(String::from("bob#0"));
    }

    fn remove_user_rps(
        context: &mut VMContextBuilder,
        contract: &mut Contract,
        farmer: AccountId,
        farm_id: String,
        time_stamp: u32,
    ) -> bool {
        testing_env!(context
            .predecessor_account_id(farmer)
            .is_view(false)
            .block_timestamp(to_nano(time_stamp))
            .build());
        contract.remove_user_rps_by_farm(farm_id)
    }

    fn to_yocto(value: &str) -> u128 {
        let vals: Vec<_> = value.split('.').collect();
        let part1 = vals[0].parse::<u128>().unwrap() * 10u128.pow(24);
        if vals.len() > 1 {
            let power = vals[1].len() as u32;
            let part2 = vals[1].parse::<u128>().unwrap() * 10u128.pow(24 - power);
            part1.checked_add(part2).unwrap()
        } else {
            part1
        }
    }

    #[test]
    fn test_basics() {
        let (mut context, mut contract) = setup_contract();
        // seed is bob, reward is charlie
        let farm_id = create_farm(
            &mut context,
            &mut contract,
            accounts(1),
            accounts(2),
            5000,
            50,
        );
        assert_eq!(farm_id, String::from("bob#0"));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.farm_kind, String::from("SIMPLE_FARM"));
        assert_eq!(farm_info.farm_status, String::from("Created"));
        assert_eq!(farm_info.seed_id, String::from("bob"));
        assert_eq!(
            farm_info.reward_token,
            AccountId::try_from(String::from("charlie")).unwrap()
        );
        assert_eq!(farm_info.reward_per_session, U128(5000));
        assert_eq!(farm_info.session_interval, 50);

        // deposit 50k, can last 10 rounds from 0 to 9
        deposit_reward(&mut context, &mut contract, 50000, 100);
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.farm_status, String::from("Running"));
        assert_eq!(farm_info.start_at, 100);

        // Farmer accounts(0) come in round 1
        register_farmer(&mut context, &mut contract, accounts(0));
        deposit_seed(&mut context, &mut contract, accounts(0), 160, 10);
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed, U128(0));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.beneficiary_reward, U128(5000));
        assert_eq!(farm_info.cur_round, 1);
        assert_eq!(farm_info.last_round, 1);

        // move to round 2, 5k unclaimed for accounts(0)
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .block_timestamp(to_nano(210))
            .is_view(true)
            .build());
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed, U128(5000));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 2);
        assert_eq!(farm_info.last_round, 1);

        // Farmer accounts(3) come in
        register_farmer(&mut context, &mut contract, accounts(3));
        // deposit seed
        deposit_seed(&mut context, &mut contract, accounts(3), 260, 10);
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed, U128(0));
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed, U128(10000));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 3);
        assert_eq!(farm_info.last_round, 3);

        // move to round 4,
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .block_timestamp(to_nano(320))
            .is_view(true)
            .build());
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed, U128(12500));
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed, U128(2500));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 4);
        assert_eq!(farm_info.last_round, 3);

        // remove all seeds at round 5
        println!("----> remove all seeds at round 5");
        withdraw_seed(&mut context, &mut contract, accounts(0), 360, 10);
        withdraw_seed(&mut context, &mut contract, accounts(3), 370, 10);
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .block_timestamp(to_nano(380))
            .is_view(true)
            .build());
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed, U128(0));
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed, U128(0));
        let rewarded = contract.get_reward(accounts(0), accounts(2));
        assert_eq!(rewarded, U128(15000));
        let rewarded = contract.get_reward(accounts(3), accounts(2));
        assert_eq!(rewarded, U128(5000));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 5);
        assert_eq!(farm_info.last_round, 5);

        // move to round 7, account3 come in again
        println!("----> move to round 7, account3 come in again");
        deposit_seed(&mut context, &mut contract, accounts(3), 460, 10);
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed, U128(0));
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed, U128(0));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.beneficiary_reward, U128(15000));
        assert_eq!(farm_info.cur_round, 7);
        assert_eq!(farm_info.last_round, 7);

        // move to round 8, account0 come in again
        println!("----> move to round 8, account0 come in again");
        deposit_seed(&mut context, &mut contract, accounts(0), 520, 10);
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed, U128(5000));
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed, U128(0));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 8);
        assert_eq!(farm_info.last_round, 8);

        // move to round 9,
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .block_timestamp(to_nano(580))
            .is_view(true)
            .build());
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed, U128(2500));
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed, U128(7500));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 9);
        assert_eq!(farm_info.last_round, 8);
        assert_eq!(farm_info.farm_status, String::from("Running"));

        // move to round 10,
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .block_timestamp(to_nano(610))
            .is_view(true)
            .build());
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed, U128(5000));
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed, U128(10000));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 10);
        assert_eq!(farm_info.last_round, 8);
        assert_eq!(farm_info.farm_status, String::from("Ended"));

        // claim reward
        println!("----> accounts(0) and accounts(3) claim reward");
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .block_timestamp(to_nano(710))
            .is_view(true)
            .build());
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed, U128(5000));
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed, U128(10000));
        claim_reward(&mut context, &mut contract, accounts(0), 720);
        claim_reward(&mut context, &mut contract, accounts(3), 730);
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .block_timestamp(to_nano(740))
            .is_view(true)
            .build());
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed, U128(0));
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed, U128(0));
        let rewarded = contract.get_reward(accounts(0), accounts(2));
        assert_eq!(rewarded, U128(20000));
        let rewarded = contract.get_reward(accounts(3), accounts(2));
        assert_eq!(rewarded, U128(15000));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 10);
        assert_eq!(farm_info.last_round, 10);

        // clean farm
        println!("----> clean farm");
        remove_farm(&mut context, &mut contract, 750);
        assert!(contract.get_farm(farm_id.clone()).is_none());

        // remove user rps
        println!("----> remove user rps");
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .block_timestamp(to_nano(760))
            .is_view(true)
            .build());
        let prev_available = contract
            .storage_balance_of(accounts(0))
            .expect("Error")
            .available
            .0;
        let ret = remove_user_rps(
            &mut context,
            &mut contract,
            accounts(0).into(),
            String::from("bob#0"),
            770,
        );
        assert!(ret);
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .block_timestamp(to_nano(780))
            .is_view(true)
            .build());
        let post_available = contract
            .storage_balance_of(accounts(0))
            .expect("Error")
            .available
            .0;
        assert_eq!(post_available - prev_available, 165 * 10_u128.pow(19));

        // withdraw seed
        println!("----> accounts(0) and accounts(3) withdraw seed");
        withdraw_seed(&mut context, &mut contract, accounts(0), 800, 10);
        withdraw_seed(&mut context, &mut contract, accounts(3), 810, 10);
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .block_timestamp(to_nano(820))
            .is_view(true)
            .build());
        let rewarded = contract.get_reward(accounts(0), accounts(2));
        assert_eq!(rewarded, U128(20000));
        let rewarded = contract.get_reward(accounts(3), accounts(2));
        assert_eq!(rewarded, U128(15000));
    }

    #[test]
    fn test_unclaimed_rewards() {
        let (mut context, mut contract) = setup_contract();
        // seed is bob, reward is charlie
        let farm_id = create_farm(
            &mut context,
            &mut contract,
            accounts(1),
            accounts(2),
            to_yocto("1"),
            50,
        );
        assert_eq!(farm_id, String::from("bob#0"));

        // deposit 10, can last 10 rounds from 0 to 9
        deposit_reward(&mut context, &mut contract, to_yocto("10"), 100);

        // Farmer1 accounts(0) come in round 0
        register_farmer(&mut context, &mut contract, accounts(0));
        deposit_seed(&mut context, &mut contract, accounts(0), 110, to_yocto("1"));
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed, U128(0));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 0);
        assert_eq!(farm_info.last_round, 0);
        assert_eq!(farm_info.claimed_reward.0, 0);
        assert_eq!(farm_info.unclaimed_reward.0, 0);

        // move to round 1,
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .block_timestamp(to_nano(160))
            .is_view(true)
            .build());
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("1"));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 1);
        assert_eq!(farm_info.last_round, 0);
        assert_eq!(farm_info.claimed_reward.0, to_yocto("0"));
        assert_eq!(farm_info.unclaimed_reward.0, to_yocto("1"));

        // Farmer2 accounts(3) come in round 1
        register_farmer(&mut context, &mut contract, accounts(3));
        // deposit seed
        deposit_seed(&mut context, &mut contract, accounts(3), 180, to_yocto("1"));
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("1"));
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("0"));

        // move to round 2,
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .block_timestamp(to_nano(210))
            .is_view(true)
            .build());
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("1.5"));
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("0.5"));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 2);
        assert_eq!(farm_info.last_round, 1);
        assert_eq!(farm_info.claimed_reward.0, to_yocto("0"));
        assert_eq!(farm_info.unclaimed_reward.0, to_yocto("2"));

        // farmer1 claim reward by farm_id at round 3
        claim_reward(&mut context, &mut contract, accounts(0), 260);
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("0"));
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("1"));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 3);
        assert_eq!(farm_info.last_round, 3);
        assert_eq!(farm_info.claimed_reward.0, to_yocto("2"));
        assert_eq!(farm_info.unclaimed_reward.0, to_yocto("1"));

        // farmer2 claim reward by seed_id at round 4
        claim_reward_by_seed(&mut context, &mut contract, accounts(3), 310);
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("0.5"));
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("0"));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 4);
        assert_eq!(farm_info.last_round, 4);
        assert_eq!(farm_info.claimed_reward.0, to_yocto("3.5"));
        assert_eq!(farm_info.unclaimed_reward.0, to_yocto("0.5"));

        // farmer1 unstake half lpt at round 5
        withdraw_seed(
            &mut context,
            &mut contract,
            accounts(0),
            360,
            to_yocto("0.4"),
        );
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("0"));
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("0.5"));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 5);
        assert_eq!(farm_info.last_round, 5);
        assert_eq!(farm_info.claimed_reward.0, to_yocto("4.5"));
        assert_eq!(farm_info.unclaimed_reward.0, to_yocto("0.5"));

        // farmer2 unstake all his lpt at round 6
        withdraw_seed(&mut context, &mut contract, accounts(3), 410, to_yocto("1"));
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("0.375"));
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("0"));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 6);
        assert_eq!(farm_info.last_round, 6);
        assert_eq!(farm_info.claimed_reward.0, to_yocto("5.625"));
        assert_eq!(farm_info.unclaimed_reward.0, to_yocto("0.375"));

        // move to round 7
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .block_timestamp(to_nano(460))
            .is_view(true)
            .build());
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("1.374999999999999999999999"));
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("0"));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 7);
        assert_eq!(farm_info.last_round, 6);
        assert_eq!(farm_info.claimed_reward.0, to_yocto("5.625"));
        assert_eq!(farm_info.unclaimed_reward.0, to_yocto("1.375"));
        withdraw_seed(
            &mut context,
            &mut contract,
            accounts(0),
            470,
            to_yocto("0.6"),
        );
        let unclaimed = contract.get_unclaimed_reward(accounts(0), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("0"));
        let unclaimed = contract.get_unclaimed_reward(accounts(3), farm_id.clone());
        assert_eq!(unclaimed.0, to_yocto("0"));
        let farm_info = contract.get_farm(farm_id.clone()).expect("Error");
        assert_eq!(farm_info.cur_round, 7);
        assert_eq!(farm_info.last_round, 7);
        assert_eq!(
            farm_info.claimed_reward.0,
            to_yocto("6.999999999999999999999999")
        );
        assert_eq!(farm_info.unclaimed_reward.0, 1);
    }

    #[test]
    #[should_panic(expected = "Insufficient storage deposit")]
    fn test_storage_withdraw() {
        let (mut context, mut contract) = setup_contract();
        // Farmer1 accounts(0) come in round 0
        register_farmer(&mut context, &mut contract, accounts(0));
        // println!("locked: {}, deposited: {}", sb.total.0, sb.available.0);
        let sb = storage_withdraw(&mut context, &mut contract, accounts(0));
        // println!("locked: {}, deposited: {}", sb.total.0, sb.available.0);
        assert_eq!(sb.total.0, 920000000000000000000);
        assert_eq!(sb.available.0, 0);

        let farm_id = create_farm(
            &mut context,
            &mut contract,
            accounts(1),
            accounts(2),
            5000,
            50,
        );
        assert_eq!(farm_id, String::from("bob#0"));

        deposit_seed(&mut context, &mut contract, accounts(0), 60, 10);
    }
}
