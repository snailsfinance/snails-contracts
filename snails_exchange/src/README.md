# snails-contracts/snails_exchange/src

All exchange contract sources are within this directory.

# snails_exchange

## Interface Structure

### Exchange core structure

```
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
```

### Stable pool structure

```
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
```

### Account structure

```
/// Account deposits information and storage cost.
#[derive(BorshSerialize, BorshDeserialize)]
pub struct Account {
    /// Native NEAR amount sent to the exchange.
    /// Used for storage right now, but in future can be used for trading as well.
    pub near_amount: Balance,
    pub tokens: UnorderedMap<AccountId, Balance>,
    pub storage_used: StorageUsage,
}
```

## Interface methods

### view functions

```
/// Returns number of pools.
pub fn get_number_of_pools(&self) -> u64

/// Returns list of pools of given length from given start index.
pub fn get_pools(&self, from_index: u64, limit: u64) -> Vec<PoolInfo>

/// Returns information about specified pool.
pub fn get_pool(&self, pool_id: u64) -> PoolInfo

/// Return total fee of the given pool.
pub fn get_pool_fee(&self, pool_id: u64) -> Vec<u128>

pub fn get_pool_admin_fee(&self, pool_id: u64) -> Vec<u128>

/// Returns number of shares given account has in given pool.
pub fn get_pool_shares(&self, pool_id: u64, account_id: AccountId) -> U128

pub fn pool_total_supply(&self, pool_id: u64) -> Balance

/// returns all pools we have
pub fn pool_len(&self) -> u64

/// Returns total number of shares in the given pool.
pub fn get_pool_total_shares(&self, pool_id: u64) -> U128

/// Returns balances of the deposits for given user outside of any pools.
/// Returns empty list if no tokens deposited.
pub fn get_deposits(&self, account_id: AccountId) -> HashMap<AccountId, U128>

/// Returns balance of the deposit for given user outside of any pools.
pub fn get_deposit(&self, account_id: AccountId, token_id: AccountId) -> U128

/// Given specific pool, returns amount of token_out recevied swapping amount_in of ///token_in.
pub fn get_return(&self,pool_id: u64,token_in: AccountId,amount_in: U128,
        token_out: AccountId,) -> U128
        
pub fn get_virtual_price(&self, pool_id: u64) -> U128

pub fn get_amp_factor(&self, pool_id: u64) -> U128

pub fn fees_info(&self, pool_id: u64) -> Fees

pub fn try_remove_liquidity_one_coin(&self,pool_id: u64,token_out: &AccountId,
        remove_lp_amount: U128,) -> U128
      
pub fn try_remove_liquidity_imbalance(&self,pool_id: u64,
        remove_coin_amount: Vec<U128>,) -> u128
    
pub fn try_remove_liquidity(&self, pool_id: u64, shares: U128) -> Vec<U128>

pub fn try_add_liquidity(&self, pool_id: u64, deposit_amounts: Vec<U128>) -> U128   
    
```

### Storage functions

User of swap contract should storage deposit first to create their accounts, which contain their deposit tokens info.

```
/// Registration only setups the account but doesn't leave space for tokens.
#[payable]
fn storage_deposit(&mut self,account_id: Option<AccountId>,
                registration_only: Option<bool>,) -> StorageBalance

#[payable]
fn storage_withdraw(&mut self, amount: Option<U128>) -> StorageBalance

#[allow(unused_variables)]
#[payable]
fn storage_unregister(&mut self, force: Option<bool>) -> bool

fn storage_balance_bounds(&self) -> StorageBalanceBounds 

fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance>     
```

### Account functions

User of swap contract should deposit tokens into the account and then could swap and add liquidity.

```
/// Registers given token in the user's account deposit.
/// Fails if not enough balance on this account to cover storage.e
//1. if all token get register, then false
//2. takes needed amount and update account
//3. refund
#[payable]
pub fn register_tokens(&mut self, token_ids: Vec<AccountId>)

/// Unregister given token from user's account deposit.
/// Panics if the balance of any given token is non 0.
#[payable]
pub fn unregister_tokens(&mut self, token_ids: Vec<AccountId>)

/// Withdraws given token from the deposits of given user.
/// Optional unregister will try to remove record of this token from AccountDeposit for /// given user.
/// Unregister will fail if the left over balance is non 0.
#[payable]
pub fn withdraw(&mut self,token_id: AccountId,amount: U128,
				unregister: Option<bool>,) -> Promise				
```

### User functions

```
///1. simple deposit: deposit token to the swap contract by transfer_call from token contract with empty msg

///2. direct swap: direct swap without deposit token by transfer_call from token contract with msg format as ""{\"pool_id\":0, \"token_out\": \"usdt.snails_fi.testnet\", \"min_amount_out\": \"1\"}""

impl FungibleTokenReceiver for SnailSwap {
fn ft_on_transfer(&mut self,sender_id: AccountId,amount: U128,
        			msg: String,) -> PromiseOrValue<U128>
        

/// Add liquidity from already deposited amounts to given pool.
#[payable]
pub fn add_liquidity(&mut self,pool_id: u64,tokens_amount: Vec<U128>,
                    min_mint_amount: Option<U128>,) -> Balance
                    
/// Remove liquidity from the pool into general pool of liquidity.
#[payable]
pub fn remove_liquidity(&mut self, pool_id: u64, shares: U128, min_amounts: Vec<U128>)

/// Remove liquidity from the pool into general pool of liquidity.
#[payable]
pub fn remove_liquidity_imbalance(&mut self,pool_id: u64,remove_coin_amount: 
                                Vec<U128>,max_amount: Option<U128>,) 
                    
#[payable]
pub fn remove_liquidity_one_coin(&mut self,pool_id: u64,token_out: AccountId,
                                remove_lp_amount: U128,min_amount: U128,) 
                    
#[payable]
pub fn swap(&mut self,pool_id: u64,token_in: AccountId,amount_in: U128,
                token_out: AccountId,minimum_amount_out: U128,) -> U128
                



/// Returns the balance of the given account. 
///If the account doesn't exist will return `"0"`.
pub fn mft_balance_of(&self, token_id: String, account_id: AccountId) -> U128
                
```



### LP token functions

```
/// Returns the balance of the given account. 
///If the account doesn't exist will return `"0"`.
pub fn mft_balance_of(&self, token_id: String, account_id: AccountId) -> U128

/// Returns the total supply of the given token, if the token is one of the pools.
/// If token references external token - fails with unimplemented.
pub fn mft_total_supply(&self, token_id: String) -> U128 

/// Register LP token of given pool for given account.
/// Fails if token_id is not a pool.
#[payable]
pub fn mft_register(&mut self, token_id: String, account_id: AccountId)

pub fn is_lp_token_registered(&self, token_id: String, account_id: AccountId) -> bool

/// Transfer LP tokens.
#[payable]
pub fn mft_transfer(&mut self,token_id: String,receiver_id: AccountId,
                    amount: U128,memo: Option<String>,)
                    
#[payable]
pub fn mft_transfer_call(&mut self,token_id: String,receiver_id: AccountId,
            amount: U128,memo: Option<String>,msg: String,) -> PromiseOrValue<U128>
            
pub fn mft_metadata(&self, token_id: String) -> FungibleTokenMetadata          
```

### Manage exchanges

```
pub fn change_fees_setting(&mut self, pool_id: u64, fees: Fees)

pub fn set_amp_params(&mut self,pool_id: u64,initial_amp_factor: u64,
                        target_amp_factor: u64,stop_ramp_ts: u64,)
```



### Owner methods

```
/// Change state of contract, Only can be called by owner.
#[payable]
pub fn change_state(&mut self, state: RunningState)
```

