
near call stnear.snails_fi.testnet storage_deposit '{"account_id":"stableswap.snails_fi.testnet"}' --accountId stableswap.snails_fi.testnet --amount 0.01
near call wnear.snails_fi.testnet storage_deposit '{"account_id":"stableswap.snails_fi.testnet"}' --accountId stableswap.snails_fi.testnet --amount 0.01


near call stableswap.snails_fi.testnet add_simple_pool '{"tokens":["wnear.snails_fi.testnet","stnear.snails_fi.testnet"],  "decimals":[24,24], "initial_amp_factor":150,"target_amp_factor":150,"start_ramp_ts":1638892389,"stop_ramp_ts":1641484265,"fees":{"admin_trade_fee_numerator":5000000000,"admin_trade_fee_denominator":10000000000,"admin_withdraw_fee_numerator":5000000000,"admin_withdraw_fee_denominator":10000000000,"trade_fee_numerator":10000000,"trade_fee_denominator":10000000000,"withdraw_fee_numerator":0,"withdraw_fee_denominator":10000000000} }' --accountId stableswap.snails_fi.testnet --amount 0.01

near call stableswap.snails_fi.testnet register_tokens '{"token_ids":["wnear.snails_fi.testnet","stnear.snails_fi.testnet"]}' --accountId snails_fi.testnet --amount 0.000000000000000000000001 --gas=300000000000000

#deposit for add liquidity
near call wnear.snails_fi.testnet ft_transfer_call '{"receiver_id":"stableswap.snails_fi.testnet", "amount":"3000000000000000000","msg":""}' --accountId snails_fi.testnet --amount 0.000000000000000000000001 --gas=300000000000000

near call stnear.snails_fi.testnet ft_transfer_call '{"receiver_id":"stableswap.snails_fi.testnet", "amount":"300000000000000000","msg":""}' --accountId snails_fi.testnet --amount 0.000000000000000000000001 --gas=300000000000000


near call stableswap.snails_fi.testnet add_liquidity '{"pool_id":1, "tokens_amount":["3000000000000","3000000000000"],"min_mint_amount":"0"}'  --accountId snails_fi.testnet --amount 0.000000000000000000000001

#deposit for swap

near call stableswap.snails_fi.testnet swap '{"pool_id":1,"token_in":"stnear.snails_fi.testnet","amount_in":"100","token_out":"wnear.snails_fi.testnet","minimum_amount_out":"0"}' --accountId snails_fi.testnet --amount 0.000000000000000000000001


near call stableswap.snails_fi.testnet remove_liquidity '{"pool_id":1, "shares":"1000","min_amounts":["0","0"]}'  --accountId snails_fi.testnet --amount 0.000000000000000000000001

near view stableswap.snails_fi.testnet mft_metadata '{"token_id":":1"}'
near view stableswap.snails_fi.testnet get_pool_admin_fee '{"pool_id":0}'
near view stableswap.snails_fi.testnet get_pool '{"pool_id":0}'
near view stableswap.snails_fi.testnet fees_info '{"pool_id":0}'
near view stableswap.snails_fi.testnet get_virtual_price '{"pool_id":1}'
near view stableswap.snails_fi.testnet get_amp_factor '{"pool_id":0}'
near call stableswap.snails_fi.testnet change_fees_setting '{"pool_id":0, "fees":{"admin_trade_fee_numerator":50,"admin_trade_fee_denominator":100,"admin_withdraw_fee_numerator":40,"admin_withdraw_fee_denominator":100,"trade_fee_numerator":3,"trade_fee_denominator":1000,"withdraw_fee_numerator":0,"withdraw_fee_denominator":1000}  }' --accountId stableswap.snails_fi.testnet

near call stableswap.snails_fi.testnet set_amp_params  '{"pool_id":0,"initial_amp_factor":100,"target_amp_factor":200,"stop_ramp_ts":1640182177}' --accountId snails_fi.testnet

near view stableswap.snails_fi.testnet try_remove_liquidity_one_coin '{"pool_id":0,"token_out":"usdc.snails_fi.testnet","remove_lp_amount":"10000000000000000000"}'

near view stableswap.snails_fi.testnet try_remove_liquidity_imbalance '{"pool_id":0,"token_out":"usdc.snails_fi.testnet","remove_lp_amount":"10000000000000000000","remove_coin_amount":["100000000000","300000000000","20000000000"]}'

near view stableswap.snails_fi.testnet try_remove_liquidity '{"pool_id":0,"shares":"1000000000000000000000"}'

near view stableswap.snails_fi.testnet try_add_liquidity '{"pool_id":0,"deposit_amounts":["100000000000","300000000000","20000000000"]}'
