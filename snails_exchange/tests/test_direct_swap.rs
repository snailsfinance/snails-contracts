use near_sdk::json_types::U128;
use near_sdk_sim::{call, to_yocto, ContractAccount, ExecutionResult, UserAccount};

use test_token::ContractContract as TestToken;

use crate::common::utils::*;
pub mod common;

fn pack_action(
    pool_id: u32,
    token_out: &str,
    amount_in: Option<u128>,
    min_amount_out: u128,
) -> String {
    if let Some(amount_in) = amount_in {
        format!(
            "{{\"pool_id\": {}, \"amount_in\": \"{}\", \"token_out\": \"{}\", \"min_amount_out\": \"{}\"}}",
            pool_id,  amount_in, token_out, min_amount_out
        )
    } else {
        format!(
            "{{\"pool_id\": {}, \"token_out\": \"{}\", \"min_amount_out\": \"{}\"}}",
            pool_id, token_out, min_amount_out
        )
    }
}

fn direct_swap(
    user: &UserAccount,
    contract: &ContractAccount<TestToken>,
    action: String,
    amount: u128,
) -> ExecutionResult {
    // {{\"pool_id\": 0, \"token_in\": \"dai\", \"token_out\": \"eth\", \"min_amount_out\": \"1\"}}
    println!("action [{}]", action);
    call!(
        user,
        contract.ft_transfer_call(swap(), amount.into(), None, action),
        deposit = 1
    )
}

#[test]
fn instant_swap_scenario_01() {
    const ONE_DAI: u128 = 1000000000000000000;
    const ONE_USDT: u128 = 1000000;
    const ONE_USDC: u128 = 1000000;
    let (root, _owner, pool, tokens) = setup_three_coin_pool_with_liquidity(
        vec![
            String::from(dai().as_str()),
            String::from(usdt().as_str()),
            String::from(usdc().as_str()),
        ],
        vec![100000 * ONE_DAI, 100000 * ONE_USDT, 100000 * ONE_USDC],
        vec![18u64, 6u64, 6u64],
    );

    let tokens = &tokens;
    let _user = root.create_user(get_accountid_from_string("user"), to_yocto("100"));
    let token_in = &tokens[0];
    let token_out = &tokens[1];

    let new_user = root.create_user(get_accountid_from_string("new_user"), to_yocto("100"));
    call!(
        new_user,
        token_in.mint((new_user.account_id.clone()), U128(to_yocto("10")))
    )
    .assert_success();

    println!("Case 0101: wrong msg");
    let out_come = direct_swap(
        &new_user,
        &token_in,
        "wrong actions".to_string(),
        to_yocto("1"),
    );
    out_come.assert_success();
    assert_eq!(get_error_count(&out_come), 1);
    assert!(get_error_status(&out_come).contains("Illegal msg in ft_transfer_call"));
    assert_eq!(balance_of(&token_in, &new_user.account_id), to_yocto("10"));
    assert_eq!(balance_of(&token_out, &new_user.account_id), to_yocto("0"));

    println!("Case 0102: less then min_amount_out");
    let action = pack_action(0, &token_out.account_id().as_str(), None, to_yocto("1.9"));

    let out_come = direct_swap(&new_user, &token_in, action, to_yocto("1"));
    out_come.assert_success();
    // println!("{:#?}", out_come.promise_results());
    assert_eq!(get_error_count(&out_come), 1);
    assert!(get_error_status(&out_come)
        .contains("Smart contract panicked: panicked at 'ERR_MIN_AMOUNT'"));
    assert!(get_storage_balance(&pool, new_user.account_id()).is_none());
    assert_eq!(balance_of(&token_in, &new_user.account_id), to_yocto("10"));
    assert_eq!(balance_of(&token_out, &new_user.account_id), to_yocto("0"));
}

#[test]
fn instant_swap_scenario_02() {
    const ONE_DAI: u128 = 1000000000000000000;
    const ONE_USDT: u128 = 1000000;
    const ONE_USDC: u128 = 1000000;
    let (root, owner, pool, tokens) = setup_three_coin_pool_with_liquidity(
        vec![
            String::from(dai().as_str()),
            String::from(usdt().as_str()),
            String::from(usdc().as_str()),
        ],
        vec![100000 * ONE_DAI, 100000 * ONE_USDT, 100000 * ONE_USDC],
        vec![18u64, 6u64, 6u64],
    );

    let tokens = &tokens;
    let _user = root.create_user(get_accountid_from_string("user"), to_yocto("100"));
    let token_in = &tokens[0];
    let token_out = &tokens[1];
    let new_user = root.create_user(get_accountid_from_string("new_user"), to_yocto("100"));
    call!(
        new_user,
        token_in.mint((new_user.account_id.clone()), U128(10 * ONE_DAI))
    )
    .assert_success();

    println!("Case 0201: registered user without any deposits and non-registered to token2");
    call!(
        new_user,
        pool.storage_deposit(None, Some(true)),
        deposit = to_yocto("1")
    )
    .assert_success();

    assert_eq!(
        get_storage_balance(&pool, new_user.account_id())
            .unwrap()
            .available
            .0,
        to_yocto("0")
    );

    assert_eq!(
        get_storage_balance(&pool, new_user.account_id())
            .unwrap()
            .total
            .0,
        to_yocto("0.00102")
    );

    // println!("{:#?}", get_storage_balance(&pool, new_user.account_id()).unwrap());
    let action = pack_action(0, &token_out.account_id().as_str(), None, 1);

    let out_come = direct_swap(&new_user, &token_in, action, 1 * ONE_DAI);
    out_come.assert_success();
    println!(
        "after swap owner tokenout {}",
        get_deposits(&pool, owner.account_id())
            .get(&String::from(token_out.account_id().as_str()))
            .unwrap()
            .0
    );
    //println!("swap one logs: {:#?}", get_logs(&out_come));
    //println!("{:#?}", out_come.promise_results());
    assert_eq!(get_error_count(&out_come), 1);
    assert!(get_error_status(&out_come)
        .contains("Smart contract panicked: The account new_user is not registered"));
    //println!("total logs: {:#?}", get_logs(&out_come));
    assert!(get_logs(&out_come)[5]
        .contains("Account new_user has not enough storage. Depositing to owner."));
    assert_eq!(
        get_storage_balance(&pool, new_user.account_id())
            .unwrap()
            .available
            .0,
        to_yocto("0")
    );
    assert_eq!(
        get_storage_balance(&pool, new_user.account_id())
            .unwrap()
            .total
            .0,
        to_yocto("0.00102")
    );

    assert_eq!(balance_of(&token_in, &new_user.account_id), (9 * ONE_DAI));

    assert_eq!(
        get_deposits(&pool, owner.account_id())
            .get(&String::from(token_out.account_id().as_str()))
            .unwrap()
            .0,
        998498
    );
    assert!(get_deposits(&pool, new_user.account_id())
        .get(&String::from(token_in.account_id().as_str()))
        .is_none());
    assert!(get_deposits(&pool, new_user.account_id())
        .get(&String::from(token_out.account_id().as_str()))
        .is_none());

    println!("Case 0202: registered user without any deposits");
    call!(
        new_user,
        token_out.mint((new_user.account_id.clone()), U128(10 * ONE_USDT))
    )
    .assert_success();
    assert_eq!(balance_of(&token_in, &new_user.account_id), (9 * ONE_DAI));
    assert_eq!(
        balance_of(&token_out, &new_user.account_id),
        (10 * ONE_USDT)
    );

    let action = pack_action(0, &token_out.account_id().as_str(), None, 1);
    let out_come = direct_swap(&new_user, &token_in, action, 1 * ONE_DAI);
    out_come.assert_success();
    // println!("{:#?}", out_come.promise_results());
    // println!("total logs: {:#?}", get_logs(&out_come));
    assert_eq!(get_error_count(&out_come), 0);
    assert_eq!(
        get_storage_balance(&pool, new_user.account_id())
            .unwrap()
            .available
            .0,
        0
    );
    assert_eq!(
        get_storage_balance(&pool, new_user.account_id())
            .unwrap()
            .total
            .0,
        to_yocto("0.00102")
    );

    println!("token out {}", balance_of(&token_out, &new_user.account_id));
    assert_eq!(balance_of(&token_in, &new_user.account_id), (8 * ONE_DAI));
    assert!(balance_of(&token_out, &new_user.account_id) > (109 * ONE_USDT / 10));

    println!("Case 0203: registered user with token already deposited");
    call!(
        new_user,
        pool.storage_deposit(None, None),
        deposit = to_yocto("1")
    )
    .assert_success();
    call!(
        new_user,
        token_in.ft_transfer_call((swap()), U128(5 * ONE_DAI), None, "".to_string()),
        deposit = 1
    )
    .assert_success();
    call!(
        new_user,
        token_out.ft_transfer_call((swap()), U128(5 * ONE_USDT), None, "".to_string()),
        deposit = 1
    )
    .assert_success();
    assert_eq!(
        get_deposits(&pool, new_user.account_id())
            .get(&String::from(token_in.account_id().as_str()))
            .unwrap()
            .0,
        (5 * ONE_DAI)
    );
    assert_eq!(
        get_deposits(&pool, new_user.account_id())
            .get(&String::from(token_out.account_id().as_str()))
            .unwrap()
            .0,
        (5 * ONE_USDT)
    );
    let action = pack_action(0, &token_out.account_id().as_str(), None, 1);
    let out_come = direct_swap(&new_user, &token_in, action, 1 * ONE_DAI);
    out_come.assert_success();
    assert_eq!(get_error_count(&out_come), 0);
    assert_eq!(
        get_deposits(&pool, new_user.account_id())
            .get(&String::from(token_in.account_id().as_str()))
            .unwrap()
            .0,
        (5 * ONE_DAI)
    );
    assert_eq!(
        get_deposits(&pool, new_user.account_id())
            .get(&String::from(token_out.account_id().as_str()))
            .unwrap()
            .0,
        (5 * ONE_USDT)
    );
    assert_eq!(balance_of(&token_in, &new_user.account_id), (2 * ONE_DAI));
    println!(
        "balance token_out {}",
        balance_of(&token_out, &new_user.account_id)
    );
    //6.9 usdt
    assert!(balance_of(&token_out, &new_user.account_id) > (69 * ONE_USDT / 10));

    println!("Case 0204: deposit token is not in action");
    let token_unkown = test_token(&root, get_accountid_from_string("unknown"), vec![swap()]);
    call!(
        new_user,
        token_unkown.mint(new_user.account_id.clone(), U128(10 * ONE_USDC))
    )
    .assert_success();

    let action = pack_action(0, &token_out.account_id().as_str(), None, 1);
    let out_come = direct_swap(&new_user, &token_unkown, action, 1 * ONE_USDC);
    out_come.assert_success();
    println!("{}", get_error_status(&out_come));
    assert_eq!(get_error_count(&out_come), 1);
    assert!(get_error_status(&out_come).contains("ERR_MISSING_TOKEN"));
    assert_eq!(
        get_deposits(&pool, new_user.account_id())
            .get(&String::from(token_in.account_id().as_str()))
            .unwrap()
            .0,
        5 * ONE_DAI
    );
    assert_eq!(
        get_deposits(&pool, new_user.account_id())
            .get(&String::from(token_out.account_id().as_str()))
            .unwrap()
            .0,
        5 * ONE_USDT
    );
}

#[test]
fn instant_swap_scenario_04() {
    const ONE_DAI: u128 = 1000000000000000000;
    const ONE_USDT: u128 = 1000000;
    const ONE_USDC: u128 = 1000000;
    let (root, owner, pool, tokens) = setup_three_coin_pool_with_liquidity(
        vec![
            String::from(dai().as_str()),
            String::from(usdt().as_str()),
            String::from(usdc().as_str()),
        ],
        vec![100000 * ONE_DAI, 100000 * ONE_USDT, 100000 * ONE_USDC],
        vec![18u64, 6u64, 6u64],
    );

    let tokens = &tokens;
    let user = root.create_user(get_accountid_from_string("user"), to_yocto("100"));
    let token_in = &tokens[0];
    let token_out = &tokens[1];
    call!(user, token_in.mint(user.account_id(), U128(10 * ONE_DAI))).assert_success();

    println!("Case 0401: non-registered user stable swap but not registered in token2");
    let action = pack_action(0, &tokens[1].account_id().as_str(), None, 1);

    let out_come = direct_swap(&user, &tokens[0], action, 1 * ONE_DAI);
    out_come.assert_success();
    assert_eq!(get_error_count(&out_come), 1);
    println!(
        "out_come {:?}",
        out_come.promise_errors()[0].as_ref().unwrap().status()
    );
    assert!(get_error_status(&out_come)
        .contains("Smart contract panicked: The account user is not registered"));
    assert!(get_storage_balance(&pool, user.account_id()).is_none());
    assert_eq!(balance_of(&tokens[0], &user.account_id), 9 * ONE_DAI);

    //save to owner account
    assert_eq!(
        get_deposits(&pool, owner.account_id())
            .get(&String::from(token_out.account_id().as_str()))
            .unwrap()
            .0,
        998498
    );

    println!("Case 0402: non-registered user stable swap");
    call!(
        user,
        token_out.storage_deposit(None, None),
        deposit = to_yocto("1")
    )
    .assert_success();

    let action = pack_action(0, &tokens[1].account_id().as_str(), None, 1);

    let out_come = direct_swap(&user, &tokens[0], action, 1 * ONE_DAI);
    out_come.assert_success();
    assert_eq!(get_error_count(&out_come), 0);
    assert!(get_storage_balance(&pool, user.account_id()).is_none());
    assert_eq!(balance_of(&token_in, &user.account_id), 8 * ONE_DAI);
    assert_eq!(balance_of(&token_out, &user.account_id), 996999);
}
