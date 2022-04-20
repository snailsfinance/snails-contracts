use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};

use near_sdk::collections::UnorderedMap;

use near_sdk::{assert_one_yocto, env, near_bindgen, AccountId, Balance, Promise};

use crate::*;

pub const U128_STORAGE: StorageUsage = 16;
const U64_STORAGE: StorageUsage = 8;
const U32_STORAGE: StorageUsage = 4;
/// max length of account id is 64 bytes. We charge per byte.
const ACC_ID_STORAGE: StorageUsage = 64;
/// As a key, 4 bytes length would be added to the head
pub const ACC_ID_AS_KEY_STORAGE: StorageUsage = ACC_ID_STORAGE + 4;
pub const KEY_PREFIX_ACC: StorageUsage = 64;
/// As a near_sdk::collection key, 1 byte for prefiex
const ACC_ID_AS_CLT_KEY_STORAGE: StorageUsage = ACC_ID_AS_KEY_STORAGE + 1;

// ACC_ID: the Contract accounts map key length
// + VAccount enum: 1 byte
// + U128_STORAGE: near_amount storage
// + U32_STORAGE: tokens HashMap length
// + U64_STORAGE: storage_used
pub const INIT_ACCOUNT_STORAGE: StorageUsage =
    ACC_ID_AS_CLT_KEY_STORAGE + 1 + U128_STORAGE + U32_STORAGE + U32_STORAGE + U64_STORAGE;

#[derive(BorshDeserialize, BorshSerialize)]
pub enum VAccount {
    Current(Account),
}

impl VAccount {
    /// Upgrades from other versions to the currently used version.
    pub fn into_current(self) -> Account {
        match self {
            VAccount::Current(account) => account,
        }
    }
}

impl From<Account> for VAccount {
    fn from(account: Account) -> Self {
        VAccount::Current(account)
    }
}

/// Account deposits information and storage cost.
#[derive(BorshSerialize, BorshDeserialize)]
pub struct Account {
    /// Native NEAR amount sent to the exchange.
    /// Used for storage right now, but in future can be used for trading as well.
    pub near_amount: Balance,
    pub tokens: UnorderedMap<AccountId, Balance>,
    pub storage_used: StorageUsage,
}

impl Account {
    /// Deposit amount to the balance of given token.
    pub fn deposit(&mut self, token: &AccountId, amount: Balance) {
        env::log_str(format!("Account deposit token {}, amount {} ", token, amount).as_str());

        if let Some(x) = self.tokens.get(token) {
            self.tokens.insert(token, &(amount.checked_add(x).unwrap()));
        } else {
            self.tokens.insert(token, &amount);
        }
    }

    /// Deposit amount to the balance of given token,
    /// if given token not register and not enough storage, deposit fails
    pub(crate) fn deposit_with_storage_check(
        &mut self,
        token: &AccountId,
        amount: Balance,
    ) -> bool {
        if let Some(balance) = self.tokens.get(token) {
            // token has been registered, just add without storage check,
            let new_balance: Balance = balance.checked_add(amount).unwrap();
            self.tokens.insert(token, &new_balance);
            true
        } else {
            // check storage after insert, if fail should unregister the token
            self.tokens.insert(token, &(amount));
            if self.storage_usage() <= self.near_amount {
                true
            } else {
                self.tokens.remove(token);
                false
            }
        }
    }

    /// Returns amount of $NEAR necessary to cover storage used by this data structure.
    pub fn storage_usage(&self) -> Balance {
        (INIT_ACCOUNT_STORAGE
            + self.tokens.len() as u64 * (KEY_PREFIX_ACC + ACC_ID_AS_KEY_STORAGE + U128_STORAGE))
            as u128
            * env::storage_byte_cost()
    }

    /// Returns how much NEAR is available for storage.
    pub fn storage_available(&self) -> Balance {
        //  avoid math overflow
        let locked = self.storage_usage();
        if self.near_amount > locked {
            self.near_amount - locked
        } else {
            0
        }
    }

    pub fn new(account_id: &AccountId) -> Self {
        Account {
            near_amount: 0,
            tokens: UnorderedMap::new(StorageKey::AccountTokens {
                account_id: account_id.clone(),
            }),
            storage_used: 0,
        }
    }

    pub fn get_tokens(&self) -> Vec<AccountId> {
        let a: Vec<AccountId> = self.tokens.keys().collect();
        a
    }

    pub fn get_balance(&self, token_id: &AccountId) -> Option<Balance> {
        if let Some(token_balance) = self.tokens.get(token_id) {
            Some(token_balance)
        } else {
            None
        }
    }

    /// Asserts there is sufficient amount of $NEAR to cover storage usage.
    pub fn assert_storage_usage(&self) {
        assert!(
            self.storage_usage() <= self.near_amount,
            "Insufficient storage deposit, expected [{}] actually [{}]",
            self.storage_usage(),
            self.near_amount
        );
    }

    /// Returns minimal account deposit storage usage possible.
    pub fn min_storage_usage() -> Balance {
        INIT_ACCOUNT_STORAGE as Balance * env::storage_byte_cost()
    }

    /// Registers given token and set balance to 0.
    pub(crate) fn register(&mut self, token_ids: &Vec<AccountId>) {
        for token_id in token_ids {
            if self.get_balance(&token_id).is_none() {
                self.tokens.insert(&token_id, &0);
            }
        }
    }

    /// Unregisters `token_id` from this account balance.
    /// Panics if the `token_id` balance is not 0.
    pub(crate) fn unregister(&mut self, token_id: &AccountId) {
        let amount = self.tokens.remove(token_id).unwrap_or_default();
        assert_eq!(amount, 0, "{}", NON_ZERO_TOKEN_BALANCE);
    }

    /// Withdraw amount of `token` from the internal balance.
    /// Panics if `amount` is bigger than the current balance.
    pub(crate) fn withdraw(&mut self, token: &AccountId, amount: Balance) {
        env::log_str(format!("withdraw token {}, amount {}", token, amount).as_str());

        if let Some(x) = self.tokens.get(token) {
            assert!(x >= amount, "Not enough tokens in deposit, current [{}]", x);
            self.tokens.insert(token, &(x - amount));
        } else {
            env::panic_str(TOKEN_NOT_REG);
        }
    }
}

impl SnailSwap {
    /// Record deposit of some number of tokens to this contract.
    /// Fails if account is not registered or if token isn't whitelisted.
    pub(crate) fn internal_deposit(
        &mut self,
        sender_id: &AccountId,
        token_id: &AccountId,
        amount: Balance,
    ) {
        let mut account = self.internal_unwrap_account(sender_id);

        account.deposit(token_id, amount);
        self.internal_save_account(&sender_id, account);
    }

    pub fn internal_unwrap_account(&self, account_id: &AccountId) -> Account {
        self.internal_get_account(account_id)
            .expect(format!("Account [{}] not registered", account_id).as_str())
    }

    pub fn internal_get_account(&self, account_id: &AccountId) -> Option<Account> {
        self.accounts.get(account_id).map(|va| va.into_current())
    }

    pub fn internal_unwrap_or_default_account(&self, account_id: &AccountId) -> Account {
        self.internal_get_account(account_id)
            .unwrap_or_else(|| Account::new(account_id))
    }

    /// Checks that account has enough storage to be stored and saves it into collection.
    /// This should be only place to directly use `self.accounts`.
    pub fn internal_save_account(&mut self, account_id: &AccountId, account: Account) {
        account.assert_storage_usage();
        self.accounts.insert(&account_id, &account.into());
    }

    /// Registers account in deposited amounts with given amount of $NEAR.
    /// If account already exists, adds amount to it.
    /// This should be used when it's known that storage is prepaid.
    pub(crate) fn internal_register_account(&mut self, account_id: &AccountId, amount: Balance) {
        let mut account = self.internal_unwrap_or_default_account(&account_id);
        account.near_amount = account.near_amount.checked_add(amount).unwrap();
        self.internal_save_account(&account_id, account);
    }

    /// storage withdraw
    pub(crate) fn internal_storage_withdraw(
        &mut self,
        account_id: &AccountId,
        amount: Balance,
    ) -> u128 {
        let mut account = self.internal_unwrap_account(&account_id);
        let available = account.storage_available();
        assert!(available > 0, "ERR_NO_STORAGE_CAN_WITHDRAW");
        let mut withdraw_amount = amount;
        if amount == 0 {
            withdraw_amount = available;
        }
        assert!(
            withdraw_amount <= available,
            "ERR_STORAGE_WITHDRAW_TOO_MUCH"
        );
        account.near_amount = account.near_amount.checked_sub(withdraw_amount).unwrap();
        self.internal_save_account(&account_id, account);
        withdraw_amount
    }

    pub(crate) fn internal_send_tokens(
        &self,
        sender_id: &AccountId,
        token_id: &AccountId,
        amount: Balance,
    ) -> Promise {
        ext_fungible_token::ft_transfer(
            sender_id.clone(),
            U128(amount),
            None,
            token_id.clone(),
            1,
            GAS_FOR_FT_TRANSFER,
        )
        .then(ext_self::exchange_callback_post_withdraw(
            token_id.clone(),
            sender_id.clone(),
            U128(amount),
            env::current_account_id(),
            0,
            GAS_FOR_RESOLVE_TRANSFER,
        ))
    }

    /// Returns current balance of given token for given user. If there is nothing recorded, returns 0.
    pub(crate) fn internal_get_deposit(
        &self,
        sender_id: &AccountId,
        token_id: &AccountId,
    ) -> Balance {
        self.internal_get_account(sender_id)
            .and_then(|x| x.get_balance(token_id))
            .unwrap_or(0)
    }

    /// save token to owner account as lostfound, no need to care about storage
    /// only global whitelisted token can be stored in lost-found
    pub(crate) fn internal_lostfound(&mut self, token_id: &AccountId, amount: u128) {
        let mut lostfound = self.internal_unwrap_or_default_account(&self.owner_id);
        lostfound.deposit(token_id, amount);
        self.accounts.insert(&self.owner_id, &lostfound.into());
    }
}

#[near_bindgen]
impl SnailSwap {
    /// Registers given token in the user's account deposit.
    /// Fails if not enough balance on this account to cover storage.e
    //1. if all token get register, then false
    //2. takes needed amount and update account
    //3. refund
    #[payable]
    pub fn register_tokens(&mut self, token_ids: Vec<AccountId>) {
        assert_one_yocto();
        self.assert_contract_running();
        let sender_id = env::predecessor_account_id();
        let mut account = self.internal_unwrap_account(&sender_id);
        account.register(&token_ids);
        self.internal_save_account(&sender_id, account);
    }

    /// Unregister given token from user's account deposit.
    /// Panics if the balance of any given token is non 0.
    #[payable]
    pub fn unregister_tokens(&mut self, token_ids: Vec<AccountId>) {
        assert_one_yocto();
        self.assert_contract_running();
        let sender_id = env::predecessor_account_id();
        let mut account = self.internal_unwrap_account(&sender_id);
        for token_id in token_ids {
            account.unregister(&token_id);
        }
        self.internal_save_account(&sender_id, account);
    }

    /// Withdraws given token from the deposits of given user.
    /// Optional unregister will try to remove record of this token from AccountDeposit for given user.
    /// Unregister will fail if the left over balance is non 0.
    #[payable]
    pub fn withdraw(
        &mut self,
        token_id: AccountId,
        amount: U128,
        unregister: Option<bool>,
    ) -> Promise {
        assert_one_yocto();
        self.assert_contract_running();
        let token_id: AccountId = token_id.into();
        let amount: u128 = amount.into();
        assert!(amount > 0, "{}", ILLEGAL_WITHDRAW_AMOUNT);
        let sender_id = env::predecessor_account_id();
        let mut account = self.internal_unwrap_account(&sender_id);
        // Note: subtraction and deregistration will be reverted if the promise fails.
        account.withdraw(&token_id, amount);
        if unregister == Some(true) {
            account.unregister(&token_id);
        }
        self.internal_save_account(&sender_id, account);
        self.internal_send_tokens(&sender_id, &token_id, amount)
    }
}
