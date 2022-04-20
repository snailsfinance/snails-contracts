use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};

use crate::bigint::U192;

/// Fees struct
#[derive(
    Clone, Copy, BorshSerialize, BorshDeserialize, Serialize, Deserialize, PartialEq, Debug,
)]
#[serde(crate = "near_sdk::serde")]
pub struct Fees {
    /// Admin trade fee numerator
    pub admin_trade_fee_numerator: u64,
    /// Admin trade fee denominator
    pub admin_trade_fee_denominator: u64,
    /// Admin withdraw fee numerator
    pub admin_withdraw_fee_numerator: u64,
    /// Admin withdraw fee denominator
    pub admin_withdraw_fee_denominator: u64,
    /// Trade fee numerator
    pub trade_fee_numerator: u64,
    /// Trade fee denominator
    pub trade_fee_denominator: u64,
    /// Withdraw fee numerator
    pub withdraw_fee_numerator: u64,
    /// Withdraw fee denominator
    pub withdraw_fee_denominator: u64,
}

impl Fees {
    /// Apply admin trade fee
    pub fn admin_trade_fee(&self, fee_amount: u128) -> Option<u128> {
        U192::from(fee_amount)
            .checked_mul(self.admin_trade_fee_numerator.into())?
            .checked_div(self.admin_trade_fee_denominator.into())?
            .to_u128()
    }

    /// Apply admin withdraw fee
    pub fn admin_withdraw_fee(&self, fee_amount: u128) -> Option<u128> {
        U192::from(fee_amount)
            .checked_mul(self.admin_withdraw_fee_numerator.into())?
            .checked_div(self.admin_withdraw_fee_denominator.into())?
            .to_u128()
    }

    /// Compute trade fee from amount
    pub fn trade_fee(&self, trade_amount: u128) -> Option<u128> {
        U192::from(trade_amount)
            .checked_mul(self.trade_fee_numerator.into())?
            .checked_div(self.trade_fee_denominator.into())?
            .to_u128()
    }

    /// Compute withdraw fee from amount
    pub fn withdraw_fee(&self, withdraw_amount: u128) -> Option<u128> {
        U192::from(withdraw_amount)
            .checked_mul(self.withdraw_fee_numerator.into())?
            .checked_div(self.withdraw_fee_denominator.into())?
            .to_u128()
    }

    /// Compute normalized fee for symmetric/asymmetric deposits/withdraws
    pub fn normalized_trade_fee(&self, n_coins: u64, amount: u128) -> Option<u128> {
        // adjusted_fee_numerator: uint256 = self.fee * N_COINS / (4 * (N_COINS - 1))
        let adjusted_trade_fee_numerator = self
            .trade_fee_numerator
            .checked_mul(n_coins)?
            .checked_div((n_coins.checked_sub(1)?).checked_mul(4)?)?;

        U192::from(amount)
            .checked_mul(adjusted_trade_fee_numerator.into())?
            .checked_div(self.trade_fee_denominator.into())?
            .to_u128()
    }
}
