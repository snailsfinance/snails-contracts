use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{serde_json, PromiseOrValue};

use crate::*;

/// Message parameters to receive via token function call.
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
//#[serde(tag = "type")]
#[serde(untagged)]
enum TokenReceiverMessage {
    /// Alternative to deposit + execute actions call.
    ///
    Swap {
        /// Pool which should be used for swapping.
        pool_id: u64,
        /// Token to swap into.
        token_out: AccountId,
        /// Required minimum amount of token_out.
        min_amount_out: U128,
    },
}

impl SnailSwap {
    fn direct_swap(
        &mut self,
        pool_id: u64,
        token_in: &AccountId,
        token_out: &AccountId,
        amount_in: Balance,
        min_amount_out: Balance,
    ) -> Balance {
        let amount_out = self.swap_core(pool_id, token_in, amount_in, token_out, min_amount_out);

        amount_out.into()
    }
}
#[near_bindgen]
impl FungibleTokenReceiver for SnailSwap {
    /// Callback on receiving tokens by this contract.
    #[allow(unreachable_code)]
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.assert_contract_running();
        let token_in = env::predecessor_account_id();

        env::log_str(
            format!(
                "Receive ft token {:?} from {}. msg [{}]",
                amount,
                env::predecessor_account_id(),
                msg
            )
            .as_str(),
        );

        if msg.is_empty() {
            // Simple deposit.
            self.internal_deposit(&sender_id, &token_in, amount.into());
            PromiseOrValue::Value(U128(0))
        } else {
            // direct swap
            let message =
                serde_json::from_str::<TokenReceiverMessage>(&msg).expect(WRONG_MSG_FORMAT);
            match message {
                TokenReceiverMessage::Swap {
                    pool_id,
                    token_out,
                    min_amount_out,
                } => {
                    let amount_out = self.direct_swap(
                        pool_id,
                        &token_in,
                        &token_out,
                        amount.0,
                        min_amount_out.0,
                    );

                    env::log_str(format!("Direct swap from sender {} pool {} token_in {} amount {} for token_out {} min_amount {}  ", 
                    pool_id,sender_id,token_in,amount.0,token_out,min_amount_out.0
                ).as_str());

                    self.internal_send_tokens(&sender_id, &token_out, amount_out);
                    // Even if send tokens fails, we don't return funds back to sender.
                    PromiseOrValue::Value(U128(0))
                }
            }
        }
    }
}
