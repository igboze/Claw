//! Vote escrow: lock CLAW for 1 to 24 months to get voting power.
//!
//! Power = amount * remaining_lock / 24 months, so it grows with lock length and
//! decays linearly towards the unlock date. Every change to a lock is checkpointed,
//! so the Governor can read anyone's power at a proposal's snapshot time.
//! Locking (rather than counting raw balances) stops flash-loan votes.
//!
//! Holder rewards: NEAR sent to `deposit_rewards` (trading fees when holders vote for
//! rewards, staking rewards, agent earnings) is shared pro-rata by locked CLAW amount and
//! claimed with `claim_rewards`. A standard reward-per-token accumulator keeps it O(1).

use clawdao_common::{assert_caller, emit_event, mul_div, MONTH_NS, U256};
use near_sdk::json_types::{U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::store::LookupMap;
use near_sdk::{
    env, ext_contract, near, require, serde_json, AccountId, BorshStorageKey, Gas, NearToken,
    PanicOnDefault, Promise, PromiseOrValue,
};

const GAS_FT_TRANSFER: Gas = Gas::from_tgas(10);
const GAS_RESOLVE: Gas = Gas::from_tgas(10);

#[ext_contract(ext_ft)]
#[allow(dead_code)]
trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub struct Lock {
    pub amount: U128,
    /// Nanosecond timestamp when the lock can be withdrawn.
    pub unlock_at: U64,
}

#[near(serializers = [borsh])]
#[derive(Clone)]
struct Checkpoint {
    ts: u64,
    amount: u128,
    unlock_at: u64,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct Params {
    pub min_months: u32,
    pub max_months: u32,
    /// Smallest amount accepted per lock, to deter storage spam.
    pub min_lock_amount: U128,
    /// Hard cap on any one account's voting power (governance-capture defence).
    pub max_power_per_account: U128,
}

#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde")]
struct LockMsg {
    months: u32,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct LockView {
    pub account_id: AccountId,
    pub amount: U128,
    pub unlock_at: U64,
    pub power: U128,
}

/// Accumulator precision.
const ACC_SCALE: u128 = 1_000_000_000_000_000_000_000_000;

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, Default)]
pub struct RewardAccount {
    /// acc_reward_per_token * amount / ACC_SCALE at the last settlement.
    pub debt: U128,
    /// NEAR earned and not yet claimed.
    pub owed: U128,
}

#[near]
#[derive(BorshStorageKey)]
enum StorageKey {
    Locks,
    History,
    Rewards,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    token_id: AccountId,
    governor_id: AccountId,
    params: Params,
    locks: LookupMap<AccountId, Lock>,
    history: LookupMap<AccountId, Vec<Checkpoint>>,
    total_locked: u128,
    /// NEAR per locked yocto-CLAW, times ACC_SCALE.
    acc_reward_per_token: u128,
    rewards: LookupMap<AccountId, RewardAccount>,
    /// Rewards that arrived while nothing was locked; added to the next deposit.
    undistributed: u128,
    total_rewards: u128,
}

pub fn power_of_lock(amount: u128, unlock_at: u64, ts: u64, params: &Params) -> u128 {
    if ts >= unlock_at || amount == 0 {
        return 0;
    }
    let max = params.max_months as u128 * MONTH_NS as u128;
    let remaining = ((unlock_at - ts) as u128).min(max);
    // remaining <= ~6.2e16 ns, so amount * remaining fits in u128 for any real supply.
    let power = amount.saturating_mul(remaining) / max;
    power.min(params.max_power_per_account.0)
}

#[near]
impl Contract {
    #[init]
    pub fn new(token_id: AccountId, governor_id: AccountId, params: Params) -> Self {
        require!(params.min_months >= 1 && params.min_months <= params.max_months, "Bad months");
        Self {
            token_id,
            governor_id,
            params,
            locks: LookupMap::new(StorageKey::Locks),
            history: LookupMap::new(StorageKey::History),
            total_locked: 0,
            acc_reward_per_token: 0,
            rewards: LookupMap::new(StorageKey::Rewards),
            undistributed: 0,
            total_rewards: 0,
        }
    }

    /// NEP-141 receiver. `msg` = `{"months": n}`. Adding to an existing lock keeps the
    /// later of the old and new unlock dates.
    pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> PromiseOrValue<U128> {
        require!(env::predecessor_account_id() == self.token_id, "Only CLAW can be locked");
        let LockMsg { months } = serde_json::from_str(&msg).expect("msg must be {\"months\": n}");
        require!(
            months >= self.params.min_months && months <= self.params.max_months,
            "Lock length out of range"
        );
        require!(amount.0 >= self.params.min_lock_amount.0, "Lock amount below minimum");
        let now = env::block_timestamp();
        let new_unlock = now + months as u64 * MONTH_NS;
        let lock = match self.locks.get(&sender_id) {
            Some(l) => Lock {
                amount: U128(l.amount.0 + amount.0),
                unlock_at: U64(l.unlock_at.0.max(new_unlock)),
            },
            None => Lock { amount, unlock_at: U64(new_unlock) },
        };
        self.total_locked += amount.0;
        self.write_lock(&sender_id, lock.clone());
        emit_event("ve_lock", &serde_json::json!({
            "account_id": sender_id, "added": amount, "amount": lock.amount, "unlock_at": lock.unlock_at,
        }));
        PromiseOrValue::Value(U128(0))
    }

    /// Push the unlock date out to `now + months`. Never shortens a lock.
    pub fn extend_lock(&mut self, months: u32) {
        require!(months >= self.params.min_months && months <= self.params.max_months, "Lock length out of range");
        let account = env::predecessor_account_id();
        let mut lock = self.locks.get(&account).cloned().expect("No lock");
        let target = env::block_timestamp() + months as u64 * MONTH_NS;
        require!(target > lock.unlock_at.0, "New unlock must be later");
        lock.unlock_at = U64(target);
        self.write_lock(&account, lock.clone());
        emit_event("ve_extend", &serde_json::json!({"account_id": account, "unlock_at": lock.unlock_at}));
    }

    /// Withdraw the whole lock after it expires.
    pub fn withdraw(&mut self) -> Promise {
        let account = env::predecessor_account_id();
        let lock = self.locks.get(&account).cloned().expect("No lock");
        require!(env::block_timestamp() >= lock.unlock_at.0, "Still locked");
        self.total_locked -= lock.amount.0;
        self.write_lock(&account, Lock { amount: U128(0), unlock_at: lock.unlock_at });
        ext_ft::ext(self.token_id.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FT_TRANSFER)
            .ft_transfer(account.clone(), lock.amount, Some("ve withdraw".into()))
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_RESOLVE).on_withdraw(account, lock))
    }

    #[private]
    pub fn on_withdraw(&mut self, account_id: AccountId, lock: Lock) {
        match env::promise_result_checked(0, 64) {
            Ok(_) => {
                emit_event("ve_withdraw", &serde_json::json!({"account_id": account_id, "amount": lock.amount}));
            }
            _ => {
                // Transfer failed (e.g. receiver not registered): restore the lock.
                self.total_locked += lock.amount.0;
                self.write_lock(&account_id, lock);
            }
        }
    }

    /// Anyone can add NEAR rewards for locked-CLAW holders.
    #[payable]
    pub fn deposit_rewards(&mut self) {
        let attached = env::attached_deposit().as_yoctonear();
        let amount = attached + self.undistributed;
        require!(amount > 0, "Attach NEAR");
        self.total_rewards += attached;
        if self.total_locked == 0 {
            self.undistributed = amount;
        } else {
            self.undistributed = 0;
            self.acc_reward_per_token += mul_div(amount, ACC_SCALE, self.total_locked);
        }
        emit_event("rewards_deposited", &serde_json::json!({
            "near": U128(attached), "total_locked": U128(self.total_locked),
        }));
    }

    pub fn claim_rewards(&mut self) -> Promise {
        let account = env::predecessor_account_id();
        let amount = self.locks.get(&account).map(|l| l.amount.0).unwrap_or(0);
        self.settle(&account, amount);
        self.reset_debt(&account, amount);
        let r = self.rewards.get_mut(&account).expect("No rewards");
        let owed = r.owed.0;
        require!(owed > 0, "Nothing to claim");
        r.owed = U128(0);
        emit_event("rewards_claimed", &serde_json::json!({ "account_id": account, "near": U128(owed) }));
        Promise::new(account).transfer(NearToken::from_yoctonear(owed))
    }

    pub fn pending_rewards(&self, account_id: AccountId) -> U128 {
        let amount = self.locks.get(&account_id).map(|l| l.amount.0).unwrap_or(0);
        let r = self.rewards.get(&account_id).cloned().unwrap_or_default();
        U128(r.owed.0 + self.accrued(amount, r.debt.0))
    }

    /// (total NEAR ever deposited, NEAR waiting for the first locker)
    pub fn rewards_info(&self) -> (U128, U128) {
        (U128(self.total_rewards), U128(self.undistributed))
    }

    /// Tier 1: Governor only.
    pub fn set_params(&mut self, params: Params) {
        assert_caller(&self.governor_id, "the Governor");
        require!(params.min_months >= 1 && params.min_months <= params.max_months, "Bad months");
        self.params = params.clone();
        emit_event("ve_params", &params);
    }

    // ---- views ----

    pub fn get_params(&self) -> Params {
        self.params.clone()
    }

    pub fn get_lock(&self, account_id: AccountId) -> Option<LockView> {
        self.locks.get(&account_id).map(|l| LockView {
            account_id: account_id.clone(),
            amount: l.amount,
            unlock_at: l.unlock_at,
            power: U128(power_of_lock(l.amount.0, l.unlock_at.0, env::block_timestamp(), &self.params)),
        })
    }

    pub fn power_of(&self, account_id: AccountId) -> U128 {
        self.power_at(account_id, U64(env::block_timestamp()))
    }

    /// Voting power at a past (or current) timestamp, from checkpoints.
    pub fn power_at(&self, account_id: AccountId, ts: U64) -> U128 {
        let Some(hist) = self.history.get(&account_id) else { return U128(0) };
        let idx = hist.partition_point(|c| c.ts <= ts.0);
        if idx == 0 {
            return U128(0);
        }
        let c = &hist[idx - 1];
        U128(power_of_lock(c.amount, c.unlock_at, ts.0, &self.params))
    }

    pub fn total_locked(&self) -> U128 {
        U128(self.total_locked)
    }

    pub fn get_config(&self) -> (AccountId, AccountId) {
        (self.token_id.clone(), self.governor_id.clone())
    }
}

impl Contract {
    fn gross(&self, amount: u128) -> u128 {
        (U256::from(amount) * U256::from(self.acc_reward_per_token) / U256::from(ACC_SCALE)).as_u128()
    }

    fn accrued(&self, amount: u128, debt: u128) -> u128 {
        self.gross(amount).saturating_sub(debt)
    }

    /// Credit rewards earned on `amount` (the balance before a change).
    fn settle(&mut self, account: &AccountId, amount: u128) {
        let mut r = self.rewards.get(account).cloned().unwrap_or_default();
        r.owed = U128(r.owed.0 + self.accrued(amount, r.debt.0));
        self.rewards.insert(account.clone(), r);
    }

    /// Start accruing from now on `new_amount`.
    fn reset_debt(&mut self, account: &AccountId, new_amount: u128) {
        let debt = self.gross(new_amount);
        if let Some(r) = self.rewards.get_mut(account) {
            r.debt = U128(debt);
        }
    }

    fn write_lock(&mut self, account: &AccountId, lock: Lock) {
        let old = self.locks.get(account).map(|l| l.amount.0).unwrap_or(0);
        self.settle(account, old);
        self.reset_debt(account, lock.amount.0);
        let cp = Checkpoint { ts: env::block_timestamp(), amount: lock.amount.0, unlock_at: lock.unlock_at.0 };
        let hist = self.history.entry(account.clone()).or_default();
        match hist.last_mut() {
            Some(last) if last.ts == cp.ts => *last = cp,
            _ => hist.push(cp),
        }
        if lock.amount.0 == 0 {
            self.locks.remove(account);
        } else {
            self.locks.insert(account.clone(), lock);
        }
    }
}

#[cfg(test)]
#[allow(unused_must_use)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    fn params() -> Params {
        Params { min_months: 1, max_months: 24, min_lock_amount: U128(1), max_power_per_account: U128(u128::MAX) }
    }

    fn setup() -> (VMContextBuilder, Contract) {
        let mut ctx = VMContextBuilder::new();
        testing_env!(ctx.block_timestamp(1_000).build());
        (ctx, Contract::new(accounts(0), accounts(1), params()))
    }

    #[test]
    fn power_scales_with_lock_length() {
        let (mut ctx, mut c) = setup();
        testing_env!(ctx.predecessor_account_id(accounts(0)).build());
        c.ft_on_transfer(accounts(2), U128(2400), r#"{"months":24}"#.into());
        c.ft_on_transfer(accounts(3), U128(2400), r#"{"months":12}"#.into());
        assert_eq!(c.power_of(accounts(2)).0, 2400);
        assert_eq!(c.power_of(accounts(3)).0, 1200);
    }

    #[test]
    fn power_decays_and_snapshots_hold() {
        let (mut ctx, mut c) = setup();
        testing_env!(ctx.predecessor_account_id(accounts(0)).build());
        c.ft_on_transfer(accounts(2), U128(2400), r#"{"months":24}"#.into());
        let t0 = 1_000u64;
        testing_env!(ctx.block_timestamp(t0 + 12 * MONTH_NS).build());
        assert_eq!(c.power_of(accounts(2)).0, 1200);
        assert_eq!(c.power_at(accounts(2), U64(t0)).0, 2400);
        assert_eq!(c.power_at(accounts(2), U64(t0 - 1)).0, 0);
    }

    #[test]
    fn cap_limits_whales() {
        let (mut ctx, mut c) = setup();
        testing_env!(ctx.predecessor_account_id(accounts(1)).build());
        c.set_params(Params { max_power_per_account: U128(100), ..params() });
        testing_env!(ctx.predecessor_account_id(accounts(0)).build());
        c.ft_on_transfer(accounts(2), U128(1_000_000), r#"{"months":24}"#.into());
        assert_eq!(c.power_of(accounts(2)).0, 100);
    }

    #[test]
    #[should_panic(expected = "Only CLAW can be locked")]
    fn rejects_other_tokens() {
        let (mut ctx, mut c) = setup();
        testing_env!(ctx.predecessor_account_id(accounts(4)).build());
        c.ft_on_transfer(accounts(2), U128(10), r#"{"months":3}"#.into());
    }

    #[test]
    #[should_panic(expected = "Still locked")]
    fn cannot_withdraw_early() {
        let (mut ctx, mut c) = setup();
        testing_env!(ctx.predecessor_account_id(accounts(0)).build());
        c.ft_on_transfer(accounts(2), U128(10), r#"{"months":3}"#.into());
        testing_env!(ctx.predecessor_account_id(accounts(2)).build());
        c.withdraw();
    }

    #[test]
    fn rewards_split_by_locked_amount() {
        let (mut ctx, mut c) = setup();
        testing_env!(ctx.predecessor_account_id(accounts(0)).build());
        c.ft_on_transfer(accounts(2), U128(3_000), r#"{"months":6}"#.into());
        c.ft_on_transfer(accounts(3), U128(1_000), r#"{"months":24}"#.into());
        testing_env!(ctx.predecessor_account_id(accounts(4)).attached_deposit(NearToken::from_yoctonear(4_000_000)).build());
        c.deposit_rewards();
        assert_eq!(c.pending_rewards(accounts(2)).0, 3_000_000);
        assert_eq!(c.pending_rewards(accounts(3)).0, 1_000_000);
        // A later locker does not share earlier rewards.
        testing_env!(ctx.predecessor_account_id(accounts(0)).attached_deposit(NearToken::from_yoctonear(0)).build());
        c.ft_on_transfer(accounts(1), U128(4_000), r#"{"months":3}"#.into());
        assert_eq!(c.pending_rewards(accounts(1)).0, 0);
        testing_env!(ctx.predecessor_account_id(accounts(4)).attached_deposit(NearToken::from_yoctonear(8_000_000)).build());
        c.deposit_rewards();
        assert_eq!(c.pending_rewards(accounts(1)).0, 4_000_000);
        assert_eq!(c.pending_rewards(accounts(2)).0, 3_000_000 + 3_000_000);
    }

    #[test]
    fn claim_pays_and_zeroes() {
        let (mut ctx, mut c) = setup();
        testing_env!(ctx.predecessor_account_id(accounts(0)).build());
        c.ft_on_transfer(accounts(2), U128(1_000), r#"{"months":6}"#.into());
        testing_env!(ctx.predecessor_account_id(accounts(4)).attached_deposit(NearToken::from_yoctonear(500)).build());
        c.deposit_rewards();
        testing_env!(ctx.predecessor_account_id(accounts(2)).attached_deposit(NearToken::from_yoctonear(0)).build());
        c.claim_rewards();
        assert_eq!(c.pending_rewards(accounts(2)).0, 0);
    }

    #[test]
    fn rewards_with_nothing_locked_wait() {
        let (mut ctx, mut c) = setup();
        testing_env!(ctx.predecessor_account_id(accounts(4)).attached_deposit(NearToken::from_yoctonear(900)).build());
        c.deposit_rewards();
        testing_env!(ctx.predecessor_account_id(accounts(0)).attached_deposit(NearToken::from_yoctonear(0)).build());
        c.ft_on_transfer(accounts(2), U128(1_000), r#"{"months":6}"#.into());
        testing_env!(ctx.predecessor_account_id(accounts(4)).attached_deposit(NearToken::from_yoctonear(100)).build());
        c.deposit_rewards();
        assert_eq!(c.pending_rewards(accounts(2)).0, 1_000);
    }

    #[test]
    #[should_panic(expected = "Only the Governor")]
    fn params_governor_only() {
        let (mut ctx, mut c) = setup();
        testing_env!(ctx.predecessor_account_id(accounts(2)).build());
        c.set_params(params());
    }
}
