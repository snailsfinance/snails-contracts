use std::collections::HashMap;
use std::convert::TryFrom;

use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::{from_value, Value};
use near_sdk::AccountId;
use near_sdk_sim::{
    call, deploy, init_simulator, to_yocto, view, ContractAccount, ExecutionResult, UserAccount,
};

near_sdk_sim::lazy_static_include::lazy_static_include_bytes! {
    TEST_TOKEN_WASM_BYTES => "../res/test_token.wasm",
    EXCHANGE_WASM_BYTES => "../res/snails_exchange.wasm",
}
use snails_exchange::{Fees, PoolInfo, SnailSwapContract as Exchange};
use test_token::ContractContract as TestToken;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct RefStorageState {
    pub deposit: U128,
    pub usage: U128,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct StorageBalance {
    pub total: U128,
    pub available: U128,
}

pub fn show_promises(r: &ExecutionResult) {
    for promise in r.promise_results() {
        println!("{:?}", promise);
    }
}

pub fn get_logs(r: &ExecutionResult) -> Vec<String> {
    let mut logs: Vec<String> = vec![];
    r.promise_results()
        .iter()
        .map(|ex| {
            ex.as_ref()
                .unwrap()
                .logs()
                .iter()
                .map(|x| logs.push(x.clone()))
                .for_each(drop)
        })
        .for_each(drop);
    logs
}

pub fn get_error_count(r: &ExecutionResult) -> u32 {
    r.promise_errors().len() as u32
}

pub fn get_error_status(r: &ExecutionResult) -> String {
    format!("{:?}", r.promise_errors()[0].as_ref().unwrap().status())
}

pub fn test_token(
    root: &UserAccount,
    token_id: AccountId,
    accounts_to_register: Vec<AccountId>,
) -> ContractAccount<TestToken> {
    let t = deploy!(
        contract: TestToken,
        contract_id: token_id,
        bytes: &TEST_TOKEN_WASM_BYTES,
        signer_account: root
    );
    call!(root, t.new()).assert_success();
    call!(
        root,
        t.mint(root.account_id.clone(), to_yocto("1000000000").into())
    )
    .assert_success();
    for account_id in accounts_to_register {
        call!(
            root,
            t.storage_deposit(Some(account_id), None),
            deposit = to_yocto("1")
        )
        .assert_success();
    }
    t
}

//*****************************
// View functions
//*****************************

/// tell a user if he has registered to given ft token
pub fn is_register_to_token(token: &ContractAccount<TestToken>, account_id: AccountId) -> bool {
    let sb = view!(token.storage_balance_of(account_id)).unwrap_json_value();
    if let Value::Null = sb {
        false
    } else {
        true
    }
}

/// get user's ft balance of given token
pub fn balance_of(token: &ContractAccount<TestToken>, account_id: &AccountId) -> u128 {
    view!(token.ft_balance_of(account_id.clone()))
        .unwrap_json::<U128>()
        .0
}

/// get stableswap's version
pub fn get_version(pool: &ContractAccount<Exchange>) -> String {
    view!(pool.version()).unwrap_json::<String>()
}

/// get stableswap's pool count
pub fn get_num_of_pools(pool: &ContractAccount<Exchange>) -> u64 {
    view!(pool.get_number_of_pools()).unwrap_json::<u64>()
}

/// get stableswap's all pool info
pub fn get_pools(pool: &ContractAccount<Exchange>) -> Vec<PoolInfo> {
    view!(pool.get_pools(0, 100)).unwrap_json::<Vec<PoolInfo>>()
}

/// get stableswap's pool info
pub fn get_pool(pool: &ContractAccount<Exchange>, pool_id: u64) -> PoolInfo {
    view!(pool.get_pool(pool_id)).unwrap_json::<PoolInfo>()
}

pub fn get_deposits(
    pool: &ContractAccount<Exchange>,
    account_id: AccountId,
) -> HashMap<String, U128> {
    view!(pool.get_deposits(account_id)).unwrap_json::<HashMap<String, U128>>()
}

pub fn get_storage_balance(
    pool: &ContractAccount<Exchange>,
    account_id: AccountId,
) -> Option<StorageBalance> {
    let sb = view!(pool.storage_balance_of(account_id)).unwrap_json_value();
    if let Value::Null = sb {
        None
    } else {
        // near_sdk::serde_json::
        let ret: StorageBalance = from_value(sb).unwrap();
        Some(ret)
    }
}

pub fn mft_balance_of(
    pool: &ContractAccount<Exchange>,
    token_or_pool: &str,
    account_id: &AccountId,
) -> u128 {
    view!(pool.mft_balance_of(token_or_pool.to_string(), account_id.clone()))
        .unwrap_json::<U128>()
        .0
}

pub fn mft_total_supply(pool: &ContractAccount<Exchange>, token_or_pool: &str) -> u128 {
    view!(pool.mft_total_supply(token_or_pool.to_string()))
        .unwrap_json::<U128>()
        .0
}

pub fn get_accountid_from_string(value: &str) -> AccountId {
    AccountId::try_from(String::from(value)).unwrap()
}
//************************************

pub fn dai() -> AccountId {
    get_accountid_from_string("dai001")
}

pub fn eth() -> AccountId {
    get_accountid_from_string("eth002")
}

pub fn usdt() -> AccountId {
    get_accountid_from_string("usdt")
}

pub fn usdc() -> AccountId {
    get_accountid_from_string("usdc")
}

pub fn swap() -> AccountId {
    get_accountid_from_string("swap")
}

fn setup_fee() -> Fees {
    //initial A = 100, target = 500，time可以设计成2周。就是2周A线性过度到500
    //admin_trade_fee = 0.5 , admin_withdraw_fee = 0.4, trade_fee = 3/1000, withdraw_fee = 4/1000

    let admin_trade_fee_numerator: u64 = 50;
    let admin_trade_fee_denominator: u64 = 100;
    let admin_withdraw_fee_numerator: u64 = 40;
    let admin_withdraw_fee_denominator: u64 = 100;
    let trade_fee_numerator: u64 = 3;
    let trade_fee_denominator: u64 = 1000;
    let withdraw_fee_numerator: u64 = 4;
    let withdraw_fee_denominator: u64 = 1000;

    Fees {
        admin_trade_fee_numerator,
        admin_trade_fee_denominator,
        admin_withdraw_fee_numerator,
        admin_withdraw_fee_denominator,
        trade_fee_numerator,
        trade_fee_denominator,
        withdraw_fee_numerator,
        withdraw_fee_denominator,
    }
}

pub fn setup_two_coin_pool_with_liquidity() -> (
    UserAccount,
    UserAccount,
    ContractAccount<Exchange>,
    ContractAccount<TestToken>,
    ContractAccount<TestToken>,
    ContractAccount<TestToken>,
) {
    let root = init_simulator(None);
    let owner = root.create_user(get_accountid_from_string("owner"), to_yocto("100"));
    let pool = deploy!(
        contract: Exchange,
        contract_id: swap(),
        bytes: &EXCHANGE_WASM_BYTES,
        signer_account: root,
        init_method: new(get_accountid_from_string("owner"))
    );
    let token1 = test_token(&root, dai(), vec![swap()]);
    let token2 = test_token(&root, eth(), vec![swap()]);
    let token3 = test_token(&root, usdt(), vec![swap()]);

    let initial_amp_factor: u64 = 100;
    let target_amp_factor: u64 = 500;
    let start_ramp_ts: u64 = 0;
    let stop_ramp_ts: u64 = 0;
    let fees: Fees = setup_fee();
    call!(
        owner,
        pool.add_simple_pool(
            vec![dai(), eth()],
            vec![18u64, 6u64],
            initial_amp_factor,
            target_amp_factor,
            start_ramp_ts,
            stop_ramp_ts,
            fees.clone()
        ),
        deposit = to_yocto("1")
    )
    .assert_success();
    call!(
        owner,
        pool.add_simple_pool(
            vec![eth(), usdt()],
            vec![6u64, 6u64],
            initial_amp_factor,
            target_amp_factor,
            start_ramp_ts,
            stop_ramp_ts,
            fees.clone()
        ),
        deposit = to_yocto("1")
    )
    .assert_success();
    call!(
        owner,
        pool.add_simple_pool(
            vec![usdt(), dai()],
            vec![6u64, 18u64],
            initial_amp_factor,
            target_amp_factor,
            start_ramp_ts,
            stop_ramp_ts,
            fees.clone()
        ),
        deposit = to_yocto("1")
    )
    .assert_success();

    call!(
        root,
        pool.storage_deposit(None, None),
        deposit = to_yocto("1")
    )
    .assert_success();

    call!(
        owner,
        pool.storage_deposit(None, None),
        deposit = to_yocto("1")
    )
    .assert_success();

    call!(
        root,
        token1.ft_transfer_call(swap(), to_yocto("105").into(), None, "".to_string()),
        deposit = 1
    )
    .assert_success();
    call!(
        root,
        token2.ft_transfer_call(swap(), to_yocto("110").into(), None, "".to_string()),
        deposit = 1
    )
    .assert_success();
    call!(
        root,
        token3.ft_transfer_call(swap(), to_yocto("110").into(), None, "".to_string()),
        deposit = 1
    )
    .assert_success();
    call!(
        root,
        pool.add_liquidity(0, vec![U128(to_yocto("10")), U128(to_yocto("20"))], None),
        deposit = to_yocto("0.0007")
    )
    .assert_success();
    call!(
        root,
        pool.add_liquidity(1, vec![U128(to_yocto("20")), U128(to_yocto("10"))], None),
        deposit = to_yocto("0.0007")
    )
    .assert_success();
    call!(
        root,
        pool.add_liquidity(2, vec![U128(to_yocto("10")), U128(to_yocto("10"))], None),
        deposit = to_yocto("0.0007")
    )
    .assert_success();
    (root, owner, pool, token1, token2, token3)
}

pub fn setup_three_coin_pool_with_liquidity(
    tokens: Vec<String>,
    amounts: Vec<u128>,
    decimals: Vec<u64>,
) -> (
    UserAccount,
    UserAccount,
    ContractAccount<Exchange>,
    Vec<ContractAccount<TestToken>>,
) {
    let root = init_simulator(None);
    let owner = root.create_user(get_accountid_from_string("owner"), to_yocto("100"));
    let pool = deploy!(
        contract: Exchange,
        contract_id: swap(),
        bytes: &EXCHANGE_WASM_BYTES,
        signer_account: root,
        init_method: new(owner.account_id())
    );

    let mut token_contracts: Vec<ContractAccount<TestToken>> = vec![];
    for token_name in &tokens {
        token_contracts.push(test_token(
            &root,
            get_accountid_from_string(token_name),
            vec![swap()],
        ));
    }
    let initial_amp_factor: u64 = 100;
    let target_amp_factor: u64 = 500;
    let start_ramp_ts: u64 = 0;
    let stop_ramp_ts: u64 = 0;
    let fees: Fees = setup_fee();
    call!(
        owner,
        pool.add_simple_pool(
            (&token_contracts)
                .into_iter()
                .map(|x| x.account_id())
                .collect(),
            decimals,
            initial_amp_factor,
            target_amp_factor,
            start_ramp_ts,
            stop_ramp_ts,
            fees.clone()
        ),
        deposit = to_yocto("1")
    )
    .assert_success();

    call!(
        root,
        pool.storage_deposit(None, None),
        deposit = to_yocto("1")
    )
    .assert_success();

    call!(
        owner,
        pool.storage_deposit(None, None),
        deposit = to_yocto("1")
    )
    .assert_success();

    for (idx, amount) in amounts.clone().into_iter().enumerate() {
        let c = token_contracts.get(idx).unwrap();
        call!(
            root,
            c.ft_transfer_call(pool.account_id(), U128(amount), None, "".to_string()),
            deposit = 1
        )
        .assert_success();
    }

    call!(
        root,
        pool.add_liquidity(
            0,
            amounts.into_iter().map(|x| U128(x)).collect(),
            Some(U128(1))
        ),
        deposit = to_yocto("0.0086")
    )
    .assert_success();

    (root, owner, pool, token_contracts)
}

pub fn mint_and_deposit_token(
    user: &UserAccount,
    token: &ContractAccount<TestToken>,
    ex: &ContractAccount<Exchange>,
    amount: u128,
) {
    call!(user, token.mint(user.account_id(), U128(amount))).assert_success();
    call!(
        user,
        ex.storage_deposit(None, None),
        deposit = to_yocto("1")
    )
    .assert_success();
    call!(
        user,
        token.ft_transfer_call(ex.account_id(), U128(amount), None, "".to_string()),
        deposit = 1
    )
    .assert_success();
}

pub fn setup_exchange(root: &UserAccount) -> (UserAccount, ContractAccount<Exchange>) {
    let owner = root.create_user(get_accountid_from_string("owner"), to_yocto("100"));
    let pool = deploy!(
        contract: Exchange,
        contract_id: swap(),
        bytes: &EXCHANGE_WASM_BYTES,
        signer_account: root,
        init_method: new(get_accountid_from_string("owner"))
    );
    (owner, pool)
}

pub fn deposit_token(
    user: &UserAccount,
    ex: &ContractAccount<Exchange>,
    tokens: Vec<&ContractAccount<TestToken>>,
    amounts: Vec<u128>,
) {
    for (idx, token) in tokens.into_iter().enumerate() {
        call!(
            user,
            ex.storage_deposit(None, None),
            deposit = to_yocto("0.1")
        )
        .assert_success();
        call!(
            user,
            token.ft_transfer_call(ex.account_id(), U128(amounts[idx]), None, "".to_string()),
            deposit = 1
        )
        .assert_success();
    }
}
