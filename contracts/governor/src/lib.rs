//! Governor: proposals, ve-weighted voting, quorum, tiered timelocks, execution.
//!
//! Each proposal targets one constitution tier. The tier decides the voting period,
//! quorum, approval threshold and timelock, and also which kinds of action the
//! proposal may carry:
//!
//! | Tier | Kinds allowed                         | Typical settings              |
//! |------|---------------------------------------|-------------------------------|
//! | 1    | `Amend` (tier 1 article), `FunctionCall` | 66.67% supermajority, 14d lock |
//! | 2    | `Amend` (tier 2 article), `SetParams` | 60%, 7d lock                  |
//! | 3    | `SetMission`, `CancelMission`         | simple majority, 2d lock      |
//!
//! Tier 0 can never be proposed. `FunctionCall` (upgrades, code hashes, guardrails,
//! governor settings) is Tier 1 only, so the strictest rules guard the most power.

use clawdao_common::{emit_event, Tier, TIER_CONSTITUTION, TIER_MISSIONS, TIER_PARAMS};
use near_sdk::json_types::{Base64VecU8, U128, U64};
use near_sdk::serde::Serialize;
use near_sdk::store::{IterableMap, LookupMap};
use near_sdk::{
    env, ext_contract, near, require, serde_json, AccountId, BorshStorageKey, Gas, NearToken,
    PanicOnDefault, Promise, PromiseError,
};

const GAS_POWER_AT: Gas = Gas::from_tgas(10);
const GAS_ON_POWER: Gas = Gas::from_tgas(15);
const GAS_CONSTITUTION_CALL: Gas = Gas::from_tgas(30);
const GAS_ON_EXECUTED: Gas = Gas::from_tgas(10);
const MAX_TITLE: usize = 200;
const MAX_ACTIONS: usize = 8;

#[ext_contract(ext_ve)]
#[allow(dead_code)]
trait VoteEscrow {
    fn power_at(&self, account_id: AccountId, ts: U64) -> U128;
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct TierConfig {
    pub voting_period_ns: U64,
    pub timelock_ns: U64,
    /// Minimum total ve power cast (for + against + abstain).
    pub quorum: U128,
    /// Share of for / (for + against) needed to pass, in basis points.
    pub threshold_bps: u16,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct ActionCall {
    pub method_name: String,
    pub args: Base64VecU8,
    pub deposit: U128,
    pub gas: U64,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub enum ProposalKind {
    /// `constitution.amend(tier, article)`; the article's tier is the proposal tier (1 or 2).
    Amend { article_json: String },
    /// `constitution.remove_article(tier, article_id)`.
    RemoveArticle { article_id: String },
    /// `constitution.set_params(params)` — Tier 2.
    SetParams { params_json: String },
    /// `constitution.set_mission(mission)` — Tier 3.
    SetMission { mission_json: String },
    /// `constitution.cancel_mission(mission_id)` — Tier 3.
    CancelMission { mission_id: String },
    /// Arbitrary calls on any account, including this governor — Tier 1 only.
    FunctionCall { receiver_id: AccountId, actions: Vec<ActionCall> },
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Status {
    Active,
    Defeated,
    Queued,
    Executing,
    Executed,
    Failed,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Choice {
    For,
    Against,
    Abstain,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub proposer: AccountId,
    pub tier: Tier,
    pub kind: ProposalKind,
    pub title: String,
    /// IPFS/Arweave URI of the full text.
    pub body_uri: String,
    pub bond: U128,
    /// ve power is read at this timestamp (proposal creation).
    pub snapshot_ts: U64,
    pub ends_at: U64,
    /// When the timelock ends; set when the proposal is queued.
    pub eta: Option<U64>,
    pub votes_for: U128,
    pub votes_against: U128,
    pub votes_abstain: U128,
    pub status: Status,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub enum VoteRecord {
    /// Waiting for the ve power callback; blocks a second vote meanwhile.
    Pending,
    Cast { choice: Choice, power: U128 },
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Config {
    pub ve_id: AccountId,
    pub constitution_id: AccountId,
    pub proposal_bond: U128,
    pub tiers: Vec<(Tier, TierConfig)>,
}

#[near]
#[derive(BorshStorageKey)]
enum StorageKey {
    Proposals,
    Votes,
    Tiers,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    ve_id: AccountId,
    constitution_id: AccountId,
    proposal_bond: u128,
    tiers: IterableMap<Tier, TierConfig>,
    proposals: IterableMap<u64, Proposal>,
    votes: LookupMap<(u64, AccountId), VoteRecord>,
    next_id: u64,
}

#[near]
impl Contract {
    #[init]
    pub fn new(
        ve_id: AccountId,
        constitution_id: AccountId,
        proposal_bond: U128,
        tiers: Vec<(Tier, TierConfig)>,
    ) -> Self {
        let mut this = Self {
            ve_id,
            constitution_id,
            proposal_bond: proposal_bond.0,
            tiers: IterableMap::new(StorageKey::Tiers),
            proposals: IterableMap::new(StorageKey::Proposals),
            votes: LookupMap::new(StorageKey::Votes),
            next_id: 0,
        };
        for (tier, cfg) in tiers {
            this.internal_set_tier(tier, cfg);
        }
        for t in [TIER_CONSTITUTION, TIER_PARAMS, TIER_MISSIONS] {
            require!(this.tiers.get(&t).is_some(), "Tiers 1, 2 and 3 must be configured");
        }
        this
    }

    /// Create a proposal. Attach `proposal_bond` yoctoNEAR; it is refunded on finalize.
    #[payable]
    pub fn propose(&mut self, tier: Tier, kind: ProposalKind, title: String, body_uri: String) -> u64 {
        require!(env::attached_deposit().as_yoctonear() >= self.proposal_bond, "Attach the proposal bond");
        require!(!title.is_empty() && title.len() <= MAX_TITLE, "Bad title");
        let cfg = self.tiers.get(&tier).cloned().expect("Tier cannot be proposed");
        validate_kind(tier, &kind);
        let now = env::block_timestamp();
        let id = self.next_id;
        self.next_id += 1;
        let proposal = Proposal {
            id,
            proposer: env::predecessor_account_id(),
            tier,
            kind,
            title,
            body_uri,
            bond: U128(env::attached_deposit().as_yoctonear()),
            snapshot_ts: U64(now),
            ends_at: U64(now + cfg.voting_period_ns.0),
            eta: None,
            votes_for: U128(0),
            votes_against: U128(0),
            votes_abstain: U128(0),
            status: Status::Active,
        };
        emit_event("proposal_created", &proposal);
        self.proposals.insert(id, proposal);
        id
    }

    /// Vote with ve power at the proposal's snapshot. One vote per account, final.
    pub fn vote(&mut self, proposal_id: u64, choice: Choice) -> Promise {
        let p = self.proposals.get(&proposal_id).expect("No proposal");
        require!(p.status == Status::Active, "Proposal not active");
        require!(env::block_timestamp() < p.ends_at.0, "Voting ended");
        let voter = env::predecessor_account_id();
        let key = (proposal_id, voter.clone());
        require!(self.votes.get(&key).is_none(), "Already voted");
        self.votes.insert(key, VoteRecord::Pending);
        ext_ve::ext(self.ve_id.clone())
            .with_static_gas(GAS_POWER_AT)
            .power_at(voter.clone(), p.snapshot_ts)
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_ON_POWER).on_power(proposal_id, voter, choice))
    }

    #[private]
    pub fn on_power(&mut self, proposal_id: u64, voter: AccountId, choice: Choice) -> U128 {
        let key = (proposal_id, voter.clone());
        let power = match env::promise_result_checked(0, 64) {
            Ok(bytes) => serde_json::from_slice::<U128>(&bytes).map(|p| p.0).unwrap_or(0),
            Err(_) => 0,
        };
        if power == 0 {
            // Let the account try again once it has power or the ve call succeeds.
            self.votes.remove(&key);
            env::log_str("No voting power at snapshot; vote not recorded");
            return U128(0);
        }
        let p = self.proposals.get_mut(&proposal_id).expect("No proposal");
        match choice {
            Choice::For => p.votes_for.0 += power,
            Choice::Against => p.votes_against.0 += power,
            Choice::Abstain => p.votes_abstain.0 += power,
        }
        self.votes.insert(key, VoteRecord::Cast { choice, power: U128(power) });
        emit_event("vote_cast", &serde_json::json!({
            "proposal_id": proposal_id, "voter": voter, "choice": choice, "power": U128(power),
        }));
        U128(power)
    }

    /// After voting ends: queue (start the timelock) or defeat. Anyone can call.
    pub fn finalize(&mut self, proposal_id: u64) -> Status {
        let now = env::block_timestamp();
        let p = self.proposals.get(&proposal_id).cloned().expect("No proposal");
        require!(p.status == Status::Active, "Proposal not active");
        require!(now >= p.ends_at.0, "Voting still open");
        let cfg = self.tiers.get(&p.tier).cloned().expect("Tier missing");
        let passed = tally_passes(&p, &cfg);
        let p = self.proposals.get_mut(&proposal_id).unwrap();
        if passed {
            p.status = Status::Queued;
            p.eta = Some(U64(now + cfg.timelock_ns.0));
        } else {
            p.status = Status::Defeated;
        }
        if p.bond.0 > 0 {
            Promise::new(p.proposer.clone()).transfer(NearToken::from_yoctonear(p.bond.0)).detach();
            p.bond = U128(0);
        }
        let status = p.status;
        emit_event("proposal_finalized", &serde_json::json!({
            "proposal_id": proposal_id, "status": status, "eta": p.eta,
            "votes_for": p.votes_for, "votes_against": p.votes_against, "votes_abstain": p.votes_abstain,
        }));
        status
    }

    /// After the timelock: run the proposal's action. Anyone can call.
    pub fn execute(&mut self, proposal_id: u64) -> Promise {
        let p = self.proposals.get(&proposal_id).cloned().expect("No proposal");
        require!(p.status == Status::Queued, "Proposal not queued");
        require!(env::block_timestamp() >= p.eta.expect("No eta").0, "Timelock not over");
        self.proposals.get_mut(&proposal_id).unwrap().status = Status::Executing;
        self.build_action(&p)
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_ON_EXECUTED).on_executed(proposal_id))
    }

    #[private]
    pub fn on_executed(&mut self, proposal_id: u64) -> Status {
        // A long return value still means the call succeeded.
        let ok = !matches!(env::promise_result_checked(0, 1_024), Err(PromiseError::Failed));
        let p = self.proposals.get_mut(&proposal_id).expect("No proposal");
        p.status = if ok { Status::Executed } else { Status::Failed };
        emit_event("proposal_executed", &serde_json::json!({"proposal_id": proposal_id, "success": ok}));
        p.status
    }

    // ---- self-governance (reached only through a Tier 1 FunctionCall proposal) ----

    #[private]
    pub fn set_tier_config(&mut self, tier: Tier, config: TierConfig) {
        self.internal_set_tier(tier, config);
    }

    #[private]
    pub fn set_proposal_bond(&mut self, bond: U128) {
        self.proposal_bond = bond.0;
        emit_event("governor_bond", &serde_json::json!({"bond": bond}));
    }

    // ---- views ----

    pub fn get_proposal(&self, proposal_id: u64) -> Option<Proposal> {
        self.proposals.get(&proposal_id).cloned()
    }

    pub fn get_proposals(&self, from_index: Option<u64>, limit: Option<u64>) -> Vec<Proposal> {
        let from = from_index.unwrap_or(0);
        let limit = limit.unwrap_or(50).min(100);
        (from..self.next_id.min(from + limit)).filter_map(|i| self.proposals.get(&i).cloned()).collect()
    }

    pub fn get_vote(&self, proposal_id: u64, account_id: AccountId) -> Option<VoteRecord> {
        self.votes.get(&(proposal_id, account_id)).cloned()
    }

    pub fn get_config(&self) -> Config {
        Config {
            ve_id: self.ve_id.clone(),
            constitution_id: self.constitution_id.clone(),
            proposal_bond: U128(self.proposal_bond),
            tiers: self.tiers.iter().map(|(t, c)| (*t, c.clone())).collect(),
        }
    }

    pub fn proposal_count(&self) -> u64 {
        self.next_id
    }
}

impl Contract {
    fn internal_set_tier(&mut self, tier: Tier, cfg: TierConfig) {
        require!((TIER_CONSTITUTION..=TIER_MISSIONS).contains(&tier), "Only tiers 1 to 3 are governed");
        require!(cfg.threshold_bps > 5_000 && cfg.threshold_bps <= 10_000, "Threshold must be a majority");
        if tier == TIER_CONSTITUTION {
            // Tier 1 guards upgrades and code hashes: keep it slow and hard.
            require!(cfg.threshold_bps >= 6_667, "Tier 1 needs a supermajority");
            require!(cfg.timelock_ns.0 >= 7 * clawdao_common::DAY_NS, "Tier 1 timelock must be >= 7 days");
        }
        emit_event("governor_tier", &serde_json::json!({"tier": tier, "config": cfg}));
        self.tiers.insert(tier, cfg);
    }

    fn build_action(&self, p: &Proposal) -> Promise {
        let call = |method: &str, args: String| {
            Promise::new(self.constitution_id.clone()).function_call(
                method.to_string(),
                args.into_bytes(),
                NearToken::from_yoctonear(0),
                GAS_CONSTITUTION_CALL,
            )
        };
        match &p.kind {
            ProposalKind::Amend { article_json } => {
                call("amend", format!(r#"{{"tier":{},"article":{}}}"#, p.tier, article_json))
            }
            ProposalKind::RemoveArticle { article_id } => call(
                "remove_article",
                serde_json::json!({"tier": p.tier, "article_id": article_id}).to_string(),
            ),
            ProposalKind::SetParams { params_json } => call("set_params", format!(r#"{{"params":{params_json}}}"#)),
            ProposalKind::SetMission { mission_json } => {
                call("set_mission", format!(r#"{{"mission":{mission_json}}}"#))
            }
            ProposalKind::CancelMission { mission_id } => {
                call("cancel_mission", serde_json::json!({"mission_id": mission_id}).to_string())
            }
            ProposalKind::FunctionCall { receiver_id, actions } => {
                let mut promise = Promise::new(receiver_id.clone());
                for a in actions {
                    promise = promise.function_call(
                        a.method_name.clone(),
                        a.args.0.clone(),
                        NearToken::from_yoctonear(a.deposit.0),
                        Gas::from_gas(a.gas.0),
                    );
                }
                promise
            }
        }
    }
}

fn is_json_object(s: &str) -> bool {
    matches!(serde_json::from_str::<serde_json::Value>(s), Ok(serde_json::Value::Object(_)))
}

/// Which kinds each tier may carry. Rejects malformed JSON early so a passed
/// proposal cannot fail on parse at execution.
pub fn validate_kind(tier: Tier, kind: &ProposalKind) {
    let ok = match kind {
        ProposalKind::Amend { article_json } => {
            (tier == TIER_CONSTITUTION || tier == TIER_PARAMS) && is_json_object(article_json)
        }
        ProposalKind::RemoveArticle { .. } => tier == TIER_CONSTITUTION || tier == TIER_PARAMS,
        ProposalKind::SetParams { params_json } => tier == TIER_PARAMS && is_json_object(params_json),
        ProposalKind::SetMission { mission_json } => tier == TIER_MISSIONS && is_json_object(mission_json),
        ProposalKind::CancelMission { .. } => tier == TIER_MISSIONS,
        ProposalKind::FunctionCall { actions, .. } => {
            tier == TIER_CONSTITUTION && !actions.is_empty() && actions.len() <= MAX_ACTIONS
        }
    };
    require!(ok, "Proposal kind not allowed for this tier");
}

pub fn tally_passes(p: &Proposal, cfg: &TierConfig) -> bool {
    let f = p.votes_for.0;
    let a = p.votes_against.0;
    let total = f + a + p.votes_abstain.0;
    if total < cfg.quorum.0 || f + a == 0 {
        return false;
    }
    f * 10_000 >= cfg.threshold_bps as u128 * (f + a)
}

#[cfg(test)]
#[allow(unused_must_use)]
mod tests {
    use super::*;
    use clawdao_common::DAY_NS;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, PromiseResult};

    fn tier(timelock_days: u64, bps: u16) -> TierConfig {
        TierConfig { voting_period_ns: U64(3 * DAY_NS), timelock_ns: U64(timelock_days * DAY_NS), quorum: U128(100), threshold_bps: bps }
    }

    fn setup() -> (VMContextBuilder, Contract) {
        let mut ctx = VMContextBuilder::new();
        testing_env!(ctx.current_account_id(accounts(5)).block_timestamp(1).build());
        let c = Contract::new(
            accounts(0),
            accounts(1),
            U128(10),
            vec![(1, tier(14, 6_667)), (2, tier(7, 6_000)), (3, tier(2, 5_001))],
        );
        (ctx, c)
    }

    fn mission() -> ProposalKind {
        ProposalKind::SetMission { mission_json: r#"{"id":"m1"}"#.into() }
    }

    fn propose(ctx: &mut VMContextBuilder, c: &mut Contract, tier: Tier, kind: ProposalKind) -> u64 {
        testing_env!(ctx.predecessor_account_id(accounts(2)).attached_deposit(NearToken::from_yoctonear(10)).build());
        c.propose(tier, kind, "t".into(), "ipfs://x".into())
    }

    fn cast(ctx: &mut VMContextBuilder, c: &mut Contract, id: u64, voter: AccountId, choice: Choice, power: u128) {
        testing_env!(
            ctx.predecessor_account_id(accounts(5)).attached_deposit(NearToken::from_yoctonear(0)).build(),
            near_sdk::test_vm_config(),
            near_sdk::RuntimeFeesConfig::test(),
            Default::default(),
            vec![PromiseResult::Successful(serde_json::to_vec(&U128(power)).unwrap())]
        );
        c.votes.insert((id, voter.clone()), VoteRecord::Pending);
        c.on_power(id, voter, choice);
    }

    #[test]
    fn full_lifecycle_respects_timelock() {
        let (mut ctx, mut c) = setup();
        let id = propose(&mut ctx, &mut c, 3, mission());
        cast(&mut ctx, &mut c, id, accounts(3), Choice::For, 80);
        cast(&mut ctx, &mut c, id, accounts(4), Choice::Against, 30);
        testing_env!(ctx.block_timestamp(1 + 3 * DAY_NS).build());
        assert_eq!(c.finalize(id), Status::Queued);
        let eta = c.get_proposal(id).unwrap().eta.unwrap().0;
        assert_eq!(eta, 1 + 5 * DAY_NS);
    }

    #[test]
    #[should_panic(expected = "Timelock not over")]
    fn cannot_execute_before_eta() {
        let (mut ctx, mut c) = setup();
        let id = propose(&mut ctx, &mut c, 3, mission());
        cast(&mut ctx, &mut c, id, accounts(3), Choice::For, 200);
        testing_env!(ctx.block_timestamp(1 + 3 * DAY_NS).build());
        c.finalize(id);
        testing_env!(ctx.block_timestamp(1 + 4 * DAY_NS).build());
        c.execute(id);
    }

    #[test]
    fn quorum_and_threshold() {
        let (mut ctx, mut c) = setup();
        let id = propose(&mut ctx, &mut c, 1, ProposalKind::Amend { article_json: r#"{"id":"a"}"#.into() });
        // 60% for: fails the tier-1 supermajority.
        cast(&mut ctx, &mut c, id, accounts(3), Choice::For, 60);
        cast(&mut ctx, &mut c, id, accounts(4), Choice::Against, 40);
        testing_env!(ctx.block_timestamp(1 + 3 * DAY_NS).build());
        assert_eq!(c.finalize(id), Status::Defeated);

        let id2 = propose(&mut ctx, &mut c, 3, mission());
        cast(&mut ctx, &mut c, id2, accounts(3), Choice::For, 50); // below quorum of 100
        testing_env!(ctx.block_timestamp(1 + 9 * DAY_NS).build());
        assert_eq!(c.finalize(id2), Status::Defeated);
    }

    #[test]
    #[should_panic(expected = "not allowed for this tier")]
    fn function_call_only_tier_one() {
        let (mut ctx, mut c) = setup();
        let kind = ProposalKind::FunctionCall {
            receiver_id: accounts(1),
            actions: vec![ActionCall { method_name: "x".into(), args: Base64VecU8(vec![]), deposit: U128(0), gas: U64(1) }],
        };
        propose(&mut ctx, &mut c, 3, kind);
    }

    #[test]
    #[should_panic(expected = "Tier cannot be proposed")]
    fn tier_zero_is_frozen() {
        let (mut ctx, mut c) = setup();
        propose(&mut ctx, &mut c, 0, ProposalKind::Amend { article_json: "{}".into() });
    }

    #[test]
    #[should_panic(expected = "Tier 1 needs a supermajority")]
    fn tier_one_cannot_be_weakened() {
        let (mut ctx, mut c) = setup();
        testing_env!(ctx.predecessor_account_id(accounts(5)).build());
        c.set_tier_config(1, tier(14, 5_500));
    }

    #[test]
    fn zero_power_vote_is_dropped() {
        let (mut ctx, mut c) = setup();
        let id = propose(&mut ctx, &mut c, 3, mission());
        cast(&mut ctx, &mut c, id, accounts(3), Choice::For, 0);
        assert!(c.get_vote(id, accounts(3)).is_none());
    }
}
