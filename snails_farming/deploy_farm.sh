near create-account snailfarm.snails_fi.testnet --masterAccount snails_fi.testnet --initialBalance 25
sleep 5
 near deploy --wasmFile target/wasm32-unknown-unknown/release/snails_farming.wasm --accountId snailfarm.snails_fi.testnet

 near call snailfarm.snails_fi.testnet new '{"owner_id":"snails_fi.testnet"}' --accountId snails_fi.testnet

#create farm
 near call snailfarm.snails_fi.testnet  create_simple_farm  '{"terms": {"seed_id": "stableswap.snails_fi.testnet@0", "reward_token": "snailcoin.snails_fi.testnet", "start_at": 0, "reward_per_session": "380517500", "session_interval": 60}, "min_deposit":"0"}' --account_id=snails_fi.testnet --amount 0.01

#farmid: stableswap.snails_fi.testnet@0#0
near call snailfarm.snails_fi.testnet change_reward_per_session '{"farm_id":"stableswap.snails_fi.testnet@0#0","reward_per_session":"3805175000000"}' --account_id=snails_fi.testnet

# deposit reward token into the farm
near call snailcoin.snails_fi.testnet storage_deposit '{"account_id": "snailfarm.snails_fi.testnet", "registration_only": true}' --account_id=snails_fi.testnet --amount=1
near call snailcoin.snails_fi.testnet ft_transfer_call '{"receiver_id": "snailfarm.snails_fi.testnet", "amount": "164383560000000000", "msg": "stableswap.snails_fi.testnet@0#0"}' --account_id=snailcoin.snails_fi.testnet --amount=0.000000000000000000000001 --gas=100000000000000

near view snailcoin.snails_fi.testnet ft_balance_of '{"account_id":"snailfarm.snails_fi.testnet"}'

#stake
# if needed, register user to the farming
near call snailfarm.snails_fi.testnet storage_deposit '{"account_id": "snails_fi.testnet", "registration_only": true}' --account_id=snails_fi.testnet --amount=1

# if needed, register farming contract to seed token
near call stableswap.snails_fi.testnet mft_register '{"token_id":":0", "account_id": "snailfarm.snails_fi.testnet"}' --account_id=snails_fi.testnet --amount=0.01

# staking
near call stableswap.snails_fi.testnet mft_transfer_call '{"receiver_id": "snailfarm.snails_fi.testnet", "token_id":":0", "amount": "100", "msg": ""}' --account_id=snails_fi.testnet --amount=0.000000000000000000000001  --gas=300000000000000

near view stableswap.snails_fi.testnet mft_balance_of '{"token_id":":0","account_id":"snailfarm.snails_fi.testnet"}'

near view snailfarm.snails_fi.testnet get_unclaimed_reward '{"account_id": "snails_fi.testnet", "farm_id": "stableswap.snails_fi.testnet@0#0"}'
near view snailfarm.snails_fi.testnet get_user_rps '{"account_id": "snails_fi.testnet", "farm_id": "stableswap.snails_fi.testnet@0#0"}'

near view snailfarm.snails_fi.testnet get_farm '{"farm_id":"stableswap.snails_fi.testnet@0#0"}'
near view snailfarm.snails_fi.testnet get_seed_info '{ "seed_id":"stableswap.snails_fi.testnet@0"}'
