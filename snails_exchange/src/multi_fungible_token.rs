use near_contract_standards::fungible_token::metadata::FungibleTokenMetadata;
use near_sdk::json_types::U128;
use near_sdk::{ext_contract, near_bindgen, Balance, PromiseOrValue};

use crate::utils::{GAS_FOR_FT_TRANSFER_CALL, GAS_FOR_RESOLVE_TRANSFER, NO_DEPOSIT};
use crate::*;

#[ext_contract(ext_self)]
trait MFTTokenResolver {
    fn mft_resolve_transfer(
        &mut self,
        token_id: String,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128;
}

#[ext_contract(ext_share_token_receiver)]
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

/// This is used to parse token_id fields in mft protocol used in ref,
/// So, if we choose #nn as a partern, should announce it in mft protocol.
/// cause : is not allowed in a normal account id, it can be a partern leading char
fn try_identify_pool_id(token_id: &String) -> Result<u64, &'static str> {
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
    if let Ok(pool_id) = try_identify_pool_id(&token_id) {
        TokenOrPool::Pool(pool_id)
    } else {
        TokenOrPool::Token(AccountId::try_from(token_id.clone()).unwrap())
    }
}

#[near_bindgen]
impl SnailSwap {
    fn internal_mft_transfer(
        &mut self,
        token_id: String,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        amount: u128,
        memo: Option<String>,
    ) {
        assert_ne!(sender_id, receiver_id, "{}", TRANSFER_TO_SELF);
        self.assert_contract_running();
        match parse_token_id(token_id) {
            TokenOrPool::Pool(pool_id) => {
                let mut pool = self.pools.get(pool_id).expect("ERR_NO_POOL");
                pool.share_transfer(sender_id, receiver_id, amount);
                self.pools.replace(pool_id, &pool);
                log!(
                    "Transfer shares {} pool: {} from {} to {}",
                    pool_id,
                    amount,
                    sender_id,
                    receiver_id
                );
            }
            TokenOrPool::Token(token_id) => {
                let mut sender_account: Account = self.internal_unwrap_account(&sender_id);
                let mut receiver_account: Account = self.internal_unwrap_account(&receiver_id);

                sender_account.withdraw(&token_id, amount);
                receiver_account.deposit(&token_id, amount);
                self.internal_save_account(&sender_id, sender_account);
                self.internal_save_account(&receiver_id, receiver_account);
                log!(
                    "Transfer {}: {} from {} to {}",
                    token_id,
                    amount,
                    sender_id,
                    receiver_id
                );
            }
        }
        if let Some(memo) = memo {
            log!("Memo: {}", memo);
        }
    }

    fn internal_mft_balance(&self, token_id: String, account_id: &AccountId) -> Balance {
        match parse_token_id(token_id) {
            TokenOrPool::Pool(pool_id) => {
                let pool = self.pools.get(pool_id).expect("ERR_NO_POOL");
                pool.share_balances(account_id)
            }
            TokenOrPool::Token(token_id) => self.internal_get_deposit(account_id, &token_id),
        }
    }

    /// Returns the balance of the given account. If the account doesn't exist will return `"0"`.
    pub fn mft_balance_of(&self, token_id: String, account_id: AccountId) -> U128 {
        self.internal_mft_balance(token_id, &account_id).into()
    }

    /// Returns the total supply of the given token, if the token is one of the pools.
    /// If token references external token - fails with unimplemented.
    pub fn mft_total_supply(&self, token_id: String) -> U128 {
        match parse_token_id(token_id) {
            TokenOrPool::Pool(pool_id) => {
                let pool = self.pools.get(pool_id).expect("ERR_NO_POOL");
                U128(pool.share_total_balance())
            }
            TokenOrPool::Token(_token_id) => unimplemented!(),
        }
    }

    /// Register LP token of given pool for given account.
    /// Fails if token_id is not a pool.
    #[payable]
    pub fn mft_register(&mut self, token_id: String, account_id: AccountId) {
        self.assert_contract_running();
        let prev_storage = env::storage_usage();
        match parse_token_id(token_id) {
            TokenOrPool::Token(_) => env::panic_str("ERR_INVALID_REGISTER"),
            TokenOrPool::Pool(pool_id) => {
                let mut pool = self.pools.get(pool_id).expect("ERR_NO_POOL");
                pool.share_register(&account_id);
                self.pools.replace(pool_id, &pool);
                self.internal_check_storage(prev_storage);
            }
        }
    }

    pub fn is_lp_token_registered(&self, token_id: String, account_id: AccountId) -> bool {
        match parse_token_id(token_id) {
            TokenOrPool::Token(_) => env::panic_str("ERR_INVALID_REGISTER"),
            TokenOrPool::Pool(pool_id) => {
                let pool = self.pools.get(pool_id).expect("ERR_NO_POOL");
                pool.is_lp_token_registered(&account_id)
            }
        }
    }

    /// Transfer LP tokens.
    #[payable]
    pub fn mft_transfer(
        &mut self,
        token_id: String,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
    ) {
        assert_one_yocto();
        self.assert_contract_running();
        self.internal_mft_transfer(
            token_id,
            &env::predecessor_account_id(),
            &receiver_id,
            amount.0,
            memo,
        );
    }

    #[payable]
    pub fn mft_transfer_call(
        &mut self,
        token_id: String,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        assert_one_yocto();
        self.assert_contract_running();
        let sender_id = env::predecessor_account_id();
        self.internal_mft_transfer(token_id.clone(), &sender_id, &receiver_id, amount.0, memo);
        assert!(
            env::prepaid_gas() >= GAS_FOR_FT_TRANSFER_CALL,
            "ERR prepaid_gas < GAS_FOR_FT_TRANSFER_CALL"
        );
        ext_share_token_receiver::mft_on_transfer(
            token_id.clone(),
            sender_id.clone(),
            amount,
            msg,
            receiver_id.clone(),
            NO_DEPOSIT,
            env::prepaid_gas() - GAS_FOR_FT_TRANSFER_CALL,
        )
        .then(ext_self::mft_resolve_transfer(
            token_id,
            sender_id,
            receiver_id.clone(),
            amount,
            env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_RESOLVE_TRANSFER,
        ))
        .into()
    }

    /// Returns how much was refunded back to the sender.
    /// If sender removed account in the meantime, the tokens are sent to the owner account.
    /// Tokens are never burnt.
    #[private]
    pub fn mft_resolve_transfer(
        &mut self,
        token_id: String,
        sender_id: AccountId,
        receiver_id: &AccountId,
        amount: U128,
    ) -> U128 {
        let unused_amount = match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(value) => {
                if let Ok(unused_amount) = near_sdk::serde_json::from_slice::<U128>(&value) {
                    std::cmp::min(amount.0, unused_amount.0)
                } else {
                    amount.0
                }
            }
            PromiseResult::Failed => amount.0,
        };
        if unused_amount > 0 {
            let receiver_balance = self.internal_mft_balance(token_id.clone(), &receiver_id);
            if receiver_balance > 0 {
                let refund_amount = std::cmp::min(receiver_balance, unused_amount);
                // If sender's account was deleted, we assume that they have also withdrew all the liquidity from pools.
                // Funds are sent to the owner account.
                let refund_to = if self.accounts.get(&sender_id).is_some() {
                    sender_id
                } else {
                    self.owner_id.clone()
                };
                self.internal_mft_transfer(token_id, &receiver_id, &refund_to, refund_amount, None);
            }
        }
        U128(unused_amount)
    }

    pub fn mft_metadata(&self, token_id: String) -> FungibleTokenMetadata {
        match parse_token_id(token_id) {
            TokenOrPool::Pool(pool_id) => FungibleTokenMetadata {
                spec: "mft-1.0.0".to_string(),
                name: format!("stableSwap-pool-{}", pool_id),
                symbol: format!("STABLE-POOL-{}", pool_id),
                icon: None,
                reference: None,
                reference_hash: None,
                decimals: 24,
            },
            TokenOrPool::Token(_token_id) => unimplemented!(),
        }
    }
}
