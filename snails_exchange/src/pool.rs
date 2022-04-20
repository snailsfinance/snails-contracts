use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{AccountId, Balance};

use crate::fees::Fees;
use crate::simple_pool::SimplePool;
/// Generic Pool, providing wrapper around different implementations of swap pools.
/// Allows to add new types of pools just by adding extra item in the enum without needing to migrate the storage.
#[derive(BorshSerialize, BorshDeserialize)]
pub enum Pool {
    SimplePool(SimplePool),
}

impl Pool {
    /// Adds liquidity into underlying pool.
    /// Updates amounts to amount kept in the pool.
    pub fn add_liquidity(
        &mut self,
        sender_id: &AccountId,
        amounts: &Vec<Balance>,
    ) -> (Balance, Vec<Balance>) {
        match self {
            Pool::SimplePool(pool) => pool.add_liquidity(sender_id, amounts),
        }
    }

    /// Removes liquidity from underlying pool.
    pub fn remove_liquidity(
        &mut self,
        sender_id: &AccountId,
        shares: Balance,
        min_amounts: Vec<Balance>,
    ) -> (Vec<Balance>, Vec<Balance>) {
        match self {
            Pool::SimplePool(pool) => pool.remove_liquidity(sender_id, shares, min_amounts),
        }
    }

    pub fn remove_liquidity_imbalance(
        &mut self,
        sender_id: &AccountId,
        remove_coin_amount: &Vec<Balance>,
    ) -> (u128, Vec<Balance>) {
        match self {
            Pool::SimplePool(pool) => {
                pool.remove_liquidity_imbalance(sender_id, remove_coin_amount)
            }
        }
    }

    pub fn remove_liquidity_one_coin(
        &mut self,
        sender_id: &AccountId,
        token_out: AccountId,
        remove_lp_amount: Balance,
        min_amount: Balance,
    ) -> (Vec<Balance>, Vec<Balance>) {
        match self {
            Pool::SimplePool(pool) => {
                pool.remove_liquidity_one_coin(sender_id, &token_out, remove_lp_amount, min_amount)
            }
        }
    }

    pub fn get_virtual_price(&self) -> u128 {
        match self {
            Pool::SimplePool(pool) => pool.get_virtual_price(),
        }
    }

    pub fn get_amp_factor(&self) -> u128 {
        match self {
            Pool::SimplePool(pool) => pool.get_amp_factor(),
        }
    }

    pub fn change_fees_setting(&mut self, fees: Fees) {
        match self {
            Pool::SimplePool(pool) => pool.change_fees_setting(fees),
        }
    }

    pub fn set_amp_params(
        &mut self,
        initial_amp_factor: u64,
        target_amp_factor: u64,
        start_ramp_ts: u64,
        stop_ramp_ts: u64,
    ) {
        match self {
            Pool::SimplePool(pool) => pool.set_amp_params(
                initial_amp_factor,
                target_amp_factor,
                start_ramp_ts,
                stop_ramp_ts,
            ),
        }
    }

    /// Returns which tokens are in the underlying pool.
    pub fn tokens(&self) -> &[AccountId] {
        match self {
            Pool::SimplePool(pool) => pool.tokens(),
        }
    }

    /// Returns given pool's total fee.
    pub fn get_fee(&self) -> Vec<u128> {
        match self {
            Pool::SimplePool(pool) => pool.get_fee(),
        }
    }

    pub fn get_admin_fee(&self) -> Vec<u128> {
        match self {
            Pool::SimplePool(pool) => pool.get_admin_fee(),
        }
    }

    pub fn fees_info(&self) -> Fees {
        match self {
            Pool::SimplePool(pool) => pool.fees_info(),
        }
    }

    /// Returns how many tokens will one receive swapping given amount of token_in for token_out.
    pub fn get_return(
        &self,
        token_in: &AccountId,
        amount_in: Balance,
        token_out: &AccountId,
    ) -> Balance {
        match self {
            Pool::SimplePool(pool) => pool.get_return(token_in, amount_in, token_out),
        }
    }

    pub fn share_total_balance(&self) -> Balance {
        match self {
            Pool::SimplePool(pool) => pool.share_total_balance(),
        }
    }

    pub fn share_balances(&self, account_id: &AccountId) -> Balance {
        match self {
            Pool::SimplePool(pool) => pool.share_balance_of(account_id),
        }
    }

    /// Swaps given number of token_in for token_out and returns received amount.
    pub fn swap(
        &mut self,
        token_in: &AccountId,
        amount_in: Balance,
        token_out: &AccountId,
        min_amount_out: Balance,
    ) -> (Balance, Balance) {
        match self {
            Pool::SimplePool(pool) => pool.swap(token_in, amount_in, token_out, min_amount_out),
        }
    }
    pub fn share_transfer(&mut self, sender_id: &AccountId, receiver_id: &AccountId, amount: u128) {
        match self {
            Pool::SimplePool(pool) => pool.share_transfer(sender_id, receiver_id, amount),
        }
    }

    pub fn share_register(&mut self, account_id: &AccountId) {
        match self {
            Pool::SimplePool(pool) => pool.share_register(account_id),
        }
    }

    pub fn is_lp_token_registered(&self, account_id: &AccountId) -> bool {
        match self {
            Pool::SimplePool(pool) => pool.is_lp_token_registered(account_id),
        }
    }

    pub fn try_remove_liquidity_one_coin(
        &self,
        token_out: &AccountId,
        remove_lp_amount: Balance,
    ) -> Balance {
        match self {
            Pool::SimplePool(pool) => {
                pool.try_remove_liquidity_one_coin(token_out, remove_lp_amount)
            }
        }
    }

    pub fn try_remove_liquidity_imbalance(&self, remove_coin_amount: &Vec<Balance>) -> u128 {
        match self {
            Pool::SimplePool(pool) => pool.try_remove_liquidity_imbalance(remove_coin_amount),
        }
    }

    pub fn try_remove_liquidity(&self, shares: Balance) -> Vec<Balance> {
        match self {
            Pool::SimplePool(pool) => pool.try_remove_liquidity(shares),
        }
    }

    pub fn try_add_liquidity(&self, deposit_amounts: &Vec<Balance>) -> Balance {
        match self {
            Pool::SimplePool(pool) => pool.try_add_liquidity(deposit_amounts),
        }
    }
}
