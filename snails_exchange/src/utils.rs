use std::collections::HashSet;

use crate::fees::Fees;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{ext_contract, AccountId, Balance, Gas};
/// Attach no deposit.
pub const NO_DEPOSIT: u128 = 0;
/// hotfix_insuffient_gas_for_mft_resolve_transfer, increase from 5T to 20T
pub const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas(20_000_000_000_000);

pub const GAS_FOR_FT_TRANSFER_CALL: Gas = Gas(25_000_000_000_000 + GAS_FOR_RESOLVE_TRANSFER.0);

/// Amount of gas for fungible token transfers.
pub const GAS_FOR_FT_TRANSFER: Gas = Gas(10_000_000_000_000);

/// 1e24
pub const PRECISION: u128 = 1_000_000_000_000_000_000_000_000;

/// Volume of swap on the given token.
#[derive(Clone, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct SwapVolume {
    pub input: U128,
    pub output: U128,
}

impl Default for SwapVolume {
    fn default() -> Self {
        Self {
            input: U128(0),
            output: U128(0),
        }
    }
}

/// Checks if there are any duplicates in the given list of tokens.
pub fn check_token_duplicates(tokens: &[AccountId]) {
    let token_set: HashSet<_> = tokens.iter().map(|a| a.as_ref()).collect();
    assert_eq!(token_set.len(), tokens.len(), "ERR_TOKEN_DUPLICATES");
}

pub fn assert_fees_info_valid(fees: &Fees) {
    assert!(
        fees.admin_trade_fee_denominator != 0 as u64,
        "ERR_admin_trade_fee_denominator"
    );
    assert!(
        fees.admin_withdraw_fee_denominator != 0 as u64,
        "ERR_admin_withdraw_fee_denominator"
    );
    assert!(
        fees.trade_fee_denominator != 0 as u64,
        "ERR_trade_fee_denominator"
    );
    assert!(
        fees.withdraw_fee_denominator != 0 as u64,
        "ERR_withdraw_fee_denominator"
    );
}

/// Adds given value to item stored in the given key in the LookupMap collection.
pub fn add_to_collection(c: &mut LookupMap<AccountId, Balance>, key: &String, value: Balance) {
    let key = AccountId::try_from(key.clone()).unwrap();
    let prev_value = c.get(&key).unwrap_or(0);
    c.insert(&key, &(prev_value.checked_add(value).unwrap()));
}

#[ext_contract(ext_self)]
pub trait SnailExchange {
    fn exchange_callback_post_withdraw(
        &mut self,
        token_id: AccountId,
        sender_id: AccountId,
        amount: U128,
    );
}
