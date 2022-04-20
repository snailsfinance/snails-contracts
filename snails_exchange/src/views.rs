//! View functions for the contract.

use std::collections::HashMap;

use crate::*;
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{near_bindgen, AccountId};

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(not(target_arch = "wasm32"), derive(Deserialize, Debug))]
pub struct ContractMetadata {
    pub version: String,
    pub owner: AccountId,
    pub pool_count: u64,
    pub state: RunningState,
}

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct RefStorageState {
    pub deposit: U128,
    pub usage: U128,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug, PartialEq))]
pub struct PoolInfo {
    /// List of tokens in the pool.
    pub token_account_ids: Vec<AccountId>,
    pub token_decimals: Vec<u64>,
    /// How much NEAR this contract has.
    pub amounts: Vec<U128>,
    /// Total number of shares.
    pub shares_total_supply: U128,

    /// Initial amplification coefficient (A)
    pub initial_amp_factor: U128,
    /// Target amplification coefficient (A)
    pub target_amp_factor: U128,
    /// Ramp A start timestamp
    pub start_ramp_ts: U128,
    /// Ramp A stop timestamp
    pub stop_ramp_ts: U128,
}

impl From<Pool> for PoolInfo {
    fn from(pool: Pool) -> Self {
        match pool {
            Pool::SimplePool(pool) => Self {
                token_account_ids: pool.token_account_ids,
                token_decimals: pool.token_decimals,
                amounts: pool.amounts.into_iter().map(|a| U128(a)).collect(),
                shares_total_supply: U128(pool.shares_total_supply),
                initial_amp_factor: U128(pool.initial_amp_factor.into()),
                target_amp_factor: U128(pool.target_amp_factor.into()),
                start_ramp_ts: U128(pool.start_ramp_ts.into()),
                stop_ramp_ts: U128(pool.stop_ramp_ts.into()),
            },
        }
    }
}

#[near_bindgen]
impl SnailSwap {
    /// Returns semver of this contract.
    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// Returns number of pools.
    pub fn get_number_of_pools(&self) -> u64 {
        self.pools.len()
    }

    /// Returns list of pools of given length from given start index.
    pub fn get_pools(&self, from_index: u64, limit: u64) -> Vec<PoolInfo> {
        (from_index..std::cmp::min(from_index + limit, self.pools.len()))
            .map(|index| self.get_pool(index))
            .collect()
    }

    /// Returns information about specified pool.
    pub fn get_pool(&self, pool_id: u64) -> PoolInfo {
        self.pools.get(pool_id).expect("ERR_NO_POOL").into()
    }

    /// Return total fee of the given pool.
    pub fn get_pool_fee(&self, pool_id: u64) -> Vec<u128> {
        self.pools.get(pool_id).expect("ERR_NO_POOL").get_fee()
    }

    pub fn get_pool_admin_fee(&self, pool_id: u64) -> Vec<u128> {
        self.pools
            .get(pool_id)
            .expect("ERR_NO_POOL")
            .get_admin_fee()
    }

    /// Returns number of shares given account has in given pool.
    pub fn get_pool_shares(&self, pool_id: u64, account_id: AccountId) -> U128 {
        self.pools
            .get(pool_id)
            .expect("ERR_NO_POOL")
            .share_balances(&account_id)
            .into()
    }

    /// Returns total number of shares in the given pool.
    pub fn get_pool_total_shares(&self, pool_id: u64) -> U128 {
        self.pools
            .get(pool_id)
            .expect("ERR_NO_POOL")
            .share_total_balance()
            .into()
    }

    /// returns all pools we have
    pub fn pool_len(&self) -> u64 {
        self.pools.len().into()
    }

    /// returns pool total supply
    pub fn pool_total_supply(&self, pool_id: u64) -> Balance {
        let pool = self.pools.get(pool_id).expect("ERR_NO_POOL");

        match pool {
            Pool::SimplePool(pool) => pool.shares_total_supply,
        }
    }

    /// Returns balances of the deposits for given user outside of any pools.
    /// Returns empty list if no tokens deposited.
    pub fn get_deposits(&self, account_id: AccountId) -> HashMap<AccountId, U128> {
        let wrapped_account = self.internal_get_account(&account_id);
        if let Some(account) = wrapped_account {
            account
                .get_tokens()
                .iter()
                .map(|token| (token.clone(), U128(account.get_balance(token).unwrap())))
                .collect()
        } else {
            HashMap::new()
        }
    }

    /// Returns balance of the deposit for given user outside of any pools.
    pub fn get_deposit(&self, account_id: AccountId, token_id: AccountId) -> U128 {
        self.internal_get_deposit(&account_id, &token_id).into()
    }

    /// Given specific pool, returns amount of token_out recevied swapping amount_in of token_in.
    pub fn get_return(
        &self,
        pool_id: u64,
        token_in: AccountId,
        amount_in: U128,
        token_out: AccountId,
    ) -> U128 {
        let pool = self.pools.get(pool_id).expect("ERR_NO_POOL");
        pool.get_return(&token_in, amount_in.into(), &token_out)
            .into()
    }

    pub fn get_virtual_price(&self, pool_id: u64) -> U128 {
        let pool = self.pools.get(pool_id).expect("ERR_NO_POOL");
        pool.get_virtual_price().into()
    }

    pub fn get_amp_factor(&self, pool_id: u64) -> U128 {
        let pool = self.pools.get(pool_id).expect("ERR_NO_POOL");
        pool.get_amp_factor().into()
    }

    pub fn fees_info(&self, pool_id: u64) -> Fees {
        let pool = self.pools.get(pool_id).expect("ERR_NO_POOL");
        pool.fees_info()
    }

    pub fn try_remove_liquidity_one_coin(
        &self,
        pool_id: u64,
        token_out: &AccountId,
        remove_lp_amount: U128,
    ) -> U128 {
        let pool = self.pools.get(pool_id).expect("ERR_NO_POOL");
        pool.try_remove_liquidity_one_coin(token_out, remove_lp_amount.0)
            .into()
    }

    pub fn try_remove_liquidity_imbalance(
        &self,
        pool_id: u64,
        remove_coin_amount: Vec<U128>,
    ) -> u128 {
        let pool = self.pools.get(pool_id).expect("ERR_NO_POOL");

        let remove_coin_amount: Vec<u128> = remove_coin_amount
            .into_iter()
            .map(|amount| amount.0)
            .collect();

        pool.try_remove_liquidity_imbalance(&remove_coin_amount)
    }

    pub fn try_remove_liquidity(&self, pool_id: u64, shares: U128) -> Vec<U128> {
        let pool = self.pools.get(pool_id).expect("ERR_NO_POOL");
        let amounts = pool.try_remove_liquidity(shares.0);

        amounts.into_iter().map(|amount| amount.into()).collect()
    }

    pub fn try_add_liquidity(&self, pool_id: u64, deposit_amounts: Vec<U128>) -> U128 {
        let pool = self.pools.get(pool_id).expect("ERR_NO_POOL");

        let deposit_amounts: Vec<u128> =
            deposit_amounts.into_iter().map(|amount| amount.0).collect();

        pool.try_add_liquidity(&deposit_amounts).into()
    }
}
