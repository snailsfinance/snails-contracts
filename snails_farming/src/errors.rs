// Storage errors //
pub const STORAGE_UNREGISTER_REWARDS_NOT_EMPTY: &str = "Still has rewards when unregister";
pub const STORAGE_UNREGISTER_SEED_NOT_EMPTY: &str = "Still has staked seed when unregister";
pub const ERR14_ACC_ALREADY_REGISTERED: &str = "Account already registered";

// Reward errors //
pub const TOKEN_NOT_REG: &str = "Token not registered";
pub const NOT_ENOUGH_TOKENS: &str = "Not enough tokens in deposit";

pub const CALLBACK_POST_WITHDRAW_INVALID: &str = "Expected 1 promise result from withdraw";

// Seed errors //
pub const SEED_NOT_EXIST: &str = "Seed not exist";
pub const NOT_ENOUGH_SEED: &str = "Not enough amount of seed";
pub const INVALID_SEED_ID: &str = "Invalid seed id";
pub const BELOW_MIN_SEED_DEPOSITED: &str = "Below min_deposit of this seed";
pub const ILLEGAL_TOKEN_ID: &str = "Illegal token_id in mft_transfer_call";

// farm errors //
pub const FARM_NOT_EXIST: &str = "Farm not exist";
pub const INVALID_FARM_ID: &str = "Invalid farm id";
pub const INVALID_FARM_STATUS: &str = "Invalid farm status";
pub const INVALID_FARM_REWARD: &str = "Invalid reward token for this farm";

pub const INTERNAL_ERROR: &str = "Internal ERROR!";

// Contract Level
pub const CONTRACT_PAUSED: &str = "Contract paused";
