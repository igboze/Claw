//! House of Stake: the treasury's NEAR is locked (and optionally staked) in a veNEAR
//! lockup owned by this contract, and CLAW holders decide how that veNEAR votes.
//!
//! - Setup, delegation and config: Governor only, through a Tier 1 `FunctionCall`.
//! - Lock / unlock / stake / withdraw: Governor only, through a Tier 2 `HosManage` proposal.
//!   Locked NEAR stays owned by this contract (the lockup is non-upgradable and only its
//!   owner can move funds), and the total is capped by `max_locked`.
//! - Votes: the Governor records a mandate after a `HosVote` proposal; anyone may then
//!   relay it with `hos_vote`, supplying the merkle proof from the HoS snapshot. The
//!   voting contract verifies the proof, so a relayer can neither forge nor change a vote.
//!
//! - Trading fees: the fee router calls `hos_lock_fees` with 80% of fee revenue attached;
//!   it is locked straight away (no cap; the policy is the point).
//! - Allocation: `treasury_allocate` (Tier 2 `Allocate`) sends treasury NEAR — staking
//!   rewards withdrawn from the lockup, agent marketplace earnings — where holders voted:
//!   lock in HoS, pay locked-CLAW holders, or buy back and burn CLAW.
//!
//! Interfaces follow houseofstake/house-of-stake-contracts (venear.dao / vote.dao on mainnet).

use crate::*;
use near_sdk::store::LookupMap;

const GAS_VENEAR_STORAGE: Gas = Gas::from_tgas(10);
const GAS_DEPLOY_LOCKUP: Gas = Gas::from_tgas(150);
const GAS_LOCKUP_SIMPLE: Gas = Gas::from_tgas(50);
const GAS_LOCKUP_STAKING: Gas = Gas::from_tgas(175);
const GAS_SET_DELEGATIONS: Gas = Gas::from_tgas(20);
const GAS_HOS_VOTE: Gas = Gas::from_tgas(30);
const GAS_ON_HOS_VOTE: Gas = Gas::from_tgas(10);
const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);

pub type HosProposalId = u32;

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct HosConfig {
    /// veNEAR contract (`venear.dao` on mainnet).
    pub venear_id: AccountId,
    /// HoS voting contract (`vote.dao` on mainnet).
    pub voting_id: AccountId,
    /// Hard cap on treasury NEAR locked in the lockup (yoctoNEAR).
    pub max_locked: U128,
    /// Attached to a first vote on a proposal (`vote_storage_fee` of the voting contract).
    pub vote_storage_fee: U128,
}

/// Mirrors HoS `VoteOption`.
#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HosVoteOption {
    For,
    Against,
    Abstain,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct HosMandate {
    pub vote: HosVoteOption,
    /// clawdao proposal that decided it.
    pub clawdao_proposal_id: u64,
    /// The vote currently cast on vote.dao, if any.
    pub cast: Option<HosVoteOption>,
    /// A relay is in flight; blocks parallel relays.
    pub pending: bool,
}

/// Mirrors HoS `DelegationEntry`.
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct DelegationEntry {
    pub account_id: AccountId,
    pub bps: u16,
}

/// Staking operations on the lockup. Pools must be on the HoS staking whitelist.
#[near(serializers = [json])]
#[derive(Clone, Debug)]
pub enum HosStakingCall {
    SelectStakingPool { staking_pool_account_id: AccountId },
    UnselectStakingPool,
    DepositAndStake { amount: U128 },
    Unstake { amount: U128 },
    UnstakeAll,
    WithdrawAllFromStakingPool,
    RefreshStakingPoolBalance,
}

/// Where holders can send treasury NEAR.
#[near(serializers = [json])]
#[derive(Clone, Copy, Debug)]
pub enum Allocation {
    /// Lock in House of Stake (counts against `max_locked`).
    StakeHoS,
    /// Pay locked-CLAW holders through the ve contract.
    HolderRewards,
    /// Buy back and burn CLAW through the fee router.
    Buyback,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct TreasuryLinks {
    pub fee_router_id: AccountId,
    pub ve_id: AccountId,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct HosInfo {
    pub config: Option<HosConfig>,
    pub lockup_account_id: Option<AccountId>,
    /// Treasury NEAR this contract has locked (tracked here; the lockup is the source of truth).
    pub locked: U128,
    /// NEAR locked from trading fees.
    pub locked_from_fees: U128,
    pub links: Option<TreasuryLinks>,
}

#[near(serializers = [borsh])]
pub struct HosState {
    pub config: Option<HosConfig>,
    pub locked: u128,
    pub mandates: LookupMap<HosProposalId, HosMandate>,
    pub locked_from_fees: u128,
    pub links: Option<TreasuryLinks>,
}

impl Default for HosState {
    fn default() -> Self {
        Self::new()
    }
}

impl HosState {
    pub fn new() -> Self {
        Self { config: None, locked: 0, mandates: LookupMap::new(StorageKey::HosMandates), locked_from_fees: 0, links: None }
    }
}

/// The lockup account veNEAR derives for an owner: hex(sha256(owner)[..20]).<venear>.
pub fn lockup_account_id(owner: &AccountId, venear_id: &AccountId) -> AccountId {
    let hash = hex::encode(&env::sha256(owner.as_bytes())[0..20]);
    format!("{hash}.{venear_id}").parse().expect("bad lockup account id")
}

#[near]
impl Contract {
    // ---- Tier 1 (FunctionCall) ----

    pub fn hos_configure(&mut self, config: HosConfig) {
        self.only_governor();
        require!(config.max_locked.0 >= self.hos.locked, "Cap below what is already locked");
        emit_event("hos_configured", &config);
        self.hos.config = Some(config);
    }

    /// Register on veNEAR and deploy this contract's lockup. Deposits come from the treasury;
    /// read them from `venear.storage_balance_bounds()` and `get_lockup_deployment_cost()`.
    pub fn hos_register(&mut self, storage_deposit: U128, lockup_deposit: U128) -> Promise {
        self.only_governor();
        let c = self.hos_config();
        Promise::new(c.venear_id.clone())
            .function_call(
                "storage_deposit".to_string(),
                b"{}".to_vec(),
                NearToken::from_yoctonear(storage_deposit.0),
                GAS_VENEAR_STORAGE,
            )
            .function_call("deploy_lockup".to_string(), b"{}".to_vec(), NearToken::from_yoctonear(lockup_deposit.0), GAS_DEPLOY_LOCKUP)
    }

    /// Split the treasury's veNEAR among delegates (empty list = undelegate).
    pub fn hos_set_delegations(&mut self, entries: Vec<DelegationEntry>, storage_deposit: U128) -> Promise {
        self.only_governor();
        let c = self.hos_config();
        emit_event("hos_delegations", &entries);
        Promise::new(c.venear_id).function_call(
            "set_delegations".to_string(),
            serde_json::to_vec(&serde_json::json!({ "entries": entries })).unwrap(),
            NearToken::from_yoctonear(storage_deposit.0.max(1)),
            GAS_SET_DELEGATIONS,
        )
    }

    /// Tier 1: the fee router and ve contract this treasury pays into.
    pub fn set_treasury_links(&mut self, links: TreasuryLinks) {
        self.only_governor();
        emit_event("treasury_links", &links);
        self.hos.links = Some(links);
    }

    // ---- fee router ----

    /// 80% of trading-fee revenue arrives here and is locked in House of Stake.
    #[payable]
    pub fn hos_lock_fees(&mut self) -> Promise {
        let links = self.hos.links.clone().expect("Treasury links not set");
        assert_caller(&links.fee_router_id, "the fee router");
        let amount = env::attached_deposit().as_yoctonear();
        require!(amount > 0, "Attach NEAR");
        self.hos.locked_from_fees += amount;
        emit_event("hos_lock_fees", &serde_json::json!({
            "amount": U128(amount), "total_from_fees": U128(self.hos.locked_from_fees),
        }));
        let lockup = self.hos_lockup();
        Promise::new(lockup.clone()).transfer(NearToken::from_yoctonear(amount)).then(self.lockup_call(
            &lockup,
            "lock_near",
            serde_json::json!({ "amount": U128(amount) }),
            GAS_LOCKUP_SIMPLE,
        ))
    }

    // ---- Tier 2 (Allocate) ----

    /// Send treasury NEAR where holders voted. Staking rewards reach the treasury through
    /// `hos_withdraw`; marketplace earnings arrive as NEAR once converted.
    pub fn treasury_allocate(&mut self, amount: U128, destination: Allocation) -> Promise {
        self.only_governor();
        require!(amount.0 > 0, "Zero amount");
        let links = self.hos.links.clone().expect("Treasury links not set");
        emit_event("treasury_allocated", &serde_json::json!({ "amount": amount, "destination": destination }));
        match destination {
            Allocation::StakeHoS => self.hos_lock(amount),
            Allocation::HolderRewards => Promise::new(links.ve_id).function_call(
                "deposit_rewards".to_string(),
                b"{}".to_vec(),
                NearToken::from_yoctonear(amount.0),
                GAS_SET_DELEGATIONS,
            ),
            Allocation::Buyback => Promise::new(links.fee_router_id).function_call(
                "deposit_buyback".to_string(),
                b"{}".to_vec(),
                NearToken::from_yoctonear(amount.0),
                GAS_SET_DELEGATIONS,
            ),
        }
    }

    // ---- Tier 2 (HosManage) ----

    /// Move treasury NEAR into the lockup and lock it, up to `max_locked`.
    pub fn hos_lock(&mut self, amount: U128) -> Promise {
        self.only_governor();
        let c = self.hos_config();
        let total = self.hos.locked.checked_add(amount.0).expect("overflow");
        require!(total <= c.max_locked.0, "Above the House of Stake lock cap");
        self.hos.locked = total;
        emit_event("hos_lock", &serde_json::json!({ "amount": amount, "total_locked": U128(total) }));
        let lockup = self.hos_lockup();
        Promise::new(lockup.clone()).transfer(NearToken::from_yoctonear(amount.0)).then(self.lockup_call(
            &lockup,
            "lock_near",
            serde_json::json!({ "amount": amount }),
            GAS_LOCKUP_SIMPLE,
        ))
    }

    /// Start the unlock period (45 days on mainnet). Forfeits accrued veNEAR growth.
    pub fn hos_begin_unlock(&mut self, amount: U128) -> Promise {
        self.only_governor();
        self.hos.locked = self.hos.locked.saturating_sub(amount.0);
        emit_event("hos_begin_unlock", &serde_json::json!({ "amount": amount }));
        let lockup = self.hos_lockup();
        self.lockup_call(&lockup, "begin_unlock_near", serde_json::json!({ "amount": amount }), GAS_LOCKUP_SIMPLE)
    }

    pub fn hos_end_unlock(&mut self, amount: Option<U128>) -> Promise {
        self.only_governor();
        let lockup = self.hos_lockup();
        self.lockup_call(&lockup, "end_unlock_near", serde_json::json!({ "amount": amount }), GAS_LOCKUP_SIMPLE)
    }

    /// Bring liquid NEAR (unlocked funds or staking rewards) back to the treasury.
    /// The receiver is always this contract.
    pub fn hos_withdraw(&mut self, amount: U128) -> Promise {
        self.only_governor();
        emit_event("hos_withdraw", &serde_json::json!({ "amount": amount }));
        let lockup = self.hos_lockup();
        let me = env::current_account_id();
        self.lockup_call(&lockup, "transfer", serde_json::json!({ "amount": amount, "receiver_id": me }), GAS_LOCKUP_SIMPLE)
    }

    pub fn hos_staking(&mut self, call: HosStakingCall) -> Promise {
        self.only_governor();
        emit_event("hos_staking", &call);
        let lockup = self.hos_lockup();
        let (method, args) = match call {
            HosStakingCall::SelectStakingPool { staking_pool_account_id } => {
                ("select_staking_pool", serde_json::json!({ "staking_pool_account_id": staking_pool_account_id }))
            }
            HosStakingCall::UnselectStakingPool => ("unselect_staking_pool", serde_json::json!({})),
            HosStakingCall::DepositAndStake { amount } => ("deposit_and_stake", serde_json::json!({ "amount": amount })),
            HosStakingCall::Unstake { amount } => ("unstake", serde_json::json!({ "amount": amount })),
            HosStakingCall::UnstakeAll => ("unstake_all", serde_json::json!({})),
            HosStakingCall::WithdrawAllFromStakingPool => ("withdraw_all_from_staking_pool", serde_json::json!({})),
            HosStakingCall::RefreshStakingPoolBalance => ("refresh_staking_pool_balance", serde_json::json!({})),
        };
        self.lockup_call(&lockup, method, args, GAS_LOCKUP_STAKING)
    }

    // ---- Tier 3 (HosVote) ----

    /// Record how CLAW holders voted on a HoS proposal. A later mandate replaces an
    /// earlier one; HoS lets the vote change while voting is open.
    pub fn hos_set_mandate(&mut self, proposal_id: HosProposalId, vote: HosVoteOption, clawdao_proposal_id: u64) {
        self.only_governor();
        let cast = self.hos.mandates.get(&proposal_id).and_then(|m| m.cast);
        self.hos.mandates.insert(proposal_id, HosMandate { vote, clawdao_proposal_id, cast, pending: false });
        emit_event("hos_mandate", &serde_json::json!({
            "hos_proposal_id": proposal_id, "vote": vote, "clawdao_proposal_id": clawdao_proposal_id,
        }));
    }

    /// Cast the mandated vote on the HoS voting contract. Anyone can call; `merkle_proof`
    /// and `v_account` come from `venear.get_proof(<this contract>)` at the proposal's
    /// snapshot block, and the voting contract rejects anything that does not verify.
    pub fn hos_vote(&mut self, proposal_id: HosProposalId, merkle_proof: serde_json::Value, v_account: serde_json::Value) -> Promise {
        require!(!self.paused, "Agent is paused");
        let c = self.hos_config();
        let m = self.hos.mandates.get_mut(&proposal_id).expect("No mandate for this proposal");
        require!(!m.pending, "A vote relay is already in flight");
        require!(m.cast != Some(m.vote), "Mandate already cast");
        let deposit = if m.cast.is_none() { c.vote_storage_fee.0.max(1) } else { 1 };
        m.pending = true;
        let vote = m.vote;
        Promise::new(c.voting_id)
            .function_call(
                "vote".to_string(),
                serde_json::to_vec(&serde_json::json!({
                    "proposal_id": proposal_id, "vote": vote, "merkle_proof": merkle_proof, "v_account": v_account,
                }))
                .unwrap(),
                NearToken::from_yoctonear(deposit),
                GAS_HOS_VOTE,
            )
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_ON_HOS_VOTE).on_hos_vote(proposal_id, vote))
    }

    #[private]
    pub fn on_hos_vote(&mut self, proposal_id: HosProposalId, vote: HosVoteOption) -> bool {
        let ok = !matches!(env::promise_result_checked(0, 1_024), Err(PromiseError::Failed));
        if let Some(m) = self.hos.mandates.get_mut(&proposal_id) {
            m.pending = false;
            if ok {
                m.cast = Some(vote);
            }
        }
        emit_event("hos_vote", &serde_json::json!({ "hos_proposal_id": proposal_id, "vote": vote, "success": ok }));
        ok
    }

    // ---- views ----

    pub fn hos_info(&self) -> HosInfo {
        HosInfo {
            config: self.hos.config.clone(),
            lockup_account_id: self.hos.config.as_ref().map(|c| lockup_account_id(&env::current_account_id(), &c.venear_id)),
            locked: U128(self.hos.locked),
            locked_from_fees: U128(self.hos.locked_from_fees),
            links: self.hos.links.clone(),
        }
    }

    pub fn hos_mandate(&self, proposal_id: HosProposalId) -> Option<HosMandate> {
        self.hos.mandates.get(&proposal_id).cloned()
    }
}

impl Contract {
    fn hos_config(&self) -> HosConfig {
        self.hos.config.clone().expect("House of Stake is not configured")
    }

    fn hos_lockup(&self) -> AccountId {
        lockup_account_id(&env::current_account_id(), &self.hos_config().venear_id)
    }

    fn lockup_call(&self, lockup: &AccountId, method: &str, args: serde_json::Value, gas: Gas) -> Promise {
        Promise::new(lockup.clone()).function_call(method.to_string(), serde_json::to_vec(&args).unwrap(), ONE_YOCTO, gas)
    }
}

#[cfg(test)]
#[allow(unused_must_use)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, PromiseResult};
    use serde_json::json;

    const NEAR: u128 = 10u128.pow(24);

    fn setup(ctx: &mut VMContextBuilder) -> Contract {
        testing_env!(ctx.current_account_id("agent.claw.near".parse().unwrap()).account_balance(NearToken::from_near(1_000)).predecessor_account_id(accounts(0)).build());
        let mut c = Contract::new(false, U64(1), accounts(0), accounts(1), "v1.signer".parse().unwrap(), None, U64(0));
        c.hos_configure(HosConfig {
            venear_id: "venear.dao".parse().unwrap(),
            voting_id: "vote.dao".parse().unwrap(),
            max_locked: U128(100 * NEAR),
            vote_storage_fee: U128(NEAR / 8),
        });
        c
    }

    #[test]
    fn lockup_id_matches_venear_derivation() {
        let mut ctx = VMContextBuilder::new();
        let c = setup(&mut ctx);
        let id = c.hos_info().lockup_account_id.unwrap().to_string();
        let (hash, rest) = id.split_once('.').unwrap();
        assert_eq!(rest, "venear.dao");
        assert_eq!(hash.len(), 40);
        assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn lock_tracks_total_and_unlock_reduces_it() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        c.hos_lock(U128(60 * NEAR));
        c.hos_lock(U128(40 * NEAR));
        assert_eq!(c.hos_info().locked.0, 100 * NEAR);
        c.hos_begin_unlock(U128(30 * NEAR));
        assert_eq!(c.hos_info().locked.0, 70 * NEAR);
    }

    #[test]
    #[should_panic(expected = "Above the House of Stake lock cap")]
    fn lock_cap_enforced() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        c.hos_lock(U128(101 * NEAR));
    }

    #[test]
    #[should_panic(expected = "Only the Governor")]
    fn treasury_moves_are_governor_only() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        testing_env!(ctx.predecessor_account_id(accounts(3)).build());
        c.hos_lock(U128(1));
    }

    #[test]
    #[should_panic(expected = "Only the Governor")]
    fn mandates_are_governor_only() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        testing_env!(ctx.predecessor_account_id(accounts(3)).build());
        c.hos_set_mandate(7, HosVoteOption::For, 1);
    }

    fn link(c: &mut Contract) {
        c.set_treasury_links(TreasuryLinks { fee_router_id: "fees.claw.near".parse().unwrap(), ve_id: "ve.claw.near".parse().unwrap() });
    }

    #[test]
    fn fee_router_locks_uncapped() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        link(&mut c);
        testing_env!(ctx.predecessor_account_id("fees.claw.near".parse().unwrap()).attached_deposit(NearToken::from_near(500)).build());
        c.hos_lock_fees();
        let info = c.hos_info();
        assert_eq!(info.locked_from_fees.0, 500 * NEAR);
        assert_eq!(info.locked.0, 0, "fee locks do not use the treasury cap");
    }

    #[test]
    #[should_panic(expected = "Only the fee router")]
    fn only_fee_router_locks_fees() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        link(&mut c);
        testing_env!(ctx.predecessor_account_id(accounts(3)).attached_deposit(NearToken::from_near(1)).build());
        c.hos_lock_fees();
    }

    #[test]
    fn allocate_routes_by_vote() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        link(&mut c);
        c.treasury_allocate(U128(NEAR), Allocation::HolderRewards);
        c.treasury_allocate(U128(NEAR), Allocation::Buyback);
        c.treasury_allocate(U128(10 * NEAR), Allocation::StakeHoS);
        assert_eq!(c.hos_info().locked.0, 10 * NEAR);
    }

    #[test]
    #[should_panic(expected = "Only the Governor")]
    fn allocate_governor_only() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        link(&mut c);
        testing_env!(ctx.predecessor_account_id(accounts(3)).build());
        c.treasury_allocate(U128(1), Allocation::Buyback);
    }

    #[test]
    #[should_panic(expected = "No mandate")]
    fn cannot_relay_without_mandate() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        testing_env!(ctx.predecessor_account_id(accounts(4)).build());
        c.hos_vote(7, json!({}), json!({}));
    }

    #[test]
    fn relay_casts_once_then_allows_changed_mandate() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        c.hos_set_mandate(7, HosVoteOption::For, 1);
        testing_env!(ctx.predecessor_account_id(accounts(4)).build());
        c.hos_vote(7, json!({"index": 0, "path": []}), json!({"V0": {}}));
        assert!(c.hos_mandate(7).unwrap().pending);

        testing_env!(
            ctx.predecessor_account_id("agent.claw.near".parse().unwrap()).build(),
            near_sdk::test_vm_config(),
            near_sdk::RuntimeFeesConfig::test(),
            Default::default(),
            vec![PromiseResult::Successful(vec![])]
        );
        assert!(c.on_hos_vote(7, HosVoteOption::For));
        let m = c.hos_mandate(7).unwrap();
        assert_eq!(m.cast, Some(HosVoteOption::For));
        assert!(!m.pending);

        // A new clawdao decision replaces the mandate and can be relayed again.
        testing_env!(ctx.predecessor_account_id(accounts(0)).build());
        c.hos_set_mandate(7, HosVoteOption::Against, 2);
        testing_env!(ctx.predecessor_account_id(accounts(4)).build());
        c.hos_vote(7, json!({}), json!({}));
    }

    #[test]
    #[should_panic(expected = "Mandate already cast")]
    fn no_duplicate_relay() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        c.hos_set_mandate(7, HosVoteOption::For, 1);
        testing_env!(
            ctx.predecessor_account_id("agent.claw.near".parse().unwrap()).build(),
            near_sdk::test_vm_config(),
            near_sdk::RuntimeFeesConfig::test(),
            Default::default(),
            vec![PromiseResult::Successful(vec![])]
        );
        c.hos.mandates.get_mut(&7).unwrap().pending = true;
        c.on_hos_vote(7, HosVoteOption::For);
        c.hos_vote(7, json!({}), json!({}));
    }
}
