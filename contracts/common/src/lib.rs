//! Shared types and NEP-297 event helpers for every clawdao contract.
//!
//! All contracts log `EVENT_JSON:{"standard":"clawdao","version":"1.0.0","event":..,"data":[..]}`
//! so the indexer and any third party parse the same data.

use near_sdk::serde::Serialize;
use near_sdk::{env, AccountId};

pub const EVENT_STANDARD: &str = "clawdao";
pub const EVENT_VERSION: &str = "1.0.0";

/// Emit a NEP-297 event with a single data object.
pub fn emit_event<T: Serialize>(event: &str, data: &T) {
    let payload = near_sdk::serde_json::json!({
        "standard": EVENT_STANDARD,
        "version": EVENT_VERSION,
        "event": event,
        "data": [data],
    });
    env::log_str(&format!("EVENT_JSON:{payload}"));
}

/// Constitution tiers.
/// 0: immutable core, 1: constitution core (supermajority), 2: operating parameters,
/// 3: missions, 4: receipts written by valid agents only.
pub type Tier = u8;
pub const TIER_CORE: Tier = 0;
pub const TIER_CONSTITUTION: Tier = 1;
pub const TIER_PARAMS: Tier = 2;
pub const TIER_MISSIONS: Tier = 3;
pub const TIER_RECEIPTS: Tier = 4;

/// One nanosecond-precision day, used for daily spend windows.
pub const DAY_NS: u64 = 86_400 * 1_000_000_000;
pub const MONTH_NS: u64 = 30 * DAY_NS;

/// Asset identifier used for budgets and caps: `"near"` for native NEAR,
/// otherwise the NEP-141 token account, or `"chain:<name>:<asset>"` for
/// assets on other chains reached through Chain Signatures.
pub type Asset = String;

pub fn assert_caller(expected: &AccountId, what: &str) {
    if env::predecessor_account_id() != *expected {
        env::panic_str(&format!("Only {what} can call this method"));
    }
}
