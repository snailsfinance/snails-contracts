//! Swap calculations and curve invariant implementation

use crate::bigint::{U192, U256, U576};
use crate::fees::Fees;
use crate::utils::PRECISION;

/// Encodes all results of swapping from a source token to a destination token
pub struct SwapResult {
    /// Assume user add token A to swap token B from pool
    /// index of asset A in pool
    pub i_a: i8,
    /// index of asset B in pool
    pub i_b: i8,
    /// added A amount
    pub amount_a: u128,
    /// substracted B amount from pool / swapped amount
    pub amount_b: u128,
    /// new amount token A in pool. Should be greater than before
    pub new_pool_a: u128,
    /// new amount token B in pool. Should be less than before
    pub new_pool_b: u128,
    /// Admin fee for the swap
    pub admin_fee: u128,
    /// Total fee for the swap. admin_fee is included in total_fee
    pub total_fee: u128,
}

/// Encodes all results of swapping from a source token to a destination token
pub struct PoolStatus {
    /// pool lp tokens changed. positive for increase / negative for decrease
    pub pool_lp_token_changed: u128,
    /// pool lp changed_direction: True for increase / False for decrease
    pub pool_lp_changed_direction: bool,
    /// coins recieved for user
    pub recieved_amount: Vec<u128>,
    /// balance after swapping
    pub new_balances: Vec<u128>,
    /// total fee of each coin
    pub total_fee_amount: Vec<u128>,
    /// admin fee of each coin
    pub admin_fee_amount: Vec<u128>,
}

/// The StableSwap invariant calculator.
pub struct SnailStableSwap {
    /// Initial amplification coefficient (A)
    initial_amp_factor: u64,
    /// Target amplification coefficient (A)
    target_amp_factor: u64,
    /// Current unix timestamp
    current_ts: u64,
    /// Ramp A start timestamp
    start_ramp_ts: u64,
    /// Ramp A stop timestamp
    stop_ramp_ts: u64,

    rates: Vec<u128>,
    coin_num: u64,
}

impl SnailStableSwap {
    /// New StableSwap calculator
    pub fn new(
        initial_amp_factor: u64,
        target_amp_factor: u64,
        current_ts: u64,
        start_ramp_ts: u64,
        stop_ramp_ts: u64,
        rates: Vec<u128>,
    ) -> Self {
        let coin_num = rates.len();
        assert!((coin_num <= 3 && coin_num >= 2), "2 <= coin_num <= 3");
        Self {
            initial_amp_factor,
            target_amp_factor,
            current_ts,
            start_ramp_ts,
            stop_ramp_ts,
            rates,
            coin_num: coin_num as u64,
        }
    }

    fn p_balances_convert(&self, balances: &Vec<u128>) -> Option<Vec<u128>> {
        let mut p_balances = balances.clone();
        for i in 0..balances.len() {
            p_balances[i] = balances[i].checked_mul(self.rates[i])?;
        }
        // None if overflow
        Some(p_balances)
    }

    fn compute_next_d(
        &self,
        amp_factor: u64,
        d_init: U576,
        d_prod: U576,
        sum_x: U192,
    ) -> Option<U576> {
        assert!(amp_factor != 0, "amp_factor == 0");
        let ann = (amp_factor as u128).checked_mul(self.coin_num.into())?;
        let leverage = U576::from(sum_x).checked_mul(ann.into())?;
        let numerator = d_init.checked_mul(
            d_prod
                .checked_mul(self.coin_num.into())?
                .checked_add(leverage.into())?,
        )?;
        assert!(ann > 1, "ann {} ", ann);
        let denominator = d_init
            .checked_mul(ann.checked_sub(1)?.into())?
            .checked_add(d_prod.checked_mul((self.coin_num.checked_add(1).unwrap()).into())?)?;

        numerator.checked_div(denominator)
    }

    /// Compute the amplification coefficient (A)
    pub fn compute_amp_factor(&self) -> Option<u64> {
        assert!(self.current_ts >= self.start_ramp_ts);
        if self.current_ts < self.stop_ramp_ts {
            let time_range = self.stop_ramp_ts.checked_sub(self.start_ramp_ts)?;
            let time_delta = self.current_ts.checked_sub(self.start_ramp_ts)?;

            // Compute amp factor based on ramp time
            if self.target_amp_factor >= self.initial_amp_factor {
                // Ramp up
                let amp_range = self
                    .target_amp_factor
                    .checked_sub(self.initial_amp_factor)?;
                let amp_delta = (amp_range as u128)
                    .checked_mul(time_delta as u128)?
                    .checked_div(time_range as u128)? as u64;

                self.initial_amp_factor.checked_add(amp_delta)
            } else {
                // Ramp down
                let amp_range = self
                    .initial_amp_factor
                    .checked_sub(self.target_amp_factor)?;
                let amp_delta = (amp_range as u128)
                    .checked_mul(time_delta as u128)?
                    .checked_div(time_range as u128)? as u64;
                self.initial_amp_factor.checked_sub(amp_delta)
            }
        } else {
            // when stop_ramp_ts == 0 or current_ts >= stop_ramp_ts
            Some(self.target_amp_factor)
        }
    }

    /// Compute stable swap invariant (D)
    fn get_d(&self, p_balances: &Vec<u128>) -> Option<U576> {
        let mut sum_x = U192::from(0);
        for &i in p_balances.iter() {
            sum_x = sum_x.checked_add(i.into())?;
        }
        if sum_x == 0.into() {
            Some(0.into())
        } else {
            let amp_factor = self.compute_amp_factor()?;

            let mut d_prev: U576;
            let mut d: U576 = sum_x.into();

            for _ in 0..256 {
                let mut d_prod = d;
                for &_x in p_balances.iter() {
                    let x_times_coins = U192::from(_x).checked_mul(self.coin_num.into())?;

                    d_prod = d_prod.checked_mul(d)?.checked_div(x_times_coins.into())?;
                }
                d_prev = d;

                d = self.compute_next_d(amp_factor, d, d_prod, sum_x)?;
                if d > d_prev {
                    if d.checked_sub(d_prev)? <= 1.into() {
                        break;
                    }
                } else if d_prev.checked_sub(d)? <= 1.into() {
                    break;
                }
            }
            Some(d)
        }
    }

    pub fn get_virtual_price(
        &self,
        balances: &Vec<u128>,
        total_token_supply: u128,
    ) -> Option<u128> {
        let p_balances = self.p_balances_convert(balances).unwrap();
        let d = self.get_d(&p_balances)?;
        Some(
            d.checked_mul(PRECISION.into())?
                .checked_div(total_token_supply.into())?
                .to_u128()?,
        )
    }

    /// Compute the amount of pool tokens to mint after a deposit
    pub fn add_liquidity(
        &self,
        deposit_amounts: &Vec<u128>,
        balances: &Vec<u128>,
        total_token_supply: u128,
        fees: &Fees,
    ) -> Option<PoolStatus> {
        let mut new_balances = balances.clone();
        let mut new_balances_d = balances.clone();
        let mut total_fee_amount = vec![0 as u128; self.coin_num as usize];
        let mut admin_fee_amount = vec![0 as u128; self.coin_num as usize];

        let mut d_0: U576 = 0.into();
        if total_token_supply > 0 {
            let p_balances = self.p_balances_convert(balances).unwrap();
            d_0 = self.get_d(&p_balances)?;
        }
        for i in 0..new_balances.len() {
            if total_token_supply == 0 {
                assert!(deposit_amounts[i] > 0); // initial deposit requires depositing all coins
            }
            new_balances[i] = new_balances[i].checked_add(deposit_amounts[i])?;
        }
        // Invariant after change
        let p_balances_new_balance = self.p_balances_convert(&new_balances).unwrap();
        let d_1 = self.get_d(&p_balances_new_balance)?;
        assert!(d_1 > d_0, "d_1 {} > d_0 {}", d_1, d_0);

        let mut d_2 = d_1;
        if total_token_supply > 0 {
            // Recalculate the invariant accounting for fees
            for i in 0..new_balances.len() {
                assert!(d_0 != 0.into(), "d_0 == 0");
                let ideal_balance: U192 = d_1
                    .checked_mul(balances[i].into())?
                    .checked_div(d_0)?
                    .to_u192()?;

                let difference = if ideal_balance > new_balances[i].into() {
                    ideal_balance.checked_sub(new_balances[i].into())?
                } else {
                    U192::from(new_balances[i]).checked_sub(ideal_balance)?
                };

                let diff_u128 = difference.to_u128()?;
                total_fee_amount[i] = fees.normalized_trade_fee(self.coin_num.into(), diff_u128)?;
                admin_fee_amount[i] = fees.admin_trade_fee(total_fee_amount[i])?;

                new_balances[i] = new_balances[i].checked_sub(admin_fee_amount[i])?;
                new_balances_d[i] = new_balances[i]
                    .checked_add(admin_fee_amount[i])?
                    .checked_sub(total_fee_amount[i])?;
            }
            let p_balances = self.p_balances_convert(&new_balances_d).unwrap();
            d_2 = self.get_d(&p_balances)?;
        }
        // else. new_balances = old_balances
        // calculate how many tokens to be mint
        let mint_lp_amount = if total_token_supply == 0 {
            d_1.to_u128()?
        } else {
            U576::from(total_token_supply)
                .checked_mul(d_2.checked_sub(d_0)?)?
                .checked_div(d_0)?
                .to_u128()?
        };
        Some(PoolStatus {
            pool_lp_token_changed: mint_lp_amount,     // calculated
            pool_lp_changed_direction: true,           // false = pool lp increase
            recieved_amount: deposit_amounts.to_vec(), // input parameter
            new_balances: new_balances.to_vec(),
            total_fee_amount: total_fee_amount.to_vec(),
            admin_fee_amount: admin_fee_amount.to_vec(),
        })
    }

    fn get_y_raw(&self, i: u8, j: u8, x: u128, balances: &Vec<u128>) -> Option<U576> {
        assert_ne!(i, j);
        assert!(i < (self.coin_num as u8));
        assert!(j < (self.coin_num as u8));

        // c =  D ** (n + 1) / (n ** (2 * n) * prod' * A)
        let amp_factor = self.compute_amp_factor()?;
        let ann = (amp_factor as u128).checked_mul(self.coin_num.into())?; // A * n ** n
        let d = self.get_d(balances)?;
        let mut c = d;
        let mut sum_: U192 = 0.into(); //avoid sum overflow
        let mut _x: u128 = 0;
        for _i in 0..balances.len() {
            if _i == (i as usize) {
                _x = x;
            } else if _i != (j as usize) {
                _x = balances[_i];
            } else {
                continue;
            }
            sum_ = sum_.checked_add(_x.into())?;

            c = c
                .checked_mul(d)?
                .checked_div(U192::from(_x).checked_mul(self.coin_num.into())?.into())?;
        }

        c = c
            .checked_mul(d)?
            .checked_div(ann.checked_mul(self.coin_num.into())?.into())?;
        // b = sum' - (A*n**n - 1) * D / (A * n**n)
        let b = d.checked_div(ann.into())?.checked_add(sum_.into())?;

        // y approximating: y**2 + b*y = c
        let mut y_prev: U576;
        let mut y = d;
        for _ in 0..256 {
            y_prev = y;
            let y_numerator = y.checked_pow(2.into())?.checked_add(c)?;
            let y_denominator = y.checked_mul(2.into())?.checked_add(b)?.checked_sub(d)?;

            y = y_numerator.checked_div(y_denominator)?;

            if y > y_prev {
                if y.checked_sub(y_prev)? <= 1.into() {
                    break;
                }
            } else if y_prev.checked_sub(y)? <= 1.into() {
                break;
            }
        }
        Some(y)
    }

    fn get_y(&self, i: u8, j: u8, x: u128, balances: &Vec<u128>) -> Option<u128> {
        self.get_y_raw(i, j, x, balances)?.to_u128()
    }

    pub fn exchange(
        &self,
        i: u8,
        j: u8,
        dx: u128,
        balances: &Vec<u128>,
        fees: &Fees,
    ) -> Option<SwapResult> {
        self.exchange_impl(i, j, dx, balances, fees)
    }

    fn exchange_impl(
        &self,
        i: u8,
        j: u8,
        dx: u128,
        balances: &Vec<u128>,
        fees: &Fees,
    ) -> Option<SwapResult> {
        let ii: usize = i as usize;
        let jj: usize = j as usize;
        let p_balances = self.p_balances_convert(balances)?;
        // overflow checked_add here, make sure x + dx u128
        let p_x = p_balances[ii].checked_add(dx.checked_mul(self.rates[ii])?)?;
        let p_y = self.get_y(i, j, p_x, &p_balances)?;

        // -1 to just in case there were some rounding errors
        let p_dy1 = p_balances[jj].checked_sub(p_y)?.checked_sub(1u128)?;
        let p_dy_fee = fees.trade_fee(p_dy1)?;
        let p_admin_fee = fees.admin_trade_fee(p_dy_fee)?;
        let dy_fee = p_dy_fee.checked_div(self.rates[jj])?;
        let admin_fee = p_admin_fee.checked_div(self.rates[jj])?;

        // final swapped y amount considering all fees now
        // remove precision
        let dy = (p_dy1.checked_sub(p_dy_fee)?).checked_div(self.rates[jj])?;

        let mut new_balances = balances.clone();
        new_balances[ii] = balances[ii].checked_add(dx)?;
        new_balances[jj] = balances[jj].checked_sub(dy)?.checked_sub(admin_fee)?;

        Some(SwapResult {
            i_a: i as i8,
            i_b: j as i8,
            amount_a: dx,
            amount_b: dy,
            new_pool_a: new_balances[ii],
            new_pool_b: new_balances[jj],
            admin_fee: admin_fee,
            total_fee: dy_fee,
        })
    }

    pub fn remove_liquidity(
        &self,
        removed_lp_amount: u128,
        balances: &Vec<u128>,
        total_token_supply: u128,
        fees: &Fees,
    ) -> Option<PoolStatus> {
        return self.remove_liquidity_impl(removed_lp_amount, balances, total_token_supply, fees);
    }

    /// removing LP amounts balanced
    fn remove_liquidity_impl(
        &self,
        removed_lp_amount: u128,
        balances: &Vec<u128>,
        total_token_supply: u128,
        fees: &Fees,
    ) -> Option<PoolStatus> {
        let mut recieved_amount = vec![0 as u128; self.coin_num as usize];
        let mut total_fee_amount = vec![0 as u128; self.coin_num as usize];
        let mut admin_fee_amount = vec![0 as u128; self.coin_num as usize];
        let mut new_balances = balances.clone();

        assert!(total_token_supply != 0);
        assert!(
            total_token_supply >= removed_lp_amount,
            "remove lp > total lp"
        );

        for i in 0..balances.len() {
            let value = U256::from(balances[i])
                .checked_mul(removed_lp_amount.into())?
                .checked_div(total_token_supply.into())?
                .to_u128()?;

            total_fee_amount[i] = fees.withdraw_fee(value)?;
            admin_fee_amount[i] = fees.admin_withdraw_fee(total_fee_amount[i])?;

            // remove patial / remove all
            // if remove all, all LP fees should be recieved to user
            if total_token_supply > removed_lp_amount {
                recieved_amount[i] = value.checked_sub(total_fee_amount[i])?;
            } else {
                // remove all here
                recieved_amount[i] = value.checked_sub(admin_fee_amount[i])?;
            }
            new_balances[i] = balances[i]
                .checked_sub(recieved_amount[i])?
                .checked_sub(admin_fee_amount[i])?;
        }

        Some(PoolStatus {
            pool_lp_token_changed: removed_lp_amount,  // input parameter
            pool_lp_changed_direction: false,          // false = lp decrease
            recieved_amount: recieved_amount.to_vec(), // calculated
            new_balances: new_balances.to_vec(),
            total_fee_amount: total_fee_amount.to_vec(),
            admin_fee_amount: admin_fee_amount.to_vec(),
        })
    }

    /// removing coin amounts customly

    pub fn remove_liquidity_imbalance(
        &self,
        remove_coin_amount: &Vec<u128>,
        balances: &Vec<u128>,
        total_token_supply: u128,
        fees: &Fees,
    ) -> Option<PoolStatus> {
        //assert!(remove_coin_amount[i] >= 0);

        let mut final_remove_coin_amount = remove_coin_amount.clone();
        let mut new_balances = balances.clone();
        let mut new_balances_d = balances.clone();
        // trade_fee + withdraw_fee
        let mut total_fee_amount = vec![0 as u128; self.coin_num as usize];
        let mut admin_fee_amount = vec![0 as u128; self.coin_num as usize];
        // withdraw fee
        let mut withdraw_fee_amount = vec![0 as u128; self.coin_num as usize];
        let mut admin_withdraw_fee_amount = vec![0 as u128; self.coin_num as usize];

        let p_balances = self.p_balances_convert(balances)?;
        let d_0 = self.get_d(&p_balances)?;
        for i in 0..new_balances.len() {
            new_balances[i] = new_balances[i].checked_sub(remove_coin_amount[i])?;
            new_balances_d[i] = new_balances_d[i].checked_sub(remove_coin_amount[i])?;
        }
        let p_balances = self.p_balances_convert(&new_balances)?;
        let d_1 = self.get_d(&p_balances)?;

        for i in 0..new_balances.len() {
            let ideal_balance = U576::from(balances[i])
                .checked_mul(d_1)?
                .checked_div(d_0)?
                .to_u128()?;
            let difference = if ideal_balance > new_balances[i] {
                ideal_balance.checked_sub(new_balances[i])?
            } else {
                new_balances[i].checked_sub(ideal_balance)?
            };
            // Fee1: trade_fee from difference
            total_fee_amount[i] = fees.normalized_trade_fee(self.coin_num.into(), difference)?;
            admin_fee_amount[i] = fees.admin_trade_fee(total_fee_amount[i])?;
            if total_fee_amount[i] > 0 {
                assert!(
                    admin_fee_amount[i] < total_fee_amount[i],
                    "admin_trade_fee error 1"
                );
            } else {
                assert!(admin_fee_amount[i] == 0u128, "admin_trade_fee error 2");
            }
            // remaining balances should more than total_trade_fee
            assert!(
                new_balances[i] > total_fee_amount[i],
                "remaining balance not enough for trade fee"
            );
            // Fee2: withdraw_fee from withdraw amounts, usually zero ..
            withdraw_fee_amount[i] = fees.withdraw_fee(remove_coin_amount[i])?;
            admin_withdraw_fee_amount[i] = fees.admin_withdraw_fee(withdraw_fee_amount[i])?;
            if withdraw_fee_amount[i] > 0 {
                assert!(
                    admin_withdraw_fee_amount[i] < withdraw_fee_amount[i],
                    "admin_withdraw_fee error 1"
                );
            } else {
                assert!(
                    admin_withdraw_fee_amount[i] == 0u128,
                    "admin_withdraw_fee error 2"
                );
            }

            // fees = trade_fee + withdraw_fee
            total_fee_amount[i] = total_fee_amount[i].checked_add(withdraw_fee_amount[i])?;
            admin_fee_amount[i] = admin_fee_amount[i].checked_add(admin_withdraw_fee_amount[i])?;

            assert!(
                new_balances[i] > total_fee_amount[i],
                "remaining balance not enough for withdraw fee"
            );

            new_balances[i] = new_balances[i].checked_sub(admin_fee_amount[i])?;

            //new_balance_d is used to compute_d, total fees are concluded.
            new_balances_d[i] = new_balances_d[i].checked_sub(total_fee_amount[i])?;
        }
        let p_new_balances_d = self.p_balances_convert(&new_balances_d)?;
        let d_2 = self.get_d(&p_new_balances_d)?;

        let mut burn_token_amount = (d_0.checked_sub(d_2)?)
            .checked_mul(U576::from(total_token_supply))?
            .checked_div(d_0)?
            .to_u128()?;

        burn_token_amount = burn_token_amount.checked_add(1)?; // +1 in case of rounding errors
        assert!(burn_token_amount > 0);
        // remove all. LP fees should be withdraw to final user
        if d_2 == 0.into() {
            for j in 0..new_balances.len() {
                let lp_fee_amount = total_fee_amount[j].checked_sub(admin_fee_amount[j])?;
                final_remove_coin_amount[j] =
                    final_remove_coin_amount[j].checked_add(lp_fee_amount)?; // LP fee
                new_balances[j] = new_balances[j].checked_sub(lp_fee_amount)?;
                assert_eq!(new_balances[j], 0u128);
                total_fee_amount[j] = admin_fee_amount[j];
            }
        }

        Some(PoolStatus {
            pool_lp_token_changed: burn_token_amount, // calculated
            pool_lp_changed_direction: false,         // false = lp decrease
            recieved_amount: final_remove_coin_amount.to_vec(), // input parameter
            new_balances: new_balances.to_vec(),
            total_fee_amount: total_fee_amount.to_vec(),
            admin_fee_amount: admin_fee_amount.to_vec(),
        })
    }

    fn get_y_d_raw(&self, i: u8, balances: &Vec<u128>, d: U576) -> Option<U576> {
        assert!(i < self.coin_num as u8);

        // c =  D ** (n + 1) / (n ** (2 * n) * prod' * A)
        let amp_factor = self.compute_amp_factor()?;
        let ann = (amp_factor as u128).checked_mul(self.coin_num.into())?; // A * n ** n
        let mut c = d;
        let mut sum_: U192 = 0.into();
        let mut _x: u128 = 0;
        for _i in 0..balances.len() {
            if _i != (i as usize) {
                _x = balances[_i];
            } else {
                continue;
            }
            sum_ = sum_.checked_add(_x.into())?;
            c = c
                .checked_mul(d)?
                .checked_div(U192::from(_x).checked_mul(self.coin_num.into())?.into())?;
        }
        c = c
            .checked_mul(d)?
            .checked_div(ann.checked_mul(self.coin_num.into())?.into())?;

        // b = sum' - (A*n**n - 1) * D / (A * n**n)
        let b = d.checked_div(ann.into())?.checked_add(sum_.into())?;

        // y approximating: y**2 + b*y = c
        let mut y_prev: U576;
        let mut y = d;
        for _ in 0..256 {
            y_prev = y;
            // y = (y * y + c) / (2 * y + b - d);
            let y_numerator = y.checked_pow(2.into())?.checked_add(c)?;
            let y_denominator = y.checked_mul(2.into())?.checked_add(b)?.checked_sub(d)?;
            y = y_numerator.checked_div(y_denominator)?;

            if y > y_prev {
                if y.checked_sub(y_prev)? <= 1.into() {
                    break;
                }
            } else if y_prev.checked_sub(y)? <= 1.into() {
                break;
            }
        }
        Some(y)
    }

    fn get_y_d(&self, i: u8, balances: &Vec<u128>, d: U576) -> Option<u128> {
        self.get_y_d_raw(i, balances, d)?.to_u128()
    }

    pub fn remove_liquidity_one_coin(
        &self,
        i: u8,
        remove_lp_amount: u128,
        balances: &Vec<u128>,
        total_token_supply: u128,
        fees: &Fees,
    ) -> Option<PoolStatus> {
        self.remove_liquidity_one_coin_impl(i, remove_lp_amount, balances, total_token_supply, fees)
    }

    // Calculate burned lp amount when withdrawing in form of only one token
    pub fn remove_liquidity_one_coin_impl(
        &self,
        i: u8,
        remove_lp_amount: u128,
        balances: &Vec<u128>,
        total_token_supply: u128,
        fees: &Fees,
    ) -> Option<PoolStatus> {
        assert!(
            remove_lp_amount <= total_token_supply,
            "remove lp > total lp"
        );
        let ii = i as usize; // for index i as type of usize
        let p_balances = self.p_balances_convert(balances)?;
        let d_0 = self.get_d(&p_balances)?;
        let d_1 = d_0.checked_sub(
            U576::from(remove_lp_amount)
                .checked_mul(d_0)?
                .checked_div(total_token_supply.into())?,
        )?;

        let p_new_y = self.get_y_d(i, &p_balances, d_1)?;
        let p_dy_0 = p_balances[ii].checked_sub(p_new_y)?; // expected p_dy without considering fees
                                                           //let dy_0 = p_dy_0.checked_div(self.rates[ii])?; // expected dy without considering fees
        let mut p_balances_reduce_fees = p_balances.clone();
        for j in 0..p_balances.len() {
            let p_dx_expected = if j == ii {
                U576::from(p_balances[j])
                    .checked_mul(d_1)?
                    .checked_div(d_0)?
                    .to_u128()?
                    .checked_sub(p_new_y)?
            } else {
                p_balances[j].checked_sub(
                    U576::from(p_balances[j])
                        .checked_mul(d_1)?
                        .checked_div(d_0)?
                        .to_u128()?,
                )?
            };
            p_balances_reduce_fees[j] = p_balances_reduce_fees[j]
                .checked_sub(fees.normalized_trade_fee(self.coin_num.into(), p_dx_expected)?)?;
        }
        let p_dy = p_balances_reduce_fees[ii]
            .checked_sub(self.get_y_d(i, &p_balances_reduce_fees, d_1)?)?
            .checked_sub(1)?; // Withdraw less 1 to account for rounding errors
                              //let dy = p_dy.checked_div(self.rates[ii])?;

        // preparing output
        let mut recieved_amount = vec![0 as u128; self.coin_num as usize];
        let mut total_fee_amount = vec![0 as u128; self.coin_num as usize];
        let mut admin_fee_amount = vec![0 as u128; self.coin_num as usize];
        let mut new_balances = balances.clone();
        //trade_fee calculate
        let mut p_total_fee_amount = p_dy_0.checked_sub(p_dy)?;
        let mut p_admin_fee_amount = fees.admin_trade_fee(p_total_fee_amount)?;
        assert!(p_total_fee_amount >= p_admin_fee_amount, "trade_fee error!");
        // withdraw fee calculate
        let p_withdraw_fee_amount = fees.withdraw_fee(p_dy)?;
        let p_admin_withdraw_fee_amount = fees.admin_withdraw_fee(p_withdraw_fee_amount)?;
        assert!(
            p_withdraw_fee_amount >= p_admin_withdraw_fee_amount,
            "withdraw_fee error!"
        );
        //total fees = trade_fee + withdraw_fee
        p_total_fee_amount = p_total_fee_amount.checked_add(p_withdraw_fee_amount)?;
        p_admin_fee_amount = p_admin_fee_amount.checked_add(p_admin_withdraw_fee_amount)?;
        // remove precision
        total_fee_amount[ii] = p_total_fee_amount.checked_div(self.rates[ii])?;
        admin_fee_amount[ii] = p_admin_fee_amount.checked_div(self.rates[ii])?;

        recieved_amount[ii] =
            (p_dy.checked_sub(p_withdraw_fee_amount)?).checked_div(self.rates[ii])?;
        // new_balance = balance - dy - admin_trade_fee + (withdraw_fee - admin_withdraw_fee)
        //              = balance - dy - admin_total_fee + withdraw_fee
        new_balances[ii] = (U192::from(p_balances[ii])
            .checked_add(p_withdraw_fee_amount.into())?
            .checked_sub(p_dy.into())?
            .checked_sub(p_admin_fee_amount.into())?
            .to_u128()?)
        .checked_div(self.rates[ii])?; //withdraw_fee. firstly add to avoid overflow

        Some(PoolStatus {
            pool_lp_token_changed: remove_lp_amount,
            pool_lp_changed_direction: false,
            recieved_amount: recieved_amount.to_vec(),
            new_balances: new_balances.to_vec(),
            total_fee_amount: total_fee_amount.to_vec(),
            admin_fee_amount: admin_fee_amount.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rand::Rng;
    use sim::Model;
    use std::cmp;

    /// Timestamp at 0
    pub const ZERO_TS: u64 = 0;
    /// Minimum ramp duration
    pub const MIN_RAMP_DURATION: u64 = 86400;
    /// Min amplification coefficient
    pub const MIN_AMP: u64 = 1;
    /// Max amplification coefficient
    pub const MAX_AMP: u64 = 1_000_000;
    /// MAX DAI with 10**decimal
    pub const MAX_DAI_INPUT: u128 = 340282366920938463463374607431768 >> 4;
    /// MAX USDT with 10**decimal
    pub const MAX_USDT_INPUT: u128 = 340282366920938463463 >> 4;
    /// MAX USDC with 10**decimal
    pub const MAX_USDC_INPUT: u128 = 340282366920938463463 >> 4;
    /// MAX NEAR with 10**decimal
    //pub const MAX_NEAR_INPUT: u128 = 340282366920938463463374607431768211455 >> 4;

    /// decimal to 1e24
    const TEST_RATES: [u128; 3 as usize] = [1000000, 1000000000000000000, 1000000000000000000];
    const TEST_TRADE_FEE: u128 = 4000000;
    const TEST_WITHDRAW_FEE: u128 = 3000000;
    const TEST_FEE_DENOMINATOR: u128 = 10000000000;
    const RAMP_TICKS: u64 = 100000;
    const TEST_N_COIN: u8 = 3;
    const TEST_MAX_TOTAL_SUPPLY: u128 = std::u128::MAX >> 4;
    const TEST_MAX_DX_WITHOUT_DECIMAL: u128 = 340282366920938 >> 4;

    //initial Fees without withdraw_fee
    const TEST_FEES_WITHOUT_WITHDRAW_FEE: Fees = Fees {
        admin_trade_fee_numerator: 5000000000,
        admin_trade_fee_denominator: 10000000000,
        admin_withdraw_fee_numerator: 5000000000,
        admin_withdraw_fee_denominator: 10000000000,
        trade_fee_numerator: 4000000,
        trade_fee_denominator: 10000000000,
        withdraw_fee_numerator: 0,
        withdraw_fee_denominator: 10000000000,
    };
    //initial Fees with withdraw_fee
    const TEST_FEES_WITH_WITHDRAW_FEE: Fees = Fees {
        admin_trade_fee_numerator: 5000000000,
        admin_trade_fee_denominator: 10000000000,
        admin_withdraw_fee_numerator: 5000000000,
        admin_withdraw_fee_denominator: 10000000000,
        trade_fee_numerator: 4000000,
        trade_fee_denominator: 10000000000,
        withdraw_fee_numerator: 3000000,
        withdraw_fee_denominator: 10000000000,
    };
    #[test]
    fn test_ramp_amp_up() {
        let mut rng = rand::thread_rng();
        let initial_amp_factor = 100;
        let target_amp_factor = initial_amp_factor * 2;
        let start_ramp_ts = rng.gen_range(ZERO_TS..=u64::MAX - RAMP_TICKS);
        let stop_ramp_ts = start_ramp_ts + MIN_RAMP_DURATION;
        println!(
            "start_ramp_ts: {}, stop_ramp_ts: {}",
            start_ramp_ts, stop_ramp_ts
        );

        for tick in 0..RAMP_TICKS {
            let current_ts = start_ramp_ts + tick;
            let snails_swap = SnailStableSwap::new(
                initial_amp_factor,
                target_amp_factor,
                current_ts,
                start_ramp_ts,
                stop_ramp_ts,
                TEST_RATES.to_vec(),
            );
            let expected = if tick >= MIN_RAMP_DURATION {
                target_amp_factor
            } else {
                initial_amp_factor + (initial_amp_factor * tick as u64 / MIN_RAMP_DURATION as u64)
            };
            assert_eq!(snails_swap.compute_amp_factor().unwrap(), expected);
        }
    }

    #[test]
    fn test_ramp_amp_down() {
        let mut rng = rand::thread_rng();
        let initial_amp_factor = 100;
        let target_amp_factor = initial_amp_factor / 10;
        let amp_range = initial_amp_factor - target_amp_factor;
        let start_ramp_ts = rng.gen_range(ZERO_TS..=u64::MAX - RAMP_TICKS);
        let stop_ramp_ts = start_ramp_ts + MIN_RAMP_DURATION;
        println!(
            "start_ramp_ts: {}, stop_ramp_ts: {}",
            start_ramp_ts, stop_ramp_ts
        );

        for tick in 0..RAMP_TICKS {
            let current_ts = start_ramp_ts + tick;
            let snails_swap = SnailStableSwap::new(
                initial_amp_factor,
                target_amp_factor,
                current_ts,
                start_ramp_ts,
                stop_ramp_ts,
                TEST_RATES.to_vec(),
            );
            let expected = if tick >= MIN_RAMP_DURATION {
                target_amp_factor
            } else {
                initial_amp_factor - (amp_range * tick as u64 / MIN_RAMP_DURATION as u64)
            };
            assert_eq!(snails_swap.compute_amp_factor().unwrap(), expected);
        }
    }

    proptest! {
        #[test]
        fn test_random_p_balances(
            initial_amp_factor in MIN_AMP..=MAX_AMP,
            target_amp_factor in MIN_AMP..=MAX_AMP,
            start_ramp_ts in ZERO_TS..=u64::MAX,
            stop_ramp_ts in ZERO_TS..=u64::MAX,
            current_ts in ZERO_TS..u64::MAX,
            b0 in u128::MIN..MAX_DAI_INPUT,
            b1 in u128::MIN..MAX_USDT_INPUT,
            b2 in u128::MIN..MAX_USDC_INPUT,
        ) {
            let snails_swap = SnailStableSwap::new(
                initial_amp_factor,
                target_amp_factor,
                current_ts,
                start_ramp_ts,
                stop_ramp_ts,
                TEST_RATES.to_vec(),
            );
            let balances = vec![b0, b1, b2];
            let p_balances = snails_swap.p_balances_convert(&balances).unwrap();
            for i in 0..p_balances.len() {
                assert_eq!(p_balances[i], balances[i] * TEST_RATES[i]);
            }
        }
    }
    fn check_d(
        model: &Model,
        balances: [u128; 3],
        current_ts: u64,
        start_ramp_ts: u64,
        stop_ramp_ts: u64,
    ) {
        let snails_swap = SnailStableSwap::new(
            model.amp_factor,
            model.amp_factor,
            current_ts,
            start_ramp_ts,
            stop_ramp_ts,
            TEST_RATES.to_vec(),
        );
        let p_balances = snails_swap.p_balances_convert(&balances.to_vec()).unwrap();
        let d = snails_swap.get_d(&p_balances).unwrap();
        assert_eq!(d.to_string(), model.sim_d().to_string());
    }

    proptest! {
        #[test]
        fn test_snails_math_get_d(
            current_ts in ZERO_TS..u64::MAX,
            amp_factor in MIN_AMP..MAX_AMP,
            b0 in 1..MAX_DAI_INPUT,             // Start at 1 to prevent divide by 0 when computing d
            b1 in 1..MAX_USDT_INPUT,
            b2 in 1..MAX_USDC_INPUT,
        ) {
            let start_ramp_ts = cmp::max(0, current_ts - MIN_RAMP_DURATION);
            let stop_ramp_ts = cmp::min(u64::MAX, current_ts + MIN_RAMP_DURATION);
            let balances = [b0, b1, b2];
            let model = Model::new(
                amp_factor.into(),
                balances.to_vec(),
                TEST_N_COIN,
                TEST_RATES.to_vec(),
                TEST_TRADE_FEE,
                TEST_WITHDRAW_FEE,
                0
            );
            check_d(&model, balances, current_ts, start_ramp_ts, stop_ramp_ts);
        }
    }

    #[test]
    fn test_snails_math_get_d_with_random_inputs() {
        for _ in 0..100 {
            let mut rng = rand::thread_rng();

            let amp_factor: u64 = rng.gen_range(MIN_AMP..=MAX_AMP);
            let b0: u128 = rng.gen_range(1..=MAX_DAI_INPUT);
            let b1: u128 = rng.gen_range(1..=MAX_USDT_INPUT);
            let b2: u128 = rng.gen_range(1..=MAX_USDC_INPUT);
            let start_ramp_ts: u64 = rng.gen_range(ZERO_TS as i64..=i64::MAX) as u64;
            let stop_ramp_ts: u64 = rng.gen_range(start_ramp_ts as i64..=i64::MAX) as u64;
            let current_ts: u64 = rng.gen_range(start_ramp_ts as i64..=stop_ramp_ts as i64) as u64;

            let balances = [b0, b1, b2];
            let model = Model::new(
                amp_factor.into(),
                balances.to_vec(),
                TEST_N_COIN,
                TEST_RATES.to_vec(),
                TEST_TRADE_FEE,
                TEST_WITHDRAW_FEE,
                0,
            );

            check_d(&model, balances, current_ts, start_ramp_ts, stop_ramp_ts);
        }
    }

    fn check_y(
        model: &Model,
        i: u8,
        j: u8,
        x: u128,
        balances: [u128; 3],
        current_ts: u64,
        start_ramp_ts: u64,
        stop_ramp_ts: u64,
    ) {
        let snails_swap = SnailStableSwap::new(
            model.amp_factor,
            model.amp_factor,
            current_ts,
            start_ramp_ts,
            stop_ramp_ts,
            TEST_RATES.to_vec(),
        );
        let p_balances = snails_swap.p_balances_convert(&balances.to_vec()).unwrap();
        let p_x = x.checked_mul(TEST_RATES[i as usize]).unwrap();
        let y = snails_swap.get_y_raw(i, j, p_x, &p_balances).unwrap();
        let y_python = model.sim_y(i.into(), j.into(), p_x.into());

        assert_eq!(y.to_string(), y_python.to_string());
    }

    proptest! {
        #[test]
        fn test_get_y_raw(
            current_ts in ZERO_TS..u64::MAX,
            amp_factor in MIN_AMP..MAX_AMP,
            b0 in 1..MAX_DAI_INPUT,             // Start at 1 to prevent divide by 0 when computing d
            b1 in 1..MAX_USDT_INPUT,
            b2 in 1..MAX_USDC_INPUT,
            i in 0..TEST_N_COIN,
            j in 0..TEST_N_COIN,
            dx in 0..TEST_MAX_DX_WITHOUT_DECIMAL,
        ) {
            if i == j {
                assert_eq!(1,1);
            }
            else {
                let start_ramp_ts = cmp::max(0, current_ts - MIN_RAMP_DURATION);
                let stop_ramp_ts = cmp::min(u64::MAX, current_ts + MIN_RAMP_DURATION);
                let balances = [b0, b1, b2];

                let model = Model::new(
                    amp_factor.into(),
                    balances.to_vec(),
                    TEST_N_COIN,
                    TEST_RATES.to_vec(),
                    TEST_TRADE_FEE,
                    TEST_WITHDRAW_FEE,
                    0
                );
                let dx_decimal = dx.checked_mul(PRECISION).unwrap().checked_div(TEST_RATES[i as usize]).unwrap();
                let x = balances[i as usize].checked_add(dx_decimal).unwrap();
                check_y(&model, i, j, x, balances, current_ts, start_ramp_ts, stop_ramp_ts);
            }
        }
    }

    #[test]
    fn test_snails_math_get_y_raw_with_random_inputs() {
        for _ in 0..100 {
            let mut rng = rand::thread_rng();

            let amp_factor: u64 = rng.gen_range(MIN_AMP..=MAX_AMP);
            let b0: u128 = rng.gen_range(1..=MAX_DAI_INPUT);
            let b1: u128 = rng.gen_range(1..=MAX_USDT_INPUT);
            let b2: u128 = rng.gen_range(1..=MAX_USDC_INPUT);
            let start_ramp_ts: u64 = rng.gen_range(ZERO_TS as i64..=i64::MAX) as u64;
            let stop_ramp_ts: u64 = rng.gen_range(start_ramp_ts as i64..=i64::MAX) as u64;
            let current_ts: u64 = rng.gen_range(start_ramp_ts as i64..=stop_ramp_ts as i64) as u64;

            let dx: u128 = rng.gen_range(1..=TEST_MAX_DX_WITHOUT_DECIMAL);

            let balances = [b0, b1, b2];
            let model = Model::new(
                amp_factor.into(),
                balances.to_vec(),
                TEST_N_COIN,
                TEST_RATES.to_vec(),
                TEST_TRADE_FEE,
                TEST_WITHDRAW_FEE,
                0,
            );

            for i in 0..TEST_N_COIN {
                let dx_decimal = dx
                    .checked_mul(PRECISION)
                    .unwrap()
                    .checked_div(TEST_RATES[i as usize])
                    .unwrap();
                let x = balances[i as usize].checked_add(dx_decimal).unwrap();
                for j in 0..TEST_N_COIN {
                    if j != i {
                        check_y(
                            &model,
                            i,
                            j,
                            x,
                            balances,
                            current_ts,
                            start_ramp_ts,
                            stop_ramp_ts,
                        );
                    }
                }
            }
        }
    }

    fn check_y_d(
        model: &Model,
        i: u8,
        balances: [u128; 3],
        d: U576,
        current_ts: u64,
        start_ramp_ts: u64,
        stop_ramp_ts: u64,
    ) {
        let snails_swap = SnailStableSwap::new(
            model.amp_factor,
            model.amp_factor,
            current_ts,
            start_ramp_ts,
            stop_ramp_ts,
            TEST_RATES.to_vec(),
        );
        let p_balances = snails_swap.p_balances_convert(&balances.to_vec()).unwrap();
        let y = snails_swap.get_y_d_raw(i, &p_balances, d).unwrap();
        let y_python = model.sim_y_d(i.into(), d.to_string());

        assert_eq!(y.to_string(), y_python.to_string());
    }

    proptest! {
        #[test]
        fn test_get_y_d_raw(
            current_ts in ZERO_TS..u64::MAX,
            amp_factor in MIN_AMP..MAX_AMP,
            b0 in 1..MAX_DAI_INPUT,             // Start at 1 to prevent divide by 0 when computing d
            b1 in 1..MAX_USDT_INPUT,
            b2 in 1..MAX_USDC_INPUT,
            i in 0..TEST_N_COIN,
        ) {

            let start_ramp_ts = cmp::max(0, current_ts - MIN_RAMP_DURATION);
            let stop_ramp_ts = cmp::min(u64::MAX, current_ts + MIN_RAMP_DURATION);
            let balances = [b0, b1, b2];

            let model = Model::new(
                amp_factor.into(),
                balances.to_vec(),
                TEST_N_COIN,
                TEST_RATES.to_vec(),
                TEST_TRADE_FEE,
                TEST_WITHDRAW_FEE,
                0
            );

            let d = U576::from_dec_str( &model.sim_d().to_string() ).unwrap();
            check_y_d(&model, i, balances, d, current_ts, start_ramp_ts, stop_ramp_ts);
        }
    }
    #[test]
    fn test_snails_math_get_y_d_raw_with_random_inputs() {
        for _ in 0..100 {
            let mut rng = rand::thread_rng();

            let amp_factor: u64 = rng.gen_range(MIN_AMP..=MAX_AMP);
            let b0: u128 = rng.gen_range(1..=MAX_DAI_INPUT);
            let b1: u128 = rng.gen_range(1..=MAX_USDT_INPUT);
            let b2: u128 = rng.gen_range(1..=MAX_USDC_INPUT);
            let start_ramp_ts: u64 = rng.gen_range(ZERO_TS as i64..=i64::MAX) as u64;
            let stop_ramp_ts: u64 = rng.gen_range(start_ramp_ts as i64..=i64::MAX) as u64;
            let current_ts: u64 = rng.gen_range(start_ramp_ts as i64..=stop_ramp_ts as i64) as u64;

            let balances = [b0, b1, b2];
            let model = Model::new(
                amp_factor.into(),
                balances.to_vec(),
                TEST_N_COIN,
                TEST_RATES.to_vec(),
                TEST_TRADE_FEE,
                TEST_WITHDRAW_FEE,
                0,
            );

            let d = U576::from_dec_str(&model.sim_d().to_string()).unwrap();
            for i in 0..TEST_N_COIN {
                check_y_d(
                    &model,
                    i,
                    balances,
                    d,
                    current_ts,
                    start_ramp_ts,
                    stop_ramp_ts,
                );
                check_y_d(
                    &model,
                    i,
                    balances,
                    d - 1,
                    current_ts,
                    start_ramp_ts,
                    stop_ramp_ts,
                );
                check_y_d(
                    &model,
                    i,
                    balances,
                    d / 2,
                    current_ts,
                    start_ramp_ts,
                    stop_ramp_ts,
                );
            }
        }
    }

    #[test]
    fn test_snails_math_extreme_parameters() {
        ////// Specific cases  //////

        // MAX balances//
        let max_balances = [MAX_DAI_INPUT, MAX_USDT_INPUT, MAX_USDC_INPUT];
        let max_snails_swap = SnailStableSwap::new(
            std::u64::MAX,
            std::u64::MAX,
            std::u64::MAX,
            std::u64::MAX,
            std::u64::MAX,
            TEST_RATES.to_vec(),
        );
        let model_max_balance = Model::new(
            std::u64::MAX,
            max_balances.to_vec(),
            TEST_N_COIN,
            TEST_RATES.to_vec(),
            TEST_TRADE_FEE,
            TEST_WITHDRAW_FEE,
            0,
        );
        // MAX balances check_d //
        //println!("test_snails_math_specific.. check_d");
        check_d(
            &model_max_balance,
            [MAX_DAI_INPUT, MAX_USDT_INPUT, MAX_USDC_INPUT],
            std::u64::MAX,
            std::u64::MAX,
            std::u64::MAX,
        );

        // MAX balances check_y //
        //println!("test_snails_math_specific.. check_y");
        let max_dx = TEST_MAX_DX_WITHOUT_DECIMAL;
        for i in 0..TEST_N_COIN {
            let dx_decimal = max_dx
                .checked_mul(PRECISION)
                .unwrap()
                .checked_div(TEST_RATES[i as usize])
                .unwrap();
            let x = max_balances[i as usize].checked_add(dx_decimal).unwrap();
            for j in 0..TEST_N_COIN {
                if j != i {
                    check_y(
                        &model_max_balance,
                        i,
                        j,
                        x,
                        max_balances,
                        std::u64::MAX,
                        std::u64::MAX,
                        std::u64::MAX,
                    );
                }
            }
        }

        // MAX balances check_y_d //
        //println!("test_snails_math_specific.. check_y_d");
        for i in 0..TEST_N_COIN {
            let p_max_balances = max_snails_swap
                .p_balances_convert(&max_balances.to_vec())
                .unwrap();
            let max_d = max_snails_swap.get_d(&p_max_balances).unwrap();
            check_y_d(
                &model_max_balance,
                i,
                max_balances,
                max_d,
                std::u64::MAX,
                std::u64::MAX,
                std::u64::MAX,
            );
            check_y_d(
                &model_max_balance,
                i,
                max_balances,
                max_d - 1,
                std::u64::MAX,
                std::u64::MAX,
                std::u64::MAX,
            );
            check_y_d(
                &model_max_balance,
                i,
                max_balances,
                max_d / 2,
                std::u64::MAX,
                std::u64::MAX,
                std::u64::MAX,
            );
        }
    }

    fn check_vp(
        model: &Model,
        balances: [u128; 3],
        current_ts: u64,
        start_ramp_ts: u64,
        stop_ramp_ts: u64,
        total_token_supply: u128,
    ) {
        let snails_swap = SnailStableSwap::new(
            model.amp_factor,
            model.amp_factor,
            current_ts,
            start_ramp_ts,
            stop_ramp_ts,
            TEST_RATES.to_vec(),
        );

        let vp = snails_swap
            .get_virtual_price(&balances.to_vec(), total_token_supply)
            .unwrap();
        let vp_python = model.sim_get_vp();
        //println!("{} {} \n",vp,vp_python);
        assert_eq!(vp, vp_python);
    }

    proptest! {
        #[test]
        fn test_get_virtual_price(
            current_ts in ZERO_TS..u64::MAX,
            amp_factor in MIN_AMP..MAX_AMP,
            b0 in 1..MAX_DAI_INPUT,
            b1 in 1..MAX_USDT_INPUT,
            b2 in 1..MAX_USDC_INPUT,
            total_token_supply in TEST_MAX_DX_WITHOUT_DECIMAL*PRECISION/3..TEST_MAX_DX_WITHOUT_DECIMAL*PRECISION,
        ) {

            let start_ramp_ts = cmp::max(0, current_ts - MIN_RAMP_DURATION);
            let stop_ramp_ts = cmp::min(u64::MAX, current_ts + MIN_RAMP_DURATION);
            let balances = [b0, b1, b2];

            let model = Model::new(
                amp_factor.into(),
                balances.to_vec(),
                TEST_N_COIN,
                TEST_RATES.to_vec(),
                TEST_TRADE_FEE,
                TEST_WITHDRAW_FEE,
                total_token_supply,
            );
            check_vp(&model, balances, current_ts, start_ramp_ts, stop_ramp_ts, total_token_supply);
        }
    }

    fn check_add_liq3(
        model: &Model,
        balances: [u128; 3],
        deposit_amounts: [u128; 3],
        current_ts: u64,
        start_ramp_ts: u64,
        stop_ramp_ts: u64,
        total_token_supply: u128,
    ) {
        let snails_swap = SnailStableSwap::new(
            model.amp_factor,
            model.amp_factor,
            current_ts,
            start_ramp_ts,
            stop_ramp_ts,
            TEST_RATES.to_vec(),
        );

        let poolstatus = snails_swap
            .add_liquidity(
                &deposit_amounts.to_vec(),
                &balances.to_vec(),
                total_token_supply,
                &TEST_FEES_WITH_WITHDRAW_FEE,
            )
            .unwrap();
        let mint_python = model.sim_add_liq3(deposit_amounts);
        //println!("{} {} \n", poolstatus.pool_lp_token_changed, mint_python);
        assert_eq!(poolstatus.pool_lp_token_changed, mint_python);
    }

    proptest! {
        #[test]
        fn test_add_liquidity(
            current_ts in ZERO_TS..u64::MAX,
            amp_factor in MIN_AMP..MAX_AMP,
            b0 in 1..MAX_DAI_INPUT,
            b1 in 1..MAX_USDT_INPUT,
            b2 in 1..MAX_USDC_INPUT,
            m0 in 0..MAX_DAI_INPUT,
            m1 in 0..MAX_USDT_INPUT,
            m2 in 0..MAX_USDC_INPUT,
            total_token_supply in 1..TEST_MAX_TOTAL_SUPPLY,
        ) {

            let start_ramp_ts = cmp::max(0, current_ts - MIN_RAMP_DURATION);
            let stop_ramp_ts = cmp::min(u64::MAX, current_ts + MIN_RAMP_DURATION);
            let balances = [b0, b1, b2];
            let deposit_amounts = [m0, m1, m2];
            let model = Model::new(
                amp_factor.into(),
                balances.to_vec(),
                TEST_N_COIN,
                TEST_RATES.to_vec(),
                TEST_TRADE_FEE,
                TEST_WITHDRAW_FEE,
                total_token_supply,
            );
            check_add_liq3(&model, balances, deposit_amounts, current_ts, start_ramp_ts, stop_ramp_ts, total_token_supply);
        }
    }

    #[test]
    fn test_snails_add_liquidity_with_random_inputs() {
        for _ in 0..100 {
            let mut rng = rand::thread_rng();

            let amp_factor: u64 = rng.gen_range(MIN_AMP..=MAX_AMP);
            let b0: u128 = rng.gen_range(1..=MAX_DAI_INPUT);
            let b1: u128 = rng.gen_range(1..=MAX_USDT_INPUT);
            let b2: u128 = rng.gen_range(1..=MAX_USDC_INPUT);
            let m0: u128 = rng.gen_range(0..=MAX_DAI_INPUT);
            let m1: u128 = rng.gen_range(0..=MAX_USDT_INPUT);
            let m2: u128 = rng.gen_range(0..=MAX_USDC_INPUT);
            let total_token_supply: u128 = rng.gen_range(1..=TEST_MAX_TOTAL_SUPPLY);
            let start_ramp_ts: u64 = rng.gen_range(ZERO_TS as i64..=i64::MAX) as u64;
            let stop_ramp_ts: u64 = rng.gen_range(start_ramp_ts as i64..=i64::MAX) as u64;
            let current_ts: u64 = rng.gen_range(start_ramp_ts as i64..=stop_ramp_ts as i64) as u64;
            //println!("test_snails_add_liquidity_with_random_inputs:");

            let balances = [b0, b1, b2];
            let deposit_amounts = [m0, m1, m2];
            let model = Model::new(
                amp_factor.into(),
                balances.to_vec(),
                TEST_N_COIN,
                TEST_RATES.to_vec(),
                TEST_TRADE_FEE,
                TEST_WITHDRAW_FEE,
                total_token_supply,
            );
            check_add_liq3(
                &model,
                balances,
                deposit_amounts,
                current_ts,
                start_ramp_ts,
                stop_ramp_ts,
                total_token_supply,
            );
        }
    }

    #[test]
    fn test_snails_add_liquidity_extreme_parameters() {
        // MAX balances//
        let max_balances = [MAX_DAI_INPUT, MAX_USDT_INPUT, MAX_USDC_INPUT];
        let max_deposit = [MAX_DAI_INPUT, MAX_USDT_INPUT, MAX_USDC_INPUT];
        let max_total_supply = TEST_MAX_TOTAL_SUPPLY >> 1;
        let model_max_balance = Model::new(
            std::u64::MAX,
            max_balances.to_vec(),
            TEST_N_COIN,
            TEST_RATES.to_vec(),
            TEST_TRADE_FEE,
            TEST_WITHDRAW_FEE,
            max_total_supply,
        );
        check_add_liq3(
            &model_max_balance,
            max_balances,
            max_deposit,
            std::u64::MAX,
            std::u64::MAX,
            std::u64::MAX,
            max_total_supply,
        );
    }

    fn check_swap(
        model: &Model,
        i: u8,
        j: u8,
        dx: u128,
        balances: [u128; 3],
        current_ts: u64,
        start_ramp_ts: u64,
        stop_ramp_ts: u64,
    ) {
        let snails_swap = SnailStableSwap::new(
            model.amp_factor,
            model.amp_factor,
            current_ts,
            start_ramp_ts,
            stop_ramp_ts,
            TEST_RATES.to_vec(),
        );

        let swap_result = snails_swap
            .exchange_impl(
                i,
                j,
                dx,
                &balances.to_vec(),
                &TEST_FEES_WITHOUT_WITHDRAW_FEE,
            )
            .unwrap();
        let (dy_python, _fee_python) = model.sim_exchange(i, j, dx);

        assert_eq!(swap_result.amount_b, dy_python);
        assert_eq!(
            swap_result.new_pool_a,
            balances[i as usize].checked_add(dx).unwrap()
        );
        assert_eq!(
            swap_result.new_pool_b,
            balances[j as usize]
                .checked_sub(swap_result.amount_b)
                .unwrap()
                .checked_sub(swap_result.admin_fee)
                .unwrap()
        );
    }

    proptest! {
        #[test]
        fn test_exchange(
            current_ts in ZERO_TS..u64::MAX,
            amp_factor in MIN_AMP..MAX_AMP,
            b0 in 1..MAX_DAI_INPUT,             // Start at 1 to prevent divide by 0 when computing d
            b1 in 1..MAX_USDT_INPUT,
            b2 in 1..MAX_USDC_INPUT,
            i in 0..TEST_N_COIN,
            j in 0..TEST_N_COIN,
            dx_wo in 0..TEST_MAX_DX_WITHOUT_DECIMAL,
        ) {
            if i == j {
                assert_eq!(1,1);
            }
            else {
                let start_ramp_ts = cmp::max(0, current_ts - MIN_RAMP_DURATION);
                let stop_ramp_ts = cmp::min(u64::MAX, current_ts + MIN_RAMP_DURATION);
                let balances = [b0, b1, b2];

                let model = Model::new(
                    amp_factor.into(),
                    balances.to_vec(),
                    TEST_N_COIN,
                    TEST_RATES.to_vec(),
                    TEST_TRADE_FEE,
                    TEST_WITHDRAW_FEE,
                    0
                );
                let dx = dx_wo.checked_mul(PRECISION).unwrap().checked_div(TEST_RATES[i as usize]).unwrap();
                check_swap(&model, i, j, dx, balances, current_ts, start_ramp_ts, stop_ramp_ts);
            }
        }
    }

    #[test]
    fn test_snails_exchange_with_random_inputs() {
        for _ in 0..100 {
            let mut rng = rand::thread_rng();

            let amp_factor: u64 = rng.gen_range(MIN_AMP..=MAX_AMP);
            let b0: u128 = rng.gen_range(1..=MAX_DAI_INPUT);
            let b1: u128 = rng.gen_range(1..=MAX_USDT_INPUT);
            let b2: u128 = rng.gen_range(1..=MAX_USDC_INPUT);
            let start_ramp_ts: u64 = rng.gen_range(ZERO_TS as i64..=i64::MAX) as u64;
            let stop_ramp_ts: u64 = rng.gen_range(start_ramp_ts as i64..=i64::MAX) as u64;
            let current_ts: u64 = rng.gen_range(start_ramp_ts as i64..=stop_ramp_ts as i64) as u64;

            let dx_wo: u128 = rng.gen_range(1..=TEST_MAX_DX_WITHOUT_DECIMAL);
            //println!("test_snails_exchange_with_random_inputs:");

            let balances = [b0, b1, b2];
            let model = Model::new(
                amp_factor.into(),
                balances.to_vec(),
                TEST_N_COIN,
                TEST_RATES.to_vec(),
                TEST_TRADE_FEE,
                TEST_WITHDRAW_FEE,
                0,
            );

            for i in 0..TEST_N_COIN {
                let dx = dx_wo
                    .checked_mul(PRECISION)
                    .unwrap()
                    .checked_div(TEST_RATES[i as usize])
                    .unwrap();
                for j in 0..TEST_N_COIN {
                    if j != i {
                        check_y(
                            &model,
                            i,
                            j,
                            dx,
                            balances,
                            current_ts,
                            start_ramp_ts,
                            stop_ramp_ts,
                        );
                    }
                }
            }
        }
    }

    fn check_remove_liq(
        model: &Model,
        balances: [u128; 3],
        current_ts: u64,
        start_ramp_ts: u64,
        stop_ramp_ts: u64,
        removed_lp_amount: u128,
        total_token_supply: u128,
    ) {
        let snails_swap = SnailStableSwap::new(
            model.amp_factor,
            model.amp_factor,
            current_ts,
            start_ramp_ts,
            stop_ramp_ts,
            TEST_RATES.to_vec(),
        );

        let pool_status = snails_swap
            .remove_liquidity(
                removed_lp_amount,
                &balances.to_vec(),
                total_token_supply,
                //&TEST_FEES_WITHOUT_WITHDRAW_FEE
                &TEST_FEES_WITH_WITHDRAW_FEE,
            )
            .unwrap();
        let (m0_python, m1_python, m2_python) = model.sim_remove_liq3(removed_lp_amount, 99);

        assert_eq!(pool_status.recieved_amount[0], m0_python);
        assert_eq!(pool_status.recieved_amount[1], m1_python);
        assert_eq!(pool_status.recieved_amount[2], m2_python);
    }

    proptest! {
        #[test]
        fn test_remove_liq(
            current_ts in ZERO_TS..u64::MAX,
            amp_factor in MIN_AMP..MAX_AMP,
            b0 in 1..MAX_DAI_INPUT,             // Start at 1 to prevent divide by 0 when computing d
            b1 in 1..MAX_USDT_INPUT,
            b2 in 1..MAX_USDC_INPUT,
            remove_lp in 1..TEST_MAX_TOTAL_SUPPLY,
            total_token_supply in 1..TEST_MAX_TOTAL_SUPPLY,
        ) {
            if remove_lp <= total_token_supply {
                let start_ramp_ts = cmp::max(0, current_ts - MIN_RAMP_DURATION);
                let stop_ramp_ts = cmp::min(u64::MAX, current_ts + MIN_RAMP_DURATION);
                let balances = [b0, b1, b2];

                let model = Model::new(
                    amp_factor.into(),
                    balances.to_vec(),
                    TEST_N_COIN,
                    TEST_RATES.to_vec(),
                    TEST_TRADE_FEE,
                    TEST_WITHDRAW_FEE, //0,
                    total_token_supply
                );
                check_remove_liq(&model, balances, current_ts, start_ramp_ts, stop_ramp_ts, remove_lp, total_token_supply);
            }
        }
    }

    fn check_remove_liq_imba(
        model: &Model,
        balances: [u128; 3],
        remove_amounts: [u128; 3],
        current_ts: u64,
        start_ramp_ts: u64,
        stop_ramp_ts: u64,
        total_token_supply: u128,
    ) {
        let snails_swap = SnailStableSwap::new(
            model.amp_factor,
            model.amp_factor,
            current_ts,
            start_ramp_ts,
            stop_ramp_ts,
            TEST_RATES.to_vec(),
        );

        let pool_status = snails_swap
            .remove_liquidity_imbalance(
                &remove_amounts.to_vec(),
                &balances.to_vec(),
                total_token_supply,
                //&TEST_FEES_WITHOUT_WITHDRAW_FEE
                &TEST_FEES_WITH_WITHDRAW_FEE,
            )
            .unwrap();

        let burn_lp_python =
            model.sim_remove_liq_imba3(remove_amounts[0], remove_amounts[1], remove_amounts[2]);
        assert_eq!(pool_status.pool_lp_token_changed, burn_lp_python);
    }

    proptest! {
        #[test]
        fn test_remove_liq_imba(
            current_ts in ZERO_TS..u64::MAX,
            amp_factor in MIN_AMP..MAX_AMP,
            b0 in 1..MAX_DAI_INPUT,             // Start at 1 to prevent divide by 0 when computing d
            b1 in 1..MAX_USDT_INPUT,
            b2 in 1..MAX_USDC_INPUT,
            m0 in 1..MAX_DAI_INPUT,
            m1 in 1..MAX_USDT_INPUT,
            m2 in 1..MAX_USDC_INPUT,
            total_token_supply in 1..TEST_MAX_TOTAL_SUPPLY,
        ) {
            if m0<=b0 && m1<=b1 && m2<=b2 {
                let charge = TEST_TRADE_FEE + TEST_WITHDRAW_FEE;
                let m0_fee = U256::from(m0) * U256::from(TEST_FEE_DENOMINATOR) / (TEST_FEE_DENOMINATOR + charge);
                let m0_u = m0_fee.to_u128().unwrap();
                let m1_fee = U256::from(m1) * U256::from(TEST_FEE_DENOMINATOR) / (TEST_FEE_DENOMINATOR + charge);
                let m1_u = m1_fee.to_u128().unwrap();
                let m2_fee = U256::from(m2) * U256::from(TEST_FEE_DENOMINATOR) / (TEST_FEE_DENOMINATOR + charge);
                let m2_u = m2_fee.to_u128().unwrap();
                let start_ramp_ts = cmp::max(0, current_ts - MIN_RAMP_DURATION);
                let stop_ramp_ts = cmp::min(u64::MAX, current_ts + MIN_RAMP_DURATION);
                let balances = [b0, b1, b2];
                let remove_amounts: [u128;3] = [m0_u, m1_u, m2_u];
                let model = Model::new(
                    amp_factor.into(),
                    balances.to_vec(),
                    TEST_N_COIN,
                    TEST_RATES.to_vec(),
                    TEST_TRADE_FEE,
                    TEST_WITHDRAW_FEE, //0,
                    total_token_supply
                );
                check_remove_liq_imba(&model, balances, remove_amounts, current_ts, start_ramp_ts, stop_ramp_ts, total_token_supply);
            }
        }
    }

    #[test]
    fn test_snails_remove_liq_imba_with_random_inputs() {
        for _ in 0..200 {
            let mut rng = rand::thread_rng();

            let amp_factor: u64 = rng.gen_range(MIN_AMP..=MAX_AMP);
            let b0: u128 = rng.gen_range(1..=MAX_DAI_INPUT);
            let b1: u128 = rng.gen_range(1..=MAX_USDT_INPUT);
            let b2: u128 = rng.gen_range(1..=MAX_USDC_INPUT);
            let m0: u128 = rng.gen_range(1..=MAX_DAI_INPUT);
            let m1: u128 = rng.gen_range(1..=MAX_USDT_INPUT);
            let m2: u128 = rng.gen_range(1..=MAX_USDC_INPUT);
            if m0 > b0 || m1 > b1 || m2 > b2 {
                continue;
            }
            let total_token_supply: u128 = rng.gen_range(1..=TEST_MAX_TOTAL_SUPPLY);
            let start_ramp_ts: u64 = rng.gen_range(ZERO_TS as i64..=i64::MAX) as u64;
            let stop_ramp_ts: u64 = rng.gen_range(start_ramp_ts as i64..=i64::MAX) as u64;
            let current_ts: u64 = rng.gen_range(start_ramp_ts as i64..=stop_ramp_ts as i64) as u64;

            //println!("test_snails_remove_liq_imba_with_random_inputs:");
            let charge = TEST_TRADE_FEE + TEST_WITHDRAW_FEE;
            let m0_fee =
                U256::from(m0) * U256::from(TEST_FEE_DENOMINATOR) / (TEST_FEE_DENOMINATOR + charge);
            let m0_u = m0_fee.to_u128().unwrap();
            let m1_fee =
                U256::from(m1) * U256::from(TEST_FEE_DENOMINATOR) / (TEST_FEE_DENOMINATOR + charge);
            let m1_u = m1_fee.to_u128().unwrap();
            let m2_fee =
                U256::from(m2) * U256::from(TEST_FEE_DENOMINATOR) / (TEST_FEE_DENOMINATOR + charge);
            let m2_u = m2_fee.to_u128().unwrap();

            let balances = [b0, b1, b2];
            let remove_amounts = [m0_u, m1_u, m2_u];
            let model = Model::new(
                amp_factor.into(),
                balances.to_vec(),
                TEST_N_COIN,
                TEST_RATES.to_vec(),
                TEST_TRADE_FEE,
                TEST_WITHDRAW_FEE, //0,
                total_token_supply,
            );
            check_remove_liq_imba(
                &model,
                balances,
                remove_amounts,
                current_ts,
                start_ramp_ts,
                stop_ramp_ts,
                total_token_supply,
            );
        }
    }

    proptest! {
        #[test]
        #[should_panic(excepted = "remaining balance not enough for trade fee")]
        fn test_snails_remove_liq_imba_remaining_balance_not_enough_for_trade_fee_proptest(
            current_ts in (ZERO_TS + MIN_RAMP_DURATION)..u64::MAX,
            amp_factor in MIN_AMP..MAX_AMP,
            b0 in 2..MAX_DAI_INPUT,
            b1 in 2..MAX_USDT_INPUT,
            b2 in 2..MAX_USDC_INPUT,
            total_token_supply in 1..TEST_MAX_TOTAL_SUPPLY,
        ) {
            let start_ramp_ts = cmp::max(0, current_ts - MIN_RAMP_DURATION);
            let stop_ramp_ts = cmp::min(u64::MAX, current_ts + MIN_RAMP_DURATION);

            let balances = [b0, b1, b2];
            let remove_amounts = [b0-1, b1-1, b2-1];

            let snails_swap = SnailStableSwap::new(
                amp_factor,
                amp_factor,
                current_ts,
                start_ramp_ts,
                stop_ramp_ts,
                TEST_RATES.to_vec(),
            );
            let _pool_status = snails_swap
            .remove_liquidity_imbalance(
                &remove_amounts.to_vec(),
                &balances.to_vec(),
                total_token_supply,
                //&TEST_FEES_WITHOUT_WITHDRAW_FEE
                &TEST_FEES_WITH_WITHDRAW_FEE,
            );
        }
    }

    proptest! {
        #[test]
        #[should_panic(excepted = "remaining balance not enough for withdraw fee")]
        fn test_snails_remove_liq_imba_remaining_balance_not_enough_for_withdraw_fee_proptest(
            current_ts in (ZERO_TS + MIN_RAMP_DURATION)..u64::MAX,
            amp_factor in MIN_AMP..MAX_AMP,
            b0 in 1..MAX_DAI_INPUT,
            b1 in 1..MAX_USDT_INPUT,
            b2 in 1..MAX_USDC_INPUT,
            total_token_supply in 1..TEST_MAX_TOTAL_SUPPLY,
        ) {
            let start_ramp_ts = cmp::max(0, current_ts - MIN_RAMP_DURATION);
            let stop_ramp_ts = cmp::min(u64::MAX, current_ts + MIN_RAMP_DURATION);

            let m0_fee = U256::from(b0) * U256::from(TEST_FEE_DENOMINATOR) / (TEST_FEE_DENOMINATOR + TEST_WITHDRAW_FEE);
            let m0_u = m0_fee.to_u128().unwrap() + 1;
            let m1_fee = U256::from(b1) * U256::from(TEST_FEE_DENOMINATOR) / (TEST_FEE_DENOMINATOR + TEST_WITHDRAW_FEE);
            let m1_u = m1_fee.to_u128().unwrap() + 1;
            let m2_fee = U256::from(b2) * U256::from(TEST_FEE_DENOMINATOR) / (TEST_FEE_DENOMINATOR + TEST_WITHDRAW_FEE);
            let m2_u = m2_fee.to_u128().unwrap() + 1;

            let balances = [b0, b1, b2];
            let remove_amounts = [m0_u, m1_u, m2_u];

            let snails_swap = SnailStableSwap::new(
                amp_factor,
                amp_factor,
                current_ts,
                start_ramp_ts,
                stop_ramp_ts,
                TEST_RATES.to_vec(),
            );
            let _pool_status = snails_swap
            .remove_liquidity_imbalance(
                &remove_amounts.to_vec(),
                &balances.to_vec(),
                total_token_supply,
                //&TEST_FEES_WITHOUT_WITHDRAW_FEE
                &TEST_FEES_WITH_WITHDRAW_FEE,
            );
        }
    }

    fn check_remove_one_coin(
        model: &Model,
        i: u8,
        balances: [u128; 3],
        current_ts: u64,
        start_ramp_ts: u64,
        stop_ramp_ts: u64,
        removed_lp_amount: u128,
        total_token_supply: u128,
    ) {
        let snails_swap = SnailStableSwap::new(
            model.amp_factor,
            model.amp_factor,
            current_ts,
            start_ramp_ts,
            stop_ramp_ts,
            TEST_RATES.to_vec(),
        );

        let pool_status = snails_swap
            .remove_liquidity_one_coin(
                i,
                removed_lp_amount,
                &balances.to_vec(),
                total_token_supply,
                //&TEST_FEES_WITHOUT_WITHDRAW_FEE
                &TEST_FEES_WITH_WITHDRAW_FEE,
            )
            .unwrap();
        let amount_python = model.sim_calc_withdraw_one_coin(removed_lp_amount, i);
        assert_eq!(pool_status.recieved_amount[i as usize], amount_python);
    }

    proptest! {
        #[test]
        fn test_remove_one_coin_proptest(
            current_ts in ZERO_TS..u64::MAX,
            amp_factor in MIN_AMP..MAX_AMP,
            b0 in 1..MAX_DAI_INPUT,             // Start at 1 to prevent divide by 0 when computing d
            b1 in 1..MAX_USDT_INPUT,
            b2 in 1..MAX_USDC_INPUT,
            i in 0..TEST_N_COIN,
            remove_lp in 1..TEST_MAX_TOTAL_SUPPLY,
            total_token_supply in 1..TEST_MAX_TOTAL_SUPPLY,
        ) {
            if remove_lp <= total_token_supply {
                let start_ramp_ts = cmp::max(0, current_ts - MIN_RAMP_DURATION);
                let stop_ramp_ts = cmp::min(u64::MAX, current_ts + MIN_RAMP_DURATION);
                let balances = [b0, b1, b2];

                let model = Model::new(
                    amp_factor.into(),
                    balances.to_vec(),
                    TEST_N_COIN,
                    TEST_RATES.to_vec(),
                    TEST_TRADE_FEE,
                    TEST_WITHDRAW_FEE, //0,
                    total_token_supply
                );
                check_remove_one_coin(&model, i, balances, current_ts, start_ramp_ts, stop_ramp_ts, remove_lp, total_token_supply);
            }
        }
    }

    #[test]
    fn test_snails_remove_one_coin_with_random_inputs() {
        for _ in 0..200 {
            let mut rng = rand::thread_rng();

            let amp_factor: u64 = rng.gen_range(MIN_AMP..=MAX_AMP);
            let b0: u128 = rng.gen_range(1..=MAX_DAI_INPUT);
            let b1: u128 = rng.gen_range(1..=MAX_USDT_INPUT);
            let b2: u128 = rng.gen_range(1..=MAX_USDC_INPUT);
            let remove_lp: u128 = rng.gen_range(1..=TEST_MAX_TOTAL_SUPPLY);
            let total_token_supply: u128 = rng.gen_range(1..=TEST_MAX_TOTAL_SUPPLY);
            if remove_lp > total_token_supply {
                continue;
            }
            let start_ramp_ts: u64 = rng.gen_range(ZERO_TS as i64..=i64::MAX) as u64;
            let stop_ramp_ts: u64 = rng.gen_range(start_ramp_ts as i64..=i64::MAX) as u64;
            let current_ts: u64 = rng.gen_range(start_ramp_ts as i64..=stop_ramp_ts as i64) as u64;

            //println!("test_snails_remove_one_coin_with_random_inputs:");

            let balances = [b0, b1, b2];
            let model = Model::new(
                amp_factor.into(),
                balances.to_vec(),
                TEST_N_COIN,
                TEST_RATES.to_vec(),
                TEST_TRADE_FEE,
                TEST_WITHDRAW_FEE, //0,
                total_token_supply,
            );

            for i in 0..TEST_N_COIN {
                check_remove_one_coin(
                    &model,
                    i,
                    balances,
                    current_ts,
                    start_ramp_ts,
                    stop_ramp_ts,
                    remove_lp,
                    total_token_supply,
                );
            }
        }
    }
}
