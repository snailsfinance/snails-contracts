use crate::errors::*;
use crate::farm_seed::SeedType;
use crate::utils::MFT_TAG;
use crate::*;
use near_sdk::json_types::U128;
use near_sdk::PromiseOrValue;

use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;

#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    /// Callback on receiving tokens by this contract.
    /// transfer reward token with specific msg indicate
    /// which farm to be deposited to.
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.assert_contract_running();
        let sender: AccountId = sender_id.into();
        let amount: u128 = amount.into();
        if msg.is_empty() {
            // ****** seed Token deposit in ********

            // if seed not exist, it will panic
            let seed_farm = self.get_seed(&env::predecessor_account_id().to_string());
            if amount < seed_farm.get_ref().min_deposit {
                env::panic_str(
                    format!(
                        "{} {}",
                        BELOW_MIN_SEED_DEPOSITED,
                        seed_farm.get_ref().min_deposit
                    )
                    .as_str(),
                )
            }

            self.internal_seed_deposit(
                &env::predecessor_account_id().to_string(),
                &sender,
                amount.into(),
                SeedType::FT,
            );
            self.assert_storage_usage(&sender);

            env::log_str(
                format!(
                    "{} deposit FT seed {} with amount {}.",
                    sender,
                    env::predecessor_account_id(),
                    amount,
                )
                .as_str(),
            );
            PromiseOrValue::Value(U128(0))
        } else {
            // ****** reward Token deposit in ********
            let farm_id = msg
                .parse::<FarmId>()
                .expect(&format!("{}", INVALID_FARM_ID));
            let mut farm = self.data().farms.get(&farm_id).expect(FARM_NOT_EXIST);

            // update farm
            assert_eq!(
                farm.get_reward_token(),
                env::predecessor_account_id(),
                "{}",
                INVALID_FARM_REWARD
            );
            if let Some(cur_remain) = farm.add_reward(&amount) {
                self.data_mut().farms.insert(&farm_id, &farm);
                let old_balance = self
                    .data()
                    .reward_info
                    .get(&env::predecessor_account_id())
                    .unwrap_or(0);
                self.data_mut().reward_info.insert(
                    &env::predecessor_account_id(),
                    &(old_balance.checked_add(amount).unwrap()),
                );
                env::log_str(
                    format!(
                        "{} added {} Reward Token, Now has {} left",
                        sender, amount, cur_remain
                    )
                    .as_str(),
                );
                PromiseOrValue::Value(U128(0))
            } else {
                env::panic_str(format!("{}", INVALID_FARM_STATUS).as_str())
            }
        }
    }
}

pub trait MFTTokenReceiver {
    fn mft_on_transfer(
        &mut self,
        token_id: String,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128>;
}

enum TokenOrPool {
    Token(AccountId),
    Pool(u64),
}

/// a sub token would use a format ":<u64>"
fn try_identify_sub_token_id(token_id: &String) -> Result<u64, &'static str> {
    if token_id.starts_with(":") {
        if let Ok(pool_id) = str::parse::<u64>(&token_id[1..token_id.len()]) {
            Ok(pool_id)
        } else {
            Err("Illegal pool id")
        }
    } else {
        Err("Illegal pool id")
    }
}

fn parse_token_id(token_id: String) -> TokenOrPool {
    if let Ok(pool_id) = try_identify_sub_token_id(&token_id) {
        TokenOrPool::Pool(pool_id)
    } else {
        TokenOrPool::Token(AccountId::try_from(token_id.clone()).unwrap())
    }
}

/// seed token deposit
#[near_bindgen]
impl MFTTokenReceiver for Contract {
    /// Callback on receiving tokens by this contract.
    fn mft_on_transfer(
        &mut self,
        token_id: String,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.assert_contract_running();
        let seed_id: String;
        match parse_token_id(token_id.clone()) {
            TokenOrPool::Pool(pool_id) => {
                seed_id = format!("{}{}{}", env::predecessor_account_id(), MFT_TAG, pool_id);
            }
            TokenOrPool::Token(_) => {
                // for seed deposit, using mft to transfer 'root' token is not supported.
                env::panic_str(ILLEGAL_TOKEN_ID);
            }
        }

        assert!(msg.is_empty(), "ERR_MSG_INCORRECT");

        // if seed not exist, it will panic
        let amount: u128 = amount.into();
        let seed_farm = self.get_seed(&seed_id);
        if amount < seed_farm.get_ref().min_deposit {
            env::panic_str(
                format!(
                    "{} {}",
                    BELOW_MIN_SEED_DEPOSITED,
                    seed_farm.get_ref().min_deposit
                )
                .as_str(),
            )
        }
        self.internal_seed_deposit(&seed_id, &sender_id, amount, SeedType::MFT);

        self.assert_storage_usage(&sender_id);

        env::log_str(
            format!(
                "{} deposit MFT seed {} with amount {}.",
                sender_id, seed_id, amount,
            )
            .as_str(),
        );

        PromiseOrValue::Value(U128(0))
    }
}
