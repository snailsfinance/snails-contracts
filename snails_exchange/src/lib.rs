//! This contract implements simple counter backed by storage on blockchain.
//!
//! The contract provides methods to [increment] / [decrement] counter and
//! [get it's current value][get_num] or [reset].
//!
//! [increment]: struct.Counter.html#method.increment
//! [decrement]: struct.Counter.html#method.decrement
//! [get_num]: struct.Counter.html#method.get_num
//! [reset]: struct.Counter.html#method.reset

use near_contract_standards::fungible_token::core_impl::ext_fungible_token;
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, Vector};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    assert_one_yocto, env, log, near_bindgen, AccountId, Balance, BorshStorageKey, PanicOnDefault,
    Promise, PromiseResult, StorageUsage,
};

use std::fmt;

use crate::utils::{
    assert_fees_info_valid, check_token_duplicates, ext_self, GAS_FOR_FT_TRANSFER,
    GAS_FOR_RESOLVE_TRANSFER,
};

#[derive(BorshStorageKey, BorshSerialize)]
pub(crate) enum StorageKey {
    Pools,
    Accounts,
    Shares { pool_id: u32 },
    AccountTokens { account_id: AccountId },
}

use crate::account::{Account, VAccount};
use crate::error::*;
pub use crate::fees::Fees;
use crate::pool::Pool;
use crate::simple_pool::SimplePool;
pub use crate::views::{ContractMetadata, PoolInfo};

mod account;
mod bigint;
mod error;
mod fees;
mod multi_fungible_token;
mod pool;
mod simple_pool;
mod snails;
mod storage_impl;
mod token_receiver;
mod utils;
mod views;

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

// add the following attributes to prepare your code for serialization and invocation on the blockchain
// More built-in Rust attributes here: https://doc.rust-lang.org/reference/attributes.html#built-in-attributes-index
#[near_bindgen]
#[derive(BorshSerialize, BorshDeserialize, PanicOnDefault)]
pub struct SnailSwap {
    owner_id: AccountId,
    /// List of all the pools.
    pools: Vector<Pool>,
    /// Running state
    state: RunningState,
    accounts: LookupMap<AccountId, VAccount>,
}

#[near_bindgen]
impl SnailSwap {
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        assert!(!env::state_exists(), "Already initialized");

        Self {
            owner_id: owner_id.clone(),
            pools: Vector::new(StorageKey::Pools),
            state: RunningState::Running,
            accounts: LookupMap::new(StorageKey::Accounts),
        }
    }

    /// Adds new "Simple Pool" with given tokens and given fee.
    /// Attached NEAR should be enough to cover the added storage.
    #[payable]
    pub fn add_simple_pool(
        &mut self,
        tokens: Vec<AccountId>,
        decimals: Vec<u64>,
        initial_amp_factor: u64,
        target_amp_factor: u64,
        start_ramp_ts: u64,
        stop_ramp_ts: u64,
        fees: Fees,
    ) -> u64 {
        self.assert_owner();
        self.assert_contract_running();
        check_token_duplicates(&tokens);

        assert_fees_info_valid(&fees);

        self.internal_add_pool(Pool::SimplePool(SimplePool::new(
            self.pools.len() as u32,
            initial_amp_factor as u64,
            target_amp_factor as u64,
            start_ramp_ts as u64,
            stop_ramp_ts as u64,
            fees,
            tokens,
            decimals,
        )))
    }

    /// Add liquidity from already deposited amounts to given pool.
    #[payable]
    pub fn add_liquidity(
        &mut self,
        pool_id: u64,
        tokens_amount: Vec<U128>,
        min_mint_amount: Option<U128>,
    ) -> Balance {
        self.assert_contract_running();
        assert!(
            env::attached_deposit() > 0,
            "Requires attached deposit of at least 1 yoctoNEAR"
        );

        let prev_storage = env::storage_usage();
        let sender_id = env::predecessor_account_id();

        /*3. deposit*/
        let mut pool = self.pools.get(pool_id).expect("ERR_NO_POOL");
        let amounts: Vec<u128> = tokens_amount
            .into_iter()
            .map(|amount| amount.into())
            .collect();

        // Add amounts given to liquidity first. It will return the balanced amounts.
        let (lp_shares, admin_fees) = pool.add_liquidity(&sender_id, &amounts);

        if let Some(min_amounts) = min_mint_amount {
            // Check that all amounts are above request min amounts in case of front running that changes the exchange rate.
            assert!(lp_shares >= min_amounts.0);
        }

        let mut deposits = self.internal_unwrap_or_default_account(&sender_id);

        let tokens = pool.tokens();

        // Subtract updated amounts from deposits. This will fail if there is not enough funds for any of the tokens.
        for i in 0..tokens.len() {
            deposits.withdraw(&tokens[i], amounts[i]);
        }

        self.transfer_admin_fees(&tokens, &admin_fees);
        self.internal_save_account(&sender_id, deposits);
        self.pools.replace(pool_id, &pool);
        self.internal_check_storage(prev_storage);

        lp_shares
    }

    fn transfer_admin_fees(&mut self, tokens: &[AccountId], admin_fees: &[u128]) {
        //allocate fees
        let mut exchange_account = self.internal_unwrap_or_default_account(&self.owner_id);
        for i in 0..tokens.len() {
            exchange_account.deposit(&tokens[i], admin_fees[i]);
        }
        self.internal_save_account(&self.owner_id.clone(), exchange_account);
    }

    /// Remove liquidity from the pool into general pool of liquidity.
    #[payable]
    pub fn remove_liquidity(&mut self, pool_id: u64, shares: U128, min_amounts: Vec<U128>) {
        assert_one_yocto();
        self.assert_contract_running();
        let prev_storage = env::storage_usage();
        let sender_id = env::predecessor_account_id();
        let mut pool = self.pools.get(pool_id).expect("ERR_NO_POOL");

        let (amounts, admin_fees) = pool.remove_liquidity(
            &sender_id,
            shares.into(),
            min_amounts
                .into_iter()
                .map(|amount| amount.into())
                .collect(),
        );

        let tokens = pool.tokens();
        let mut deposits = self.internal_unwrap_or_default_account(&sender_id);

        for i in 0..tokens.len() {
            deposits.deposit(&tokens[i], amounts[i]);
        }

        // Freed up storage balance from LP tokens will be returned to near_balance.
        if prev_storage > env::storage_usage() {
            deposits.near_amount = deposits
                .near_amount
                .checked_add(
                    ((prev_storage.checked_sub(env::storage_usage()).unwrap()) as Balance)
                        .checked_mul(env::storage_byte_cost())
                        .unwrap(),
                )
                .unwrap();
        }

        self.transfer_admin_fees(&tokens, &admin_fees);
        self.internal_save_account(&sender_id, deposits);
        self.pools.replace(pool_id, &pool);
        self.internal_check_storage(prev_storage);
    }

    /// Remove liquidity from the pool into general pool of liquidity.

    #[payable]
    pub fn remove_liquidity_imbalance(
        &mut self,
        pool_id: u64,
        remove_coin_amount: Vec<U128>,
        max_amount: Option<U128>,
    ) {
        assert_one_yocto();
        self.assert_contract_running();
        let prev_storage = env::storage_usage();
        let sender_id = env::predecessor_account_id();
        let mut pool = self.pools.get(pool_id).expect("ERR_NO_POOL");

        let remove_coin_amount: Vec<Balance> = remove_coin_amount
            .into_iter()
            .map(|amount| amount.into())
            .collect();

        let (removed_lp, admin_fees) =
            pool.remove_liquidity_imbalance(&sender_id, &remove_coin_amount);

        if let Some(x) = max_amount {
            assert!(x.0 >= removed_lp, "ERR_EXCEED_MAX_AMOUNT_LP_INPUT");
        }

        let tokens = pool.tokens();
        let mut deposits = self.internal_unwrap_or_default_account(&sender_id);

        for i in 0..tokens.len() {
            deposits.deposit(&tokens[i], remove_coin_amount[i]);
        }

        // Freed up storage balance from LP tokens will be returned to near_balance.
        if prev_storage > env::storage_usage() {
            deposits.near_amount = deposits
                .near_amount
                .checked_add(
                    ((prev_storage.checked_sub(env::storage_usage()).unwrap()) as Balance)
                        .checked_mul(env::storage_byte_cost())
                        .unwrap(),
                )
                .unwrap();
        }

        self.transfer_admin_fees(&tokens, &admin_fees);
        self.internal_save_account(&sender_id, deposits);
        self.pools.replace(pool_id, &pool);
        self.internal_check_storage(prev_storage);
    }

    #[payable]
    pub fn remove_liquidity_one_coin(
        &mut self,
        pool_id: u64,
        token_out: AccountId,
        remove_lp_amount: U128,
        min_amount: U128,
    ) {
        assert_one_yocto();
        self.assert_contract_running();
        let prev_storage = env::storage_usage();
        let sender_id = env::predecessor_account_id();
        let mut pool = self.pools.get(pool_id).expect("ERR_NO_POOL");

        let (amounts, admin_fees) = pool.remove_liquidity_one_coin(
            &sender_id,
            token_out.into(),
            remove_lp_amount.into(),
            min_amount.into(),
        );

        let tokens = pool.tokens();
        let mut deposits = self.internal_unwrap_or_default_account(&sender_id);

        for i in 0..tokens.len() {
            deposits.deposit(&tokens[i], amounts[i]);
        }

        // Freed up storage balance from LP tokens will be returned to near_balance.
        if prev_storage > env::storage_usage() {
            deposits.near_amount = deposits
                .near_amount
                .checked_add(
                    ((prev_storage.checked_sub(env::storage_usage()).unwrap()) as Balance)
                        .checked_mul(env::storage_byte_cost())
                        .unwrap(),
                )
                .unwrap();
        }

        self.transfer_admin_fees(&tokens, &admin_fees);
        self.internal_save_account(&sender_id, deposits);
        self.pools.replace(pool_id, &pool);
        self.internal_check_storage(prev_storage);
    }

    fn swap_core(
        &mut self,
        pool_id: u64,
        token_in: &AccountId,
        amount_in: Balance,
        token_out: &AccountId,
        minimum_amount_out: Balance,
    ) -> Balance {
        self.assert_contract_running();

        let mut pool = self.pools.get(pool_id).expect("ERR_NO_POOL");

        let (amount_out, admin_fee) = pool.swap(token_in, amount_in, token_out, minimum_amount_out);
        self.pools.replace(pool_id, &pool);
        //allocate fees
        let mut exchange_account = self.internal_unwrap_account(&self.owner_id);
        exchange_account.deposit(token_out, admin_fee);
        self.internal_save_account(&self.owner_id.clone(), exchange_account);

        amount_out.into()
    }

    #[payable]
    pub fn swap(
        &mut self,
        pool_id: u64,
        token_in: AccountId,
        amount_in: U128,
        token_out: AccountId,
        minimum_amount_out: U128,
    ) -> U128 {
        let sender_id = env::predecessor_account_id();
        let mut account = self.internal_unwrap_account(&sender_id);

        let amount_out = self.swap_core(
            pool_id,
            &token_in,
            amount_in.0,
            &token_out,
            minimum_amount_out.0,
        );
        account.withdraw(&token_in, amount_in.0);

        account.deposit(&token_out, amount_out);
        self.internal_save_account(&sender_id, account);

        amount_out.into()
    }

    pub fn change_fees_setting(&mut self, pool_id: u64, fees: Fees) {
        self.assert_owner();
        assert_fees_info_valid(&fees);

        let mut pool = self.pools.get(pool_id).expect("ERR_NO_POOL");

        pool.change_fees_setting(fees);
        self.pools.replace(pool_id, &pool);
    }

    pub fn set_amp_params(
        &mut self,
        pool_id: u64,
        initial_amp_factor: u64,
        target_amp_factor: u64,
        stop_ramp_ts: u64,
    ) {
        self.assert_owner();

        let mut pool = self.pools.get(pool_id).expect("ERR_NO_POOL");
        let start_ramp_ts = (near_sdk::env::block_timestamp() as u64) / (1e9 as u64);

        pool.set_amp_params(
            initial_amp_factor,
            target_amp_factor,
            start_ramp_ts,
            stop_ramp_ts,
        );
        self.pools.replace(pool_id, &pool);
    }

    /// Change state of contract, Only can be called by owner.
    #[payable]
    pub fn change_state(&mut self, state: RunningState) {
        assert_one_yocto();
        self.assert_owner();

        if self.state != state {
            if state == RunningState::Running {
                // only owner can resume the contract
                self.assert_owner();
            }
            env::log_str(
                format!(
                    "Contract state changed from {} to {} by {}",
                    self.state,
                    state,
                    env::predecessor_account_id()
                )
                .as_str(),
            );

            self.state = state;
        }
    }

    /// Check how much storage taken costs and refund the left over back.
    fn internal_check_storage(&self, prev_storage: StorageUsage) {
        let storage_cost = (env::storage_usage()
            .checked_sub(prev_storage)
            .unwrap_or_default() as Balance)
            .checked_mul(env::storage_byte_cost())
            .unwrap();

        env::log_str(
            format!(
                "SnailSwap internal_check_storage need: {}, attached: {}",
                storage_cost,
                env::attached_deposit()
            )
            .as_str(),
        );

        let refund = env::attached_deposit()
            .checked_sub(storage_cost)
            .expect("ERR_STORAGE_DEPOSIT");
        if refund > 0 {
            Promise::new(env::predecessor_account_id()).transfer(refund);
        }
    }
}

impl SnailSwap {
    fn assert_contract_running(&self) {
        match self.state {
            RunningState::Running => (),
            _ => env::panic_str(CONTRACT_PAUSED),
        };
    }

    fn assert_owner(&self) {
        let sender_id = env::predecessor_account_id();
        assert!(
            self.owner_id == sender_id,
            "ERR_NOT_OWNER owner [{}] sender [{}]",
            self.owner_id,
            sender_id
        );
    }

    /// Adds given pool to the list and returns it's id.
    /// If there is not enough attached balance to cover storage, fails.
    /// If too much attached - refunds it back.
    fn internal_add_pool(&mut self, pool: Pool) -> u64 {
        let prev_storage = env::storage_usage();
        let id = self.pools.len() as u64;
        self.pools.push(&pool);
        self.internal_check_storage(prev_storage);
        id
    }
}

#[near_bindgen]
impl SnailSwap {
    #[private]
    pub fn exchange_callback_post_withdraw(
        &mut self,
        token_id: AccountId,
        sender_id: AccountId,
        amount: U128,
    ) {
        assert_eq!(
            env::promise_results_count(),
            1,
            "{}",
            CALLBACK_POST_WITHDRAW_INVALID
        );
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(_) => {
                env::log_str(
                    format!("SnailSwap exchange_callback_post_withdraw success.").as_str(),
                );
            }
            PromiseResult::Failed => {
                // This reverts the changes from withdraw function.
                // If account doesn't exit, deposits to the owner's account as lostfound.
                let mut failed = false;
                if let Some(mut account) = self.internal_get_account(&sender_id) {
                    if account.deposit_with_storage_check(&token_id, amount.0) {
                        // cause storage already checked, here can directly save
                        self.accounts.insert(&sender_id, &account.into());
                    } else {
                        // we can ensure that internal_get_account here would NOT cause a version upgrade,
                        // cause it is callback, the account must be the current version or non-exist,
                        // so, here we can just leave it without insert, won't cause storage collection inconsistency.
                        env::log_str(
                            format!(
                                "Account {} has not enough storage. Depositing to owner.",
                                sender_id
                            )
                            .as_str(),
                        );
                        failed = true;
                    }
                } else {
                    env::log_str(
                        format!(
                            "Account {} is not registered. Depositing to owner.",
                            sender_id
                        )
                        .as_str(),
                    );
                    failed = true;
                }
                if failed {
                    self.internal_lostfound(&token_id, amount.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, Balance};
    use near_sdk_sim::to_yocto;

    use super::*;

    use near_sdk::serde::{Deserialize, Serialize};
    use near_sdk::serde_json;

    fn setup_fee() -> Fees {
        //initial A = 100, target = 500，time可以设计成2周。就是2周A线性过度到500
        //admin_trade_fee = 0.5 , admin_withdraw_fee = 0.4, trade_fee = 3/1000, withdraw_fee = 4/1000

        let admin_trade_fee_numerator: u64 = 50;
        let admin_trade_fee_denominator: u64 = 100;
        let admin_withdraw_fee_numerator: u64 = 40;
        let admin_withdraw_fee_denominator: u64 = 100;
        let trade_fee_numerator: u64 = 3;
        let trade_fee_denominator: u64 = 1000;
        let withdraw_fee_numerator: u64 = 4;
        let withdraw_fee_denominator: u64 = 1000;

        Fees {
            admin_trade_fee_numerator,
            admin_trade_fee_denominator,
            admin_withdraw_fee_numerator,
            admin_withdraw_fee_denominator,
            trade_fee_numerator,
            trade_fee_denominator,
            withdraw_fee_numerator,
            withdraw_fee_denominator,
        }
    }

    fn setup_contract() -> (VMContextBuilder, SnailSwap) {
        let mut context = VMContextBuilder::new();
        testing_env!(context.predecessor_account_id(accounts(0)).build());

        let mut contract = SnailSwap::new(accounts(0));

        //Must, used to collect fees
        testing_env!(context
            .predecessor_account_id(accounts(0).clone())
            .attached_deposit(to_yocto("1"))
            .build());
        contract.storage_deposit(Some(accounts(0).clone()), None);
        (context, contract)
    }

    fn deposit_tokens(
        context: &mut VMContextBuilder,
        contract: &mut SnailSwap,
        account_id: AccountId,
        token_amounts: Vec<(AccountId, Balance)>,
    ) {
        if contract.storage_balance_of(account_id.clone()).is_none() {
            testing_env!(context
                .predecessor_account_id(account_id.clone())
                .attached_deposit(to_yocto("1"))
                .build());
            contract.storage_deposit(None, None);
        }
        testing_env!(context
            .predecessor_account_id(account_id.clone())
            .attached_deposit(to_yocto("1"))
            .build());
        let tokens = token_amounts
            .iter()
            .map(|(token_id, _)| token_id.clone().into())
            .collect();
        testing_env!(context.attached_deposit(1).build());
        contract.register_tokens(tokens);
        for (token_id, amount) in token_amounts {
            testing_env!(context
                .predecessor_account_id(token_id)
                .attached_deposit(1)
                .build());
            contract.ft_on_transfer(account_id.clone(), U128(amount), "".to_string());
        }
    }

    fn create_pool_with_liquidity(
        context: &mut VMContextBuilder,
        contract: &mut SnailSwap,
        account_id: AccountId,
        token_amounts: Vec<(AccountId, Balance)>,
        decimals: Vec<u64>,
    ) -> u64 {
        let tokens = token_amounts
            .iter()
            .map(|(x, _)| x.clone())
            .collect::<Vec<_>>();
        testing_env!(context.predecessor_account_id(accounts(0)).build());
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .attached_deposit(env::storage_byte_cost() * 5500)
            .build());

        let initial_amp_factor: u64 = 100;
        let target_amp_factor: u64 = 500;
        let start_ramp_ts: u64 = 0;
        let stop_ramp_ts: u64 = 0;
        let fees: Fees = setup_fee();

        let pool_id = contract.add_simple_pool(
            tokens,
            decimals,
            initial_amp_factor,
            target_amp_factor,
            start_ramp_ts,
            stop_ramp_ts,
            fees,
        );

        testing_env!(context
            .predecessor_account_id(account_id.clone())
            .attached_deposit(to_yocto("1"))
            .build());
        contract.storage_deposit(None, None);
        deposit_tokens(context, contract, accounts(3), token_amounts.clone());
        testing_env!(context
            .predecessor_account_id(account_id.clone())
            .attached_deposit(to_yocto("0.008"))
            .build());

        let expected_lp = contract.try_add_liquidity(
            pool_id,
            token_amounts
                .clone()
                .into_iter()
                .map(|(_, x)| U128(x))
                .collect(),
        );

        contract.add_liquidity(
            pool_id,
            token_amounts.into_iter().map(|(_, x)| U128(x)).collect(),
            None,
        );

        assert_eq!(contract.get_pool_shares(0, accounts(3)), expected_lp);

        pool_id
    }

    #[derive(Serialize, Deserialize)]
    #[serde(crate = "near_sdk::serde")]
    //#[serde(untagged)]
    #[serde(tag = "type")]
    enum TokenReceiverMessage {
        /// Alternative to deposit + execute actions call.
        ///
        Execute {
            pool_id: u64,
        },
        Swap {
            pool_id: u64,
        },
    }

    #[test]
    fn test_tag_serde() {
        let j = "
{
    \"type\":\"Swap\",\"pool_id\":1
}";

        let message: TokenReceiverMessage =
            serde_json::from_str::<TokenReceiverMessage>(j).unwrap();
        match message {
            TokenReceiverMessage::Execute { pool_id: _ } => {}
            TokenReceiverMessage::Swap { pool_id: _ } => {}
        }
    }

    fn get_balance_with_decimals(balance: u128, decimals: u32) -> u128 {
        let base: u128 = 10;
        balance * base.pow(decimals) as u128
    }

    #[test]
    #[should_panic(expected = "Contract paused")]
    fn test_change_state() {
        const COIN_NUM: usize = 2;
        let (mut context, mut contract) = setup_contract();
        let token_decimals: [u32; COIN_NUM] = [18, 6];

        // add liquidity of (1,2) tokens
        create_pool_with_liquidity(
            &mut context,
            &mut contract,
            accounts(3),
            vec![
                (
                    accounts(1),
                    get_balance_with_decimals(10, token_decimals[0]),
                ),
                (
                    accounts(2),
                    get_balance_with_decimals(10, token_decimals[1]),
                ),
            ],
            vec![token_decimals[0].into(), token_decimals[1].into()],
        );

        deposit_tokens(
            &mut context,
            &mut contract,
            accounts(3),
            vec![
                (
                    accounts(1),
                    get_balance_with_decimals(100, token_decimals[0]),
                ),
                (
                    accounts(2),
                    get_balance_with_decimals(100, token_decimals[1]),
                ),
            ],
        );

        let mut context = VMContextBuilder::new();

        testing_env!(context
            .predecessor_account_id(accounts(0).clone())
            .attached_deposit(1)
            .build());

        contract.change_state(RunningState::Paused);

        deposit_tokens(&mut context, &mut contract, accounts(1), vec![]);
    }
    #[test]
    fn test_basics_two_coins() {
        const COIN_NUM: usize = 2;
        let (mut context, mut contract) = setup_contract();
        let token_decimals: [u32; COIN_NUM] = [18, 6];
        let one_token_amount_0 = get_balance_with_decimals(1, token_decimals[0]);
        let one_token_amount_1 = get_balance_with_decimals(1, token_decimals[1]);

        // add liquidity of (1,2) tokens
        create_pool_with_liquidity(
            &mut context,
            &mut contract,
            accounts(3),
            vec![
                (
                    accounts(1),
                    get_balance_with_decimals(10, token_decimals[0]),
                ),
                (
                    accounts(2),
                    get_balance_with_decimals(10, token_decimals[1]),
                ),
            ],
            vec![token_decimals[0].into(), token_decimals[1].into()],
        );

        deposit_tokens(
            &mut context,
            &mut contract,
            accounts(3),
            vec![
                (
                    accounts(1),
                    get_balance_with_decimals(100, token_decimals[0]),
                ),
                (
                    accounts(2),
                    get_balance_with_decimals(100, token_decimals[1]),
                ),
            ],
        );

        deposit_tokens(&mut context, &mut contract, accounts(1), vec![]);

        assert_eq!(
            contract.get_deposit(accounts(3), accounts(1)),
            get_balance_with_decimals(100, token_decimals[0]).into()
        );
        assert_eq!(
            contract.get_deposit(accounts(3), accounts(2)),
            get_balance_with_decimals(100, token_decimals[1]).into()
        );

        let lp_decimals: u32 = 24;
        assert_eq!(
            contract.get_pool_total_shares(0).0,
            get_balance_with_decimals(20, lp_decimals).into()
        );

        let get_amount_ret = contract.get_return(
            0,
            accounts(1).into(),
            one_token_amount_0.into(),
            accounts(2).into(),
        );

        testing_env!(context
            .predecessor_account_id(accounts(3))
            .attached_deposit(1)
            .build());
        let amount_out = contract.swap(
            0,
            accounts(1).into(),
            one_token_amount_0.into(),
            accounts(2).into(),
            0.into(),
        );

        assert_eq!(get_amount_ret, amount_out);

        assert_eq!(
            contract.get_deposit(accounts(3), accounts(1)).0,
            99 * one_token_amount_0
        );

        // transfer some of token_id 2 from acc 3 to acc 1.
        testing_env!(context.predecessor_account_id(accounts(3)).build());
        contract.mft_transfer(
            accounts(2).to_string(),
            accounts(1),
            U128(one_token_amount_1),
            None,
        );
        assert_eq!(
            contract.get_deposit(accounts(3), accounts(2)).0,
            99 * one_token_amount_1 + amount_out.0
        );
        assert_eq!(
            contract.get_deposit(accounts(1), accounts(2)).0,
            one_token_amount_1
        );

        testing_env!(context
            .predecessor_account_id(accounts(3))
            .attached_deposit(to_yocto("0.0067"))
            .build());
        contract.mft_register(":0".to_string(), accounts(1));
        testing_env!(context
            .predecessor_account_id(accounts(3))
            .attached_deposit(1)
            .build());
        // transfer 1m shares in pool 0 to acc 1.
        contract.mft_transfer(":0".to_string(), accounts(1), U128(1_000_000), None);

        let pool_id: u64 = 0;
        let remove_lp = contract.get_pool_shares(0, accounts(3));
        testing_env!(context.predecessor_account_id(accounts(3)).build());

        let expect_balances = contract.try_remove_liquidity(pool_id, remove_lp);

        let deposit1 = contract.get_deposit(accounts(3), accounts(1)).0;
        let deposit2 = contract.get_deposit(accounts(3), accounts(2)).0;

        contract.remove_liquidity(pool_id, remove_lp, vec![1.into(), 2.into()]);

        assert_eq!(
            contract.get_deposit(accounts(3), accounts(1)).0 - deposit1,
            expect_balances[0].0
        );

        assert_eq!(
            contract.get_deposit(accounts(3), accounts(2)).0 - deposit2,
            expect_balances[1].0
        );

        // Exchange fees left in the pool as liquidity + 1m from transfer.

        contract.withdraw(
            accounts(1),
            contract.get_deposit(accounts(3), accounts(1)),
            None,
        );
        assert_eq!(contract.get_deposit(accounts(3), accounts(1)).0, 0);

        //check fees
        let total_admin_fees = contract.get_pool_admin_fee(0);

        let deposit1 = contract.get_deposit(accounts(0), accounts(1)).0;
        let deposit2 = contract.get_deposit(accounts(0), accounts(2)).0;

        assert_eq!(total_admin_fees[0], deposit1);
        assert_eq!(total_admin_fees[1], deposit2);
    }

    #[test]
    fn test_basics_three_coins() {
        let (mut context, mut contract) = setup_contract();
        let token_decimals: [u32; 3] = [18, 6, 10];
        let one_token_amount_0 = get_balance_with_decimals(1, token_decimals[0]);
        let one_token_amount_1 = get_balance_with_decimals(1, token_decimals[1]);

        // add liquidity of (1,2) tokens
        create_pool_with_liquidity(
            &mut context,
            &mut contract,
            accounts(3),
            vec![
                (
                    accounts(1),
                    get_balance_with_decimals(10, token_decimals[0]),
                ),
                (
                    accounts(2),
                    get_balance_with_decimals(10, token_decimals[1]),
                ),
                (
                    accounts(4),
                    get_balance_with_decimals(10, token_decimals[2]),
                ),
            ],
            vec![
                token_decimals[0].into(),
                token_decimals[1].into(),
                token_decimals[2].into(),
            ],
        );

        deposit_tokens(
            &mut context,
            &mut contract,
            accounts(3),
            vec![
                (
                    accounts(1),
                    get_balance_with_decimals(100, token_decimals[0]),
                ),
                (
                    accounts(2),
                    get_balance_with_decimals(100, token_decimals[1]),
                ),
                (
                    accounts(4),
                    get_balance_with_decimals(100, token_decimals[2]),
                ),
            ],
        );

        deposit_tokens(&mut context, &mut contract, accounts(1), vec![]);

        assert_eq!(
            contract.get_deposit(accounts(3), accounts(1)),
            get_balance_with_decimals(100, token_decimals[0]).into()
        );
        assert_eq!(
            contract.get_deposit(accounts(3), accounts(2)),
            get_balance_with_decimals(100, token_decimals[1]).into()
        );
        assert_eq!(
            contract.get_deposit(accounts(3), accounts(4)),
            get_balance_with_decimals(100, token_decimals[2]).into()
        );

        let lp_decimals: u32 = 24;
        assert_eq!(
            contract.get_pool_total_shares(0).0,
            get_balance_with_decimals(30, lp_decimals).into()
        );

        let get_amount_ret = contract.get_return(
            0,
            accounts(1).into(),
            one_token_amount_0.into(),
            accounts(2).into(),
        );

        testing_env!(context
            .predecessor_account_id(accounts(3))
            .attached_deposit(1)
            .build());
        let amount_out = contract.swap(
            0,
            accounts(1).into(),
            one_token_amount_0.into(),
            accounts(2).into(),
            0.into(),
        );

        assert_eq!(get_amount_ret, amount_out);

        assert_eq!(
            contract.get_deposit(accounts(3), accounts(1)).0,
            99 * one_token_amount_0
        );

        // transfer some of token_id 2 from acc 3 to acc 1.
        testing_env!(context.predecessor_account_id(accounts(3)).build());
        contract.mft_transfer(
            accounts(2).to_string(),
            accounts(1),
            U128(one_token_amount_1),
            None,
        );
        assert_eq!(
            contract.get_deposit(accounts(3), accounts(2)).0,
            99 * one_token_amount_1 + amount_out.0
        );
        assert_eq!(
            contract.get_deposit(accounts(1), accounts(2)).0,
            one_token_amount_1
        );

        testing_env!(context
            .predecessor_account_id(accounts(3))
            .attached_deposit(to_yocto("0.0067"))
            .build());
        contract.mft_register(":0".to_string(), accounts(1));
        testing_env!(context
            .predecessor_account_id(accounts(3))
            .attached_deposit(1)
            .build());
        // transfer 1m shares in pool 0 to acc 1.
        contract.mft_transfer(":0".to_string(), accounts(1), U128(1_000_000), None);

        let pool_id: u64 = 0;
        let remove_lp = contract.get_pool_shares(0, accounts(3));
        testing_env!(context.predecessor_account_id(accounts(3)).build());

        let expect_balances = contract.try_remove_liquidity(pool_id, remove_lp);

        let deposit1 = contract.get_deposit(accounts(3), accounts(1)).0;
        let deposit2 = contract.get_deposit(accounts(3), accounts(2)).0;
        let deposit3 = contract.get_deposit(accounts(3), accounts(4)).0;

        contract.remove_liquidity(pool_id, remove_lp, vec![1.into(), 2.into(), 3.into()]);

        assert_eq!(
            contract.get_deposit(accounts(3), accounts(1)).0 - deposit1,
            expect_balances[0].0
        );

        assert_eq!(
            contract.get_deposit(accounts(3), accounts(2)).0 - deposit2,
            expect_balances[1].0
        );
        assert_eq!(
            contract.get_deposit(accounts(3), accounts(4)).0 - deposit3,
            expect_balances[2].0
        );

        // Exchange fees left in the pool as liquidity + 1m from transfer.

        contract.withdraw(
            accounts(1),
            contract.get_deposit(accounts(3), accounts(1)),
            None,
        );
        assert_eq!(contract.get_deposit(accounts(3), accounts(1)).0, 0);

        //check fees
        let total_admin_fees = contract.get_pool_admin_fee(0);

        let deposit1 = contract.get_deposit(accounts(0), accounts(1)).0;
        let deposit2 = contract.get_deposit(accounts(0), accounts(2)).0;
        let deposit3 = contract.get_deposit(accounts(0), accounts(4)).0;

        assert_eq!(total_admin_fees[0], deposit1);
        assert_eq!(total_admin_fees[1], deposit2);
        assert_eq!(total_admin_fees[2], deposit3);
    }

    /// Test liquidity management.
    #[test]
    fn test_liquidity_basic() {
        let token_decimals: u32 = 6;

        let (mut context, mut contract) = setup_contract();

        deposit_tokens(
            &mut context,
            &mut contract,
            accounts(0),
            vec![
                (accounts(1), to_yocto("0")),
                (accounts(2), to_yocto("0")),
                (accounts(4), to_yocto("0")),
            ],
        );
        testing_env!(context.predecessor_account_id(accounts(0)).build());
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .attached_deposit(env::storage_byte_cost() * 5500)
            .build());

        let initial_amp_factor: u64 = 100;
        let target_amp_factor: u64 = 500;
        let start_ramp_ts: u64 = 0;
        let stop_ramp_ts: u64 = 0;
        let fees: Fees = setup_fee();

        let id = contract.add_simple_pool(
            vec![accounts(1), accounts(2), accounts(4)],
            vec![6, 6, 6],
            initial_amp_factor,
            target_amp_factor,
            start_ramp_ts,
            stop_ramp_ts,
            fees,
        );

        testing_env!(context.predecessor_account_id(accounts(3)).build());
        deposit_tokens(
            &mut context,
            &mut contract,
            accounts(3),
            vec![
                (accounts(1), get_balance_with_decimals(100, token_decimals)),
                (accounts(2), get_balance_with_decimals(100, token_decimals)),
                (accounts(4), get_balance_with_decimals(100, token_decimals)),
            ],
        );

        testing_env!(context.predecessor_account_id(accounts(3)).build());

        let deposit1 = contract.get_deposit(accounts(0), accounts(1)).0;
        let deposit2 = contract.get_deposit(accounts(0), accounts(2)).0;
        let deposit3 = contract.get_deposit(accounts(0), accounts(4)).0;
        assert_eq!(deposit1, 0);
        assert_eq!(deposit2, 0);
        assert_eq!(deposit3, 0);

        testing_env!(context.attached_deposit(to_yocto("0.008")).build());

        let expected_lp = contract.try_add_liquidity(
            id,
            vec![
                U128(get_balance_with_decimals(50, token_decimals)),
                U128(get_balance_with_decimals(10, token_decimals)),
                U128(get_balance_with_decimals(20, token_decimals)),
            ],
        );

        let before_add_lp = contract.get_pool_shares(0, accounts(3));
        contract.add_liquidity(
            id,
            vec![
                U128(get_balance_with_decimals(50, token_decimals)),
                U128(get_balance_with_decimals(10, token_decimals)),
                U128(get_balance_with_decimals(20, token_decimals)),
            ],
            None,
        );

        assert_eq!(
            contract.get_pool_shares(0, accounts(3)).0 - before_add_lp.0,
            expected_lp.0
        );
        let expected_lp = contract.try_add_liquidity(
            id,
            vec![
                U128(get_balance_with_decimals(50, token_decimals)),
                U128(get_balance_with_decimals(50, token_decimals)),
                U128(get_balance_with_decimals(20, token_decimals)),
            ],
        );
        let before_add_lp = contract.get_pool_shares(0, accounts(3));

        contract.add_liquidity(
            id,
            vec![
                U128(get_balance_with_decimals(50, token_decimals)),
                U128(get_balance_with_decimals(50, token_decimals)),
                U128(get_balance_with_decimals(20, token_decimals)),
            ],
            None,
        );
        assert_eq!(
            contract.get_pool_shares(0, accounts(3)).0 - before_add_lp.0,
            expected_lp.0
        );
        let amounts = contract.get_pool(id).amounts;
        let deposit1 = contract.get_deposit(accounts(3), accounts(1)).0;
        let deposit2 = contract.get_deposit(accounts(3), accounts(2)).0;
        let deposit3 = contract.get_deposit(accounts(3), accounts(4)).0;

        assert_eq!(deposit1, get_balance_with_decimals(0, token_decimals));
        assert_eq!(deposit2, get_balance_with_decimals(40, token_decimals));
        assert_eq!(deposit3, get_balance_with_decimals(60, token_decimals));

        let all_amounts: u128 =
            deposit1 + amounts[0].0 + deposit2 + amounts[1].0 + deposit3 + amounts[2].0;
        let admin_fee = contract.get_pool_admin_fee(0);

        let all_amounts: u128 = all_amounts + admin_fee.into_iter().sum::<u128>();

        assert_eq!(all_amounts, get_balance_with_decimals(300, token_decimals));

        testing_env!(context.attached_deposit(1).build());

        let expect_balances =
            contract.try_remove_liquidity(id, U128(get_balance_with_decimals(1, token_decimals)));

        let deposit1 = contract.get_deposit(accounts(3), accounts(1)).0;
        let deposit2 = contract.get_deposit(accounts(3), accounts(2)).0;
        let deposit3 = contract.get_deposit(accounts(3), accounts(4)).0;

        contract.remove_liquidity(
            id,
            U128(get_balance_with_decimals(1, token_decimals)),
            vec![U128(0), U128(0), U128(0)],
        );

        assert_eq!(
            contract.get_deposit(accounts(3), accounts(1)).0 - deposit1,
            expect_balances[0].0
        );

        assert_eq!(
            contract.get_deposit(accounts(3), accounts(2)).0 - deposit2,
            expect_balances[1].0
        );
        assert_eq!(
            contract.get_deposit(accounts(3), accounts(4)).0 - deposit3,
            expect_balances[2].0
        );

        // Check that amounts add up to deposits.
        let amounts = contract.get_pool(id).amounts;
        let deposit1 = contract.get_deposit(accounts(3), accounts(1)).0;
        let deposit2 = contract.get_deposit(accounts(3), accounts(2)).0;
        let deposit3 = contract.get_deposit(accounts(3), accounts(4)).0;

        let all_amounts: u128 =
            deposit1 + amounts[0].0 + deposit2 + amounts[1].0 + deposit3 + amounts[2].0;
        let admin_fee = contract.get_pool_admin_fee(0).into_iter().sum::<u128>();
        let all_amounts = all_amounts + admin_fee;
        assert_eq!(all_amounts, get_balance_with_decimals(300, token_decimals));

        //check fees
        let deposit1 = contract.get_deposit(accounts(0), accounts(1)).0;
        let deposit2 = contract.get_deposit(accounts(0), accounts(2)).0;
        let deposit3 = contract.get_deposit(accounts(0), accounts(4)).0;
        let actual_total_fees = deposit1 + deposit2 + deposit3; //3 is the deposit token for add pool
        assert_eq!(admin_fee, actual_total_fees);
    }

    #[test]
    fn test_lp_shares_liquidity() {
        let (mut context, mut contract) = setup_contract();

        deposit_tokens(
            &mut context,
            &mut contract,
            accounts(0),
            vec![
                (accounts(1), to_yocto("0")),
                (accounts(2), to_yocto("0")),
                (accounts(4), to_yocto("0")),
            ],
        );
        testing_env!(context.predecessor_account_id(accounts(0)).build());
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .attached_deposit(env::storage_byte_cost() * 5500)
            .build());
        let initial_amp_factor: u64 = 100;
        let target_amp_factor: u64 = 500;
        let start_ramp_ts: u64 = 0;
        let stop_ramp_ts: u64 = 0;
        let fees: Fees = setup_fee();

        let id = contract.add_simple_pool(
            vec![accounts(1), accounts(2), accounts(4)],
            vec![6, 6, 6],
            initial_amp_factor,
            target_amp_factor,
            start_ramp_ts,
            stop_ramp_ts,
            fees,
        );

        let deposit_amount = 3000;

        testing_env!(context.predecessor_account_id(accounts(3)).build());
        deposit_tokens(
            &mut context,
            &mut contract,
            accounts(3),
            vec![
                (accounts(1), deposit_amount),
                (accounts(2), deposit_amount),
                (accounts(4), deposit_amount),
            ],
        );

        testing_env!(context.predecessor_account_id(accounts(3)).build());

        let deposit1 = contract.get_deposit(accounts(3), accounts(1)).0;
        let deposit2 = contract.get_deposit(accounts(3), accounts(2)).0;
        let deposit3 = contract.get_deposit(accounts(3), accounts(4)).0;
        assert_eq!(deposit1, deposit_amount.into());
        assert_eq!(deposit2, deposit_amount.into());
        assert_eq!(deposit3, deposit_amount.into());

        testing_env!(context.attached_deposit(1).build());

        let all_lp_shares = contract.get_pool_total_shares(0).0;
        assert_eq!(all_lp_shares, to_yocto("0").into());

        testing_env!(context.attached_deposit(to_yocto("0.008")).build());
        let before_add_lp = contract.get_pool_shares(0, accounts(3));
        let expected_lp = contract.try_add_liquidity(
            id,
            vec![
                U128(deposit_amount),
                U128(deposit_amount),
                U128(deposit_amount),
            ],
        );
        contract.add_liquidity(
            id,
            vec![
                U128(deposit_amount),
                U128(deposit_amount),
                U128(deposit_amount),
            ],
            None,
        );
        assert_eq!(
            contract.get_pool_shares(0, accounts(3)).0 - before_add_lp.0,
            expected_lp.0
        );
        let deposit1 = contract.get_deposit(accounts(3), accounts(1)).0;
        let deposit2 = contract.get_deposit(accounts(3), accounts(2)).0;
        let deposit3 = contract.get_deposit(accounts(3), accounts(4)).0;
        assert_eq!(deposit1, to_yocto("0").into());
        assert_eq!(deposit2, to_yocto("0").into());
        assert_eq!(deposit3, to_yocto("0").into());

        /*check all tokens*/
        let amounts = contract.get_pool(id).amounts;
        let pool_amounts = amounts[0].0 + amounts[1].0 + amounts[2].0;

        /*fees*/
        let deposit1_0 = contract.get_deposit(accounts(0), accounts(1)).0;
        let deposit2_0 = contract.get_deposit(accounts(0), accounts(2)).0;
        let deposit3_0 = contract.get_deposit(accounts(0), accounts(4)).0;
        let add_liquidity_fees = deposit1_0 + deposit2_0 + deposit3_0;

        assert_eq!(
            add_liquidity_fees + pool_amounts,
            (deposit_amount * 3).into()
        );

        let all_lp_shares = contract.get_pool_total_shares(0).0;
        assert_ne!(all_lp_shares, to_yocto("0").into());
        assert_eq!(all_lp_shares, contract.get_pool_shares(id, accounts(3)).0);

        testing_env!(context.attached_deposit(1).build());

        let expect_balances = contract.try_remove_liquidity(id, U128(all_lp_shares));

        let deposit1 = contract.get_deposit(accounts(3), accounts(1)).0;
        let deposit2 = contract.get_deposit(accounts(3), accounts(2)).0;
        let deposit3 = contract.get_deposit(accounts(3), accounts(4)).0;

        contract.remove_liquidity(id, U128(all_lp_shares), vec![U128(0), U128(0), U128(0)]);

        assert_eq!(
            contract.get_deposit(accounts(3), accounts(1)).0 - deposit1,
            expect_balances[0].0
        );

        assert_eq!(
            contract.get_deposit(accounts(3), accounts(2)).0 - deposit2,
            expect_balances[1].0
        );
        assert_eq!(
            contract.get_deposit(accounts(3), accounts(4)).0 - deposit3,
            expect_balances[2].0
        );
        let all_lp_shares = contract.get_pool_total_shares(0).0;
        assert_eq!(all_lp_shares, to_yocto("0").into());
        assert_eq!(all_lp_shares, contract.get_pool_shares(id, accounts(3)).0);

        let deposit1_3 = contract.get_deposit(accounts(3), accounts(1)).0;
        let deposit2_3 = contract.get_deposit(accounts(3), accounts(2)).0;
        let deposit3_3 = contract.get_deposit(accounts(3), accounts(4)).0;

        let account_3_amount = deposit1_3 + deposit2_3 + deposit3_3;

        let deposit1_0 = contract.get_deposit(accounts(0), accounts(1)).0;
        let deposit2_0 = contract.get_deposit(accounts(0), accounts(2)).0;
        let deposit3_0 = contract.get_deposit(accounts(0), accounts(4)).0;

        let account_0_amount = deposit1_0 + deposit2_0 + deposit3_0;

        let admin_fee = contract.get_pool_admin_fee(0).into_iter().sum::<u128>();

        assert_eq!(account_0_amount, admin_fee);

        let total_tokens = account_3_amount + admin_fee;

        assert_eq!(total_tokens, (deposit_amount * 3).into());

        let amounts = contract.get_pool(id).amounts;
        assert_eq!(amounts[0].0, 0);
        assert_eq!(amounts[1].0, 0);
        assert_eq!(amounts[2].0, 0);
    }

    fn set_up_liquidity(
        token_decimals: u32,
        common_deposit_amount: u32,
    ) -> (VMContextBuilder, SnailSwap, Balance) {
        let (mut context, mut contract) = setup_contract();

        let lp_token_decimals = 24;

        deposit_tokens(
            &mut context,
            &mut contract,
            accounts(0),
            vec![
                (accounts(1), to_yocto("0")),
                (accounts(2), to_yocto("0")),
                (accounts(4), to_yocto("0")),
            ],
        );
        testing_env!(context.predecessor_account_id(accounts(0)).build());
        testing_env!(context
            .predecessor_account_id(accounts(0))
            .attached_deposit(env::storage_byte_cost() * 5500)
            .build());

        let initial_amp_factor: u64 = 100;
        let target_amp_factor: u64 = 500;
        let start_ramp_ts: u64 = 0;
        let stop_ramp_ts: u64 = 0;
        let fees: Fees = setup_fee();

        let id = contract.add_simple_pool(
            vec![accounts(1), accounts(2), accounts(4)],
            vec![
                token_decimals as u64,
                token_decimals as u64,
                token_decimals as u64,
            ],
            initial_amp_factor,
            target_amp_factor,
            start_ramp_ts,
            stop_ramp_ts,
            fees,
        );

        testing_env!(context.predecessor_account_id(accounts(3)).build());
        deposit_tokens(
            &mut context,
            &mut contract,
            accounts(3),
            vec![
                (accounts(1), get_balance_with_decimals(100, token_decimals)),
                (accounts(2), get_balance_with_decimals(100, token_decimals)),
                (accounts(4), get_balance_with_decimals(100, token_decimals)),
            ],
        );

        testing_env!(context.predecessor_account_id(accounts(3)).build());

        let deposit1 = contract.get_deposit(accounts(0), accounts(1)).0;
        let deposit2 = contract.get_deposit(accounts(0), accounts(2)).0;
        let deposit3 = contract.get_deposit(accounts(0), accounts(4)).0;
        assert_eq!(deposit1, 0);
        assert_eq!(deposit2, 0);
        assert_eq!(deposit3, 0);

        testing_env!(context.attached_deposit(to_yocto("0.008")).build());
        let before_add_lp = contract.get_pool_shares(0, accounts(3));
        let expected_lp = contract.try_add_liquidity(
            id,
            vec![
                U128(get_balance_with_decimals(
                    common_deposit_amount as u128,
                    token_decimals,
                )),
                U128(get_balance_with_decimals(
                    common_deposit_amount as u128,
                    token_decimals,
                )),
                U128(get_balance_with_decimals(
                    common_deposit_amount as u128,
                    token_decimals,
                )),
            ],
        );

        contract.add_liquidity(
            id,
            vec![
                U128(get_balance_with_decimals(
                    common_deposit_amount as u128,
                    token_decimals,
                )),
                U128(get_balance_with_decimals(
                    common_deposit_amount as u128,
                    token_decimals,
                )),
                U128(get_balance_with_decimals(
                    common_deposit_amount as u128,
                    token_decimals,
                )),
            ],
            None,
        );
        assert_eq!(
            contract.get_pool_shares(0, accounts(3)).0 - before_add_lp.0,
            expected_lp.0
        );
        let lp_token: Balance = contract.get_pool_shares(0, accounts(3)).0;
        assert_eq!(
            get_balance_with_decimals(common_deposit_amount as u128 * 3, lp_token_decimals,),
            contract.get_pool_shares(0, accounts(3)).0
        );

        (context, contract, lp_token)
    }

    #[test]
    #[should_panic(expected = "ERR_LESS_THAN_MIN_AMOUNT")]
    fn test_remove_liquidity_less_than_min_amount() {
        let token_decimals: u32 = 6;
        let common_deposit_amount: u32 = 100;
        let id: u64 = 0;
        let (mut context, mut contract, lp_amount) =
            set_up_liquidity(token_decimals, common_deposit_amount);

        testing_env!(context.attached_deposit(1).build());

        //should failed here
        contract.remove_liquidity(
            id,
            U128(lp_amount),
            vec![U128(lp_amount), U128(lp_amount), U128(lp_amount)],
        );
    }

    #[test]
    fn test_remove_liquidity_imbalance() {
        let token_decimals: u32 = 6;
        let common_deposit_amount: u32 = 100;
        let id: u64 = 0;
        let (mut context, mut contract, _lp_amount) =
            set_up_liquidity(token_decimals, common_deposit_amount);

        testing_env!(context.attached_deposit(1).build());

        let expected_remove_lp = contract.try_remove_liquidity_imbalance(
            id,
            vec![
                U128(get_balance_with_decimals(50 as u128, token_decimals)),
                U128(get_balance_with_decimals(20 as u128, token_decimals)),
                U128(get_balance_with_decimals(10 as u128, token_decimals)),
            ],
        );

        contract.remove_liquidity_imbalance(
            id,
            vec![
                U128(get_balance_with_decimals(50 as u128, token_decimals)),
                U128(get_balance_with_decimals(20 as u128, token_decimals)),
                U128(get_balance_with_decimals(10 as u128, token_decimals)),
            ],
            None,
        );

        assert_eq!(
            _lp_amount - contract.get_pool_shares(0, accounts(3)).0,
            expected_remove_lp
        );

        let lp_token_decimals: u32 = 24;
        let lp_amount: Balance = contract.get_pool_shares(0, accounts(3)).0;

        assert!(lp_amount < get_balance_with_decimals(300 - 80, lp_token_decimals));
        assert!(get_balance_with_decimals(300 - 81, lp_token_decimals) < lp_amount);
    }

    #[test]
    #[should_panic(expected = "INVALID_INPUT_AMOUNT")]
    fn test_remove_liquidity_imbalance_exceed_deposit() {
        let token_decimals: u32 = 6;
        let common_deposit_amount: u32 = 100;
        let id: u64 = 0;
        let (mut context, mut contract, _lp_amount) =
            set_up_liquidity(token_decimals, common_deposit_amount);

        testing_env!(context.attached_deposit(1).build());

        contract.remove_liquidity_imbalance(
            id,
            vec![
                U128(get_balance_with_decimals(100 as u128, token_decimals)),
                U128(get_balance_with_decimals(20 as u128, token_decimals)),
                U128(get_balance_with_decimals(10 as u128, token_decimals)),
            ],
            None,
        );
    }

    #[test]
    #[should_panic(expected = "ERR_EXCEED_MAX_AMOUNT_LP_INPUT")]
    fn test_remove_liquidity_imbalance_exceed_max_amount() {
        let token_decimals: u32 = 6;
        let common_deposit_amount: u32 = 100;
        let id: u64 = 0;
        let (mut context, mut contract, _lp_amount) =
            set_up_liquidity(token_decimals, common_deposit_amount);

        testing_env!(context.attached_deposit(1).build());
        let expected_remove_lp = contract.try_remove_liquidity_imbalance(
            id,
            vec![
                U128(get_balance_with_decimals(10 as u128, token_decimals)),
                U128(get_balance_with_decimals(10 as u128, token_decimals)),
                U128(get_balance_with_decimals(10 as u128, token_decimals)),
            ],
        );
        contract.remove_liquidity_imbalance(
            id,
            vec![
                U128(get_balance_with_decimals(10 as u128, token_decimals)),
                U128(get_balance_with_decimals(10 as u128, token_decimals)),
                U128(get_balance_with_decimals(10 as u128, token_decimals)),
            ],
            Some(U128(expected_remove_lp - 1)),
        );
    }

    #[test]
    fn test_remove_liquidity_onecoin() {
        let token_decimals: u32 = 6;
        let common_deposit_amount: u32 = 100;
        let id: u64 = 0;
        let (mut context, mut contract, _lp_amount) =
            set_up_liquidity(token_decimals, common_deposit_amount);

        testing_env!(context.attached_deposit(1).build());
        let lp_decimals: u32 = 24;
        let remove_lp_amount = get_balance_with_decimals(99 as u128, lp_decimals);

        let expected_received_token =
            contract.try_remove_liquidity_one_coin(id, &accounts(1), U128(remove_lp_amount));

        let token_before_remove = contract.get_deposit(accounts(3), accounts(1));
        contract.remove_liquidity_one_coin(id, accounts(1), U128(remove_lp_amount), U128(0));
        let token_after_remove = contract.get_deposit(accounts(3), accounts(1));
        assert_eq!(
            token_after_remove.0 - token_before_remove.0,
            expected_received_token.0
        );
    }

    #[test]
    #[should_panic(expected = "ERR_EXCEED_MIN_AMOUNT")]
    fn test_remove_liquidity_onecoin_could_exceed_one_coin_balance() {
        let token_decimals: u32 = 6;
        let common_deposit_amount: u32 = 100;
        let id: u64 = 0;
        let (mut context, mut contract, _lp_amount) =
            set_up_liquidity(token_decimals, common_deposit_amount);

        testing_env!(context.attached_deposit(1).build());

        let lp_decimals: u32 = 24;
        let remove_lp_amount = get_balance_with_decimals(200 as u128, lp_decimals);

        let expected_received_token =
            contract.try_remove_liquidity_one_coin(id, &accounts(1), U128(remove_lp_amount));

        let token_before_remove = contract.get_deposit(accounts(3), accounts(1));

        contract.remove_liquidity_one_coin(
            id,
            accounts(1),
            U128(remove_lp_amount),
            U128(get_balance_with_decimals(200 as u128, lp_decimals)),
        );
        let token_after_remove = contract.get_deposit(accounts(3), accounts(1));

        assert_eq!(
            token_after_remove.0 - token_before_remove.0,
            expected_received_token.0
        );
    }

    #[test]
    fn test_remove_liquidity_onecoin_exceed_min_amount() {
        let token_decimals: u32 = 6;
        let common_deposit_amount: u32 = 100;
        let id: u64 = 0;
        let (mut context, mut contract, _lp_amount) =
            set_up_liquidity(token_decimals, common_deposit_amount);

        testing_env!(context.attached_deposit(1).build());

        let lp_decimals: u32 = 24;
        let remove_lp_amount = get_balance_with_decimals(200 as u128, lp_decimals);

        let expected_received_token =
            contract.try_remove_liquidity_one_coin(id, &accounts(1), U128(remove_lp_amount));

        let token_before_remove = contract.get_deposit(accounts(3), accounts(1));

        contract.remove_liquidity_one_coin(id, accounts(1), U128(remove_lp_amount), U128(0));

        let token_after_remove = contract.get_deposit(accounts(3), accounts(1));

        assert_eq!(
            token_after_remove.0 - token_before_remove.0,
            expected_received_token.0
        );
    }

    /// Test fee info change.
    #[test]
    fn test_fees_info_change() {
        let (_context, mut contract) = setup_contract();
        let initial_amp_factor: u64 = 100;
        let target_amp_factor: u64 = 500;
        let start_ramp_ts: u64 = 0;
        let stop_ramp_ts: u64 = 0;
        let mut fees: Fees = setup_fee();

        let id = contract.add_simple_pool(
            vec![accounts(1), accounts(2), accounts(4)],
            vec![6, 6, 6],
            initial_amp_factor,
            target_amp_factor,
            start_ramp_ts,
            stop_ramp_ts,
            fees,
        );

        assert_eq!(fees, contract.fees_info(id));

        fees.admin_trade_fee_numerator = 1 as u64;
        fees.admin_trade_fee_denominator = 2 as u64;
        fees.admin_withdraw_fee_numerator = 3 as u64;
        fees.admin_withdraw_fee_denominator = 3 as u64;
        fees.trade_fee_numerator = 123 as u64;
        fees.trade_fee_denominator = 431 as u64;
        fees.withdraw_fee_numerator = 153 as u64;
        fees.withdraw_fee_denominator = 431 as u64;

        assert_ne!(fees, contract.fees_info(id));

        contract.change_fees_setting(id, fees);

        assert_eq!(fees, contract.fees_info(id));
    }
}
