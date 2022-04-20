use crate::StorageKey;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::{env, AccountId, Balance};

use crate::error::{LP_ALREADY_REGISTERED, LP_NOT_REGISTERED, ZERO_SHARES};

use crate::utils::{add_to_collection, SwapVolume};

use crate::fees::Fees;
use crate::snails::{PoolStatus, SnailStableSwap};

/// Implementation of simple pool, that maintains constant product between balances of all the tokens.
/// Similar in design to "Uniswap".
/// Liquidity providers when depositing receive shares, that can be later burnt to withdraw pool's tokens in proportion.
#[derive(BorshSerialize, BorshDeserialize)]
pub struct SimplePool {
    /// List of tokens in the pool.
    pub token_account_ids: Vec<AccountId>,
    pub token_decimals: Vec<u64>,
    /// How much NEAR this contract has.
    pub amounts: Vec<Balance>,
    /// Volumes accumulated by this pool.
    pub volumes: Vec<SwapVolume>,
    pub total_fees: Vec<Balance>,
    pub admin_fees: Vec<Balance>,
    /// Shares of the pool by liquidity providers.
    pub shares: LookupMap<AccountId, Balance>,
    /// Total number of shares.
    pub shares_total_supply: Balance,

    /// Initial amplification coefficient (A)
    pub initial_amp_factor: u64,
    /// Target amplification coefficient (A)
    pub target_amp_factor: u64,
    /// Ramp A start timestamp
    pub start_ramp_ts: u64,
    /// Ramp A stop timestamp
    pub stop_ramp_ts: u64,

    pub fees: Fees,

    pub apply_new_fee_ts: u64,

    pub new_fees: Fees,
}

pub fn decimals_to_rates(vector: &Vec<u64>) -> Vec<u128> {
    let mut arr = vec![0u128; vector.len()];
    let base: u128 = 10; // an explicit type is required
    for (place, element) in arr.iter_mut().zip(vector.iter()) {
        assert!(24 >= *element, "invalid rates number");
        *place = base.pow(24 as u32 - *element as u32) as u128;
    }
    arr
}

impl SimplePool {
    pub fn new(
        id: u32,
        initial_amp_factor: u64,
        target_amp_factor: u64,
        start_ramp_ts: u64,
        stop_ramp_ts: u64,
        fees: Fees,
        token_account_ids: Vec<AccountId>,
        decimals: Vec<u64>,
    ) -> Self {
        assert_eq!(token_account_ids.len(), decimals.len());
        Self {
            token_account_ids: token_account_ids.iter().map(|a| a.clone().into()).collect(),
            token_decimals: decimals,
            amounts: vec![0u128; token_account_ids.len()],
            volumes: vec![SwapVolume::default(); token_account_ids.len()],
            total_fees: vec![0u128; token_account_ids.len()],
            admin_fees: vec![0u128; token_account_ids.len()],
            shares: LookupMap::new(StorageKey::Shares { pool_id: id }),
            shares_total_supply: 0,
            initial_amp_factor: initial_amp_factor,
            target_amp_factor: target_amp_factor,
            start_ramp_ts: start_ramp_ts,
            stop_ramp_ts: stop_ramp_ts,
            fees: fees,
            apply_new_fee_ts: 0,
            new_fees: fees,
        }
    }

    pub fn set_amp_params(
        &mut self,
        initial_amp_factor: u64,
        target_amp_factor: u64,
        start_ramp_ts: u64,
        stop_ramp_ts: u64,
    ) {
        self.initial_amp_factor = initial_amp_factor;
        self.target_amp_factor = target_amp_factor;
        self.start_ramp_ts = start_ramp_ts;
        self.stop_ramp_ts = stop_ramp_ts;
    }

    pub fn coin_num(&self) -> usize {
        self.token_account_ids.len()
    }

    /// Returns given pool's total fee.
    pub fn get_fee(&self) -> Vec<u128> {
        self.total_fees.iter().map(|fee| (fee.clone())).collect()
    }

    pub fn get_admin_fee(&self) -> Vec<u128> {
        self.admin_fees.iter().map(|fee| (fee.clone())).collect()
    }
    /// Returns balance of shares for given user.
    pub fn share_balance_of(&self, account_id: &AccountId) -> Balance {
        self.shares.get(account_id).unwrap_or_default()
    }

    pub fn fees_info(&self) -> Fees {
        self.fees
    }

    /// Returns total number of shares in this pool.
    pub fn share_total_balance(&self) -> Balance {
        self.shares_total_supply
    }

    /// Returns how much token you will receive if swap `token_amount_in` of `token_in` for `token_out`.
    pub fn get_return(
        &self,
        token_in: &AccountId,
        amount_in: Balance,
        token_out: &AccountId,
    ) -> Balance {
        self.internal_get_return(
            self.token_index(token_in),
            amount_in,
            self.token_index(token_out),
        )
    }

    fn assert_param_num(&self, param_num: usize) {
        assert_eq!(
            self.coin_num(),
            param_num,
            "param_num should equal to coin num"
        );
    }

    fn add_liquidity_impl(&self, deposit_amounts: &Vec<Balance>) -> PoolStatus {
        self.assert_param_num(deposit_amounts.len());

        let unix_timestamp_s = (near_sdk::env::block_timestamp() as u64) / (1e9 as u64);

        let rates = decimals_to_rates(&self.token_decimals);

        let invariant = SnailStableSwap::new(
            self.initial_amp_factor,
            self.target_amp_factor,
            unix_timestamp_s,
            self.start_ramp_ts,
            self.stop_ramp_ts,
            rates,
        );

        invariant
            .add_liquidity(
                deposit_amounts,
                &self.amounts,
                self.shares_total_supply,
                &self.fees,
            )
            .expect("ERR_ADD_LIQUIDITY_FAILED")
    }
    pub fn try_add_liquidity(&self, deposit_amounts: &Vec<Balance>) -> Balance {
        let poolstatus = self.add_liquidity_impl(deposit_amounts);

        let mint_shares = poolstatus.pool_lp_token_changed;
        assert!(poolstatus.pool_lp_changed_direction == true);

        mint_shares.into()
    }

    /// Adds the amounts of tokens to liquidity pool and returns number of shares that this user receives.
    /// Updates amount to amount kept in the pool.
    pub fn add_liquidity(
        &mut self,
        sender_id: &AccountId,
        deposit_amounts: &Vec<Balance>,
    ) -> (Balance, Vec<Balance>) {
        let poolstatus = self.add_liquidity_impl(deposit_amounts);

        let mint_shares = poolstatus.pool_lp_token_changed;
        assert!(poolstatus.pool_lp_changed_direction == true);

        //update amounts and fees
        for i in 0..self.token_account_ids.len() {
            self.amounts[i] = poolstatus.new_balances[i].into();
            self.total_fees[i] = self.total_fees[i]
                .checked_add(poolstatus.total_fee_amount[i])
                .unwrap();

            self.admin_fees[i] = self.admin_fees[i]
                .checked_add(poolstatus.admin_fee_amount[i])
                .unwrap();
        }

        self.mint_shares(&sender_id, mint_shares.into());
        assert!(mint_shares > 0, "{}", ZERO_SHARES);
        env::log_str(
            format!(
                "Liquidity added {:?}, minted {} shares, shares_total_supply {}",
                deposit_amounts
                    .iter()
                    .zip(self.token_account_ids.iter())
                    .map(|(amount, token_id)| format!("{} {}", amount, token_id))
                    .collect::<Vec<String>>(),
                mint_shares,
                self.shares_total_supply
            )
            .as_str(),
        );
        (mint_shares.into(), poolstatus.admin_fee_amount)
    }

    fn remove_liquidity_impl(&self, shares: Balance) -> PoolStatus {
        let unix_timestamp_s = (near_sdk::env::block_timestamp() as u64) / (1e9 as u64);
        let rates = decimals_to_rates(&self.token_decimals);

        let invariant = SnailStableSwap::new(
            self.initial_amp_factor,
            self.target_amp_factor,
            unix_timestamp_s,
            self.start_ramp_ts,
            self.stop_ramp_ts,
            rates,
        );

        invariant
            .remove_liquidity(shares, &self.amounts, self.shares_total_supply, &self.fees)
            .expect("ERR_REMOVE_LIQUIDITY_FAILED")
    }

    pub fn try_remove_liquidity(&self, shares: Balance) -> Vec<Balance> {
        let poolstatus = self.remove_liquidity_impl(shares);

        poolstatus
            .recieved_amount
            .iter()
            .map(|amount| *amount)
            .collect()
    }
    pub fn remove_liquidity(
        &mut self,
        sender_id: &AccountId,
        shares: Balance,
        min_amounts: Vec<Balance>,
    ) -> (Vec<Balance>, Vec<Balance>) {
        self.assert_param_num(min_amounts.len());
        let poolstatus = self.remove_liquidity_impl(shares);
        let prev_shares_amount = self.shares.get(&sender_id).expect("ERR_NO_SHARES");
        let amounts = self.process_amount_and_fees(sender_id, prev_shares_amount, &poolstatus);

        for i in 0..self.token_account_ids.len() {
            assert!(amounts[i] >= min_amounts[i], "ERR_LESS_THAN_MIN_AMOUNT");
        }

        (amounts, poolstatus.admin_fee_amount)
    }

    pub fn process_amount_and_fees(
        &mut self,
        sender_id: &AccountId,
        prev_shares_amount: Balance,
        poolstatus: &PoolStatus,
    ) -> Vec<Balance> {
        let burn_shares: Balance = poolstatus.pool_lp_token_changed.into();
        assert!(poolstatus.pool_lp_changed_direction == false);

        self.shares_total_supply = self.shares_total_supply.checked_sub(burn_shares).unwrap();

        let mut result = vec![];
        //update amounts
        for i in 0..self.token_account_ids.len() {
            self.amounts[i] = poolstatus.new_balances[i].into();

            self.total_fees[i] = self.total_fees[i]
                .checked_add(poolstatus.total_fee_amount[i])
                .unwrap();

            self.admin_fees[i] = self.admin_fees[i]
                .checked_add(poolstatus.admin_fee_amount[i])
                .unwrap();

            result.push(poolstatus.recieved_amount[i] as u128);
        }

        if prev_shares_amount == burn_shares {
            // never unregister a LP when he remove liqudity.
            self.shares.insert(&sender_id, &0);
        } else {
            self.shares.insert(
                &sender_id,
                &(prev_shares_amount.checked_sub(burn_shares).unwrap()),
            );
        }

        env::log_str(
            format!(
                "{} shares of liquidity removed: receive back {:?}",
                burn_shares,
                result
                    .iter()
                    .zip(self.token_account_ids.iter())
                    .map(|(amount, token_id)| format!("{} {}", amount, token_id))
                    .collect::<Vec<String>>(),
            )
            .as_str(),
        );

        result
    }
    fn remove_liquidity_imbalance_impl(&self, remove_coin_amount: &Vec<Balance>) -> PoolStatus {
        self.assert_param_num(remove_coin_amount.len());
        for i in 0..self.token_account_ids.len() {
            //should not drain out any coin
            assert!(
                self.amounts[i] > remove_coin_amount[i],
                "INVALID_INPUT_AMOUNT"
            );
        }

        let unix_timestamp_s = (near_sdk::env::block_timestamp() as u64) / (1e9 as u64);
        let rates = decimals_to_rates(&self.token_decimals);

        let invariant = SnailStableSwap::new(
            self.initial_amp_factor,
            self.target_amp_factor,
            unix_timestamp_s,
            self.start_ramp_ts,
            self.stop_ramp_ts,
            rates,
        );

        invariant
            .remove_liquidity_imbalance(
                remove_coin_amount,
                &self.amounts,
                self.shares_total_supply,
                &self.fees,
            )
            .expect("REMOVE_LIQUIDITY_IMBALANCE_FAILED")
    }

    pub fn try_remove_liquidity_imbalance(&self, remove_coin_amount: &Vec<Balance>) -> u128 {
        let poolstatus = self.remove_liquidity_imbalance_impl(remove_coin_amount);
        poolstatus.pool_lp_token_changed.into()
    }

    pub fn remove_liquidity_imbalance(
        &mut self,
        sender_id: &AccountId,
        remove_coin_amount: &Vec<Balance>,
    ) -> (u128, Vec<Balance>) {
        let poolstatus = self.remove_liquidity_imbalance_impl(remove_coin_amount);

        let prev_shares_amount = self.shares.get(&sender_id).expect("ERR_NO_SHARES");
        let amounts = self.process_amount_and_fees(sender_id, prev_shares_amount, &poolstatus);

        for i in 0..self.token_account_ids.len() {
            assert!(amounts[i] == remove_coin_amount[i]);
        }

        (
            poolstatus.pool_lp_token_changed.into(),
            poolstatus.admin_fee_amount,
        )
    }

    fn remove_liquidity_one_coin_impl(
        &self,
        token_index: u8,
        remove_lp_amount: Balance,
    ) -> PoolStatus {
        let unix_timestamp_s = (near_sdk::env::block_timestamp() as u64) / (1e9 as u64);
        let rates = decimals_to_rates(&self.token_decimals);

        let invariant = SnailStableSwap::new(
            self.initial_amp_factor,
            self.target_amp_factor,
            unix_timestamp_s,
            self.start_ramp_ts,
            self.stop_ramp_ts,
            rates,
        );

        invariant
            .remove_liquidity_one_coin(
                token_index,
                remove_lp_amount,
                &self.amounts,
                self.shares_total_supply,
                &self.fees,
            )
            .expect("ERR_CANT_REMOVE_LIQUIDITY_ONE_COIN")
    }

    pub fn try_remove_liquidity_one_coin(
        &self,
        token_out: &AccountId,
        remove_lp_amount: Balance,
    ) -> Balance {
        let token_index = self.token_index(token_out) as u8;
        let poolstatus = self.remove_liquidity_one_coin_impl(token_index, remove_lp_amount);
        poolstatus.recieved_amount[token_index as usize]
    }

    pub fn remove_liquidity_one_coin(
        &mut self,
        sender_id: &AccountId,
        token_out: &AccountId,
        remove_lp_amount: Balance,
        min_amount: Balance,
    ) -> (Vec<Balance>, Vec<Balance>) {
        let token_index = self.token_index(token_out) as u8;
        let poolstatus = self.remove_liquidity_one_coin_impl(token_index, remove_lp_amount);
        let prev_shares_amount = self.shares.get(&sender_id).expect("ERR_NO_SHARES");
        let amounts = self.process_amount_and_fees(sender_id, prev_shares_amount, &poolstatus);
        assert!(
            amounts[token_index as usize] >= min_amount,
            "ERR_EXCEED_MIN_AMOUNT"
        );

        (amounts, poolstatus.admin_fee_amount)
    }

    /// Swap `token_amount_in` of `token_in` token into `token_out` and return how much was received.
    /// Assuming that `token_amount_in` was already received from `sender_id`.
    pub fn swap(
        &mut self,
        token_in: &AccountId,
        amount_in: Balance,
        token_out: &AccountId,
        min_amount_out: Balance,
    ) -> (Balance, Balance) {
        let unix_timestamp_s = (near_sdk::env::block_timestamp() as u64) / (1e9 as u64);
        let rates = decimals_to_rates(&self.token_decimals);

        let invariant = SnailStableSwap::new(
            self.initial_amp_factor,
            self.target_amp_factor,
            unix_timestamp_s,
            self.start_ramp_ts,
            self.stop_ramp_ts,
            rates,
        );

        let in_idx = self.token_index(token_in);
        let out_idx = self.token_index(token_out);

        let result = invariant
            .exchange(
                in_idx as u8,
                out_idx as u8,
                amount_in,
                &self.amounts,
                &self.fees,
            )
            .expect("ERR_SWAP_FAILED");

        let amount_out: Balance = (result.amount_b as u128).into();
        assert!(amount_out >= min_amount_out, "ERR_MIN_AMOUNT");

        self.amounts[in_idx] = self.amounts[in_idx].checked_add(amount_in).unwrap();

        self.amounts[out_idx] = self.amounts[out_idx].checked_sub(amount_out).unwrap();

        let total_fee_amount: Balance = (result.total_fee as u128).into();
        let admin_fee_amount: Balance = (result.admin_fee as u128).into();

        self.total_fees[out_idx] = self.total_fees[out_idx]
            .checked_add(total_fee_amount)
            .unwrap();

        self.admin_fees[out_idx] = self.admin_fees[out_idx]
            .checked_add(admin_fee_amount)
            .unwrap();

        // Keeping track of volume per each input traded separately.
        // Reported volume with fees will be sum of `input`, without fees will be sum of `output`.

        self.volumes[in_idx].input.0 = self.volumes[in_idx].input.0.checked_add(amount_in).unwrap();

        self.volumes[in_idx].output.0 = self.volumes[in_idx]
            .output
            .0
            .checked_add(amount_out)
            .unwrap();

        env::log_str(
            format!(
                "Swapped {} {} for {} {} with admin fee {} total_fee {}",
                amount_in, token_in, amount_out, token_out, result.admin_fee, result.total_fee
            )
            .as_str(),
        );

        (amount_out, admin_fee_amount)
    }

    pub fn change_fees_setting(&mut self, fees: Fees) {
        self.fees = fees
    }

    /// Returns token index for given pool.
    fn token_index(&self, token_id: &AccountId) -> usize {
        self.token_account_ids
            .iter()
            .position(|id| id == token_id)
            .expect("ERR_MISSING_TOKEN")
    }

    /// Returns number of tokens in outcome, given amount.
    /// Tokens are provided as indexes into token list for given pool.
    fn internal_get_return(
        &self,
        token_in: usize,
        amount_in: Balance,
        token_out: usize,
    ) -> Balance {
        let unix_timestamp_s = (near_sdk::env::block_timestamp() as u64) / (1e9 as u64);
        let rates = decimals_to_rates(&self.token_decimals);

        let invariant = SnailStableSwap::new(
            self.initial_amp_factor,
            self.target_amp_factor,
            unix_timestamp_s,
            self.start_ramp_ts,
            self.stop_ramp_ts,
            rates,
        );

        let in_idx = token_in;
        let out_idx = token_out;

        let result = invariant
            .exchange(
                in_idx as u8,
                out_idx as u8,
                amount_in,
                &self.amounts,
                &self.fees,
            )
            .expect("ERR_GET_RETURN_FAILED");

        result.amount_b
    }

    /// Mint new shares for given user.
    fn mint_shares(&mut self, account_id: &AccountId, shares: Balance) {
        if shares == 0 {
            return;
        }
        self.shares_total_supply = self.shares_total_supply.checked_add(shares).unwrap();
        add_to_collection(&mut self.shares, &account_id.to_string(), shares);
    }

    pub fn tokens(&self) -> &[AccountId] {
        &self.token_account_ids
    }

    /// Transfers shares from predecessor to receiver.
    pub fn share_transfer(&mut self, sender_id: &AccountId, receiver_id: &AccountId, amount: u128) {
        let balance = self.shares.get(&sender_id).expect("ERR_NO_SHARES");
        if let Some(new_balance) = balance.checked_sub(amount) {
            self.shares.insert(&sender_id, &new_balance);
        } else {
            env::panic_str("ERR_NOT_ENOUGH_SHARES");
        }
        let balance_out = self.shares.get(&receiver_id).expect(LP_NOT_REGISTERED);
        self.shares
            .insert(&receiver_id, &(balance_out.checked_add(amount).unwrap()));
    }

    /// Register given account with 0 balance in shares.
    /// Storage payment should be checked by caller.
    pub fn share_register(&mut self, account_id: &AccountId) {
        if self.shares.contains_key(account_id) {
            env::panic_str(LP_ALREADY_REGISTERED);
        }
        self.shares.insert(account_id, &0);
    }

    pub fn is_lp_token_registered(&self, account_id: &AccountId) -> bool {
        self.shares.contains_key(account_id)
    }

    pub fn get_virtual_price(&self) -> u128 {
        let unix_timestamp_s = (near_sdk::env::block_timestamp() as u64) / (1e9 as u64);
        let rates = decimals_to_rates(&self.token_decimals);

        let invariant = SnailStableSwap::new(
            self.initial_amp_factor,
            self.target_amp_factor,
            unix_timestamp_s,
            self.start_ramp_ts,
            self.stop_ramp_ts,
            rates,
        );

        invariant
            .get_virtual_price(&self.amounts, self.shares_total_supply)
            .expect("ERR_INVALID_VIRUTAL_PRICE")
    }

    pub fn get_amp_factor(&self) -> u128 {
        let unix_timestamp_s = (near_sdk::env::block_timestamp() as u64) / (1e9 as u64);
        let rates = decimals_to_rates(&self.token_decimals);

        let invariant = SnailStableSwap::new(
            self.initial_amp_factor,
            self.target_amp_factor,
            unix_timestamp_s,
            self.start_ramp_ts,
            self.stop_ramp_ts,
            rates,
        );

        invariant.compute_amp_factor().expect("ERR_amp_factor") as u128
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decimals_to_rates() {
        const RATES: [u128; 3 as usize] = [1000000, 1000000000000000000, 1000000000000000000];
        let decimals: Vec<u64> = vec![18, 6, 6];
        let rates = decimals_to_rates(&decimals);
        for i in 0..rates.len() {
            assert_eq!(rates[i], RATES[i]);
        }
    }
}
