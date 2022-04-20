//! Error types
/// #[derive(BorshSerialize, BorshDeserialize)]

pub const LP_NOT_REGISTERED: &str = "LP not registered";
pub const LP_ALREADY_REGISTERED: &str = "LP already registered";

// Accounts.

pub const TOKEN_NOT_REG: &str = "Token not registered";
pub const NON_ZERO_TOKEN_BALANCE: &str = "Non-zero token balance";
pub const CALLBACK_POST_WITHDRAW_INVALID: &str = "Expected 1 promise result from withdraw";
// pub const ERR26_ACCESS_KEY_NOT_ALLOWED: &str = "E26: access key not allowed";
pub const WRONG_MSG_FORMAT: &str = "Illegal msg in ft_transfer_call";
pub const ILLEGAL_WITHDRAW_AMOUNT: &str = "Illegal withdraw amount";

// Liquidity operations.

pub const ZERO_SHARES: &str = "Minting zero shares";
pub const TRANSFER_TO_SELF: &str = "Transfer to self";
// Action result.

// Contract Level
pub const CONTRACT_PAUSED: &str = "Contract paused";
