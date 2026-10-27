//! Constitution: the tiered rules the agent lives by, and the guardrail that checks
//! every spend before a signature is requested.
//!
//! - Tier 0: core articles, written once at init, never changeable.
//! - Tier 1: constitution articles. Governor only (Tier 1 vote).
//! - Tier 2: operating articles and parameters (loop interval, daily caps). Governor only.
//! - Tier 3: missions with budget, deadline and metric. Governor only.
//! - Tier 4: receipts and earnings. Written only by the agent contract: a receipt after
//!   every action that spent, an earning after every Agent Market job a buyer accepted.
//!
//! Work missions: a Tier 3 mission with `work` terms lets the agent take paid jobs on the
//! NEAR AI Agent Market (tags, minimum price, how many at once). Work missions never spend:
//! their budget must be zero, so `spend_check` always rejects them.
//!
//! `hash` is sha256 over tiers 0 to 3 (articles, params, mission definitions). It changes
//! whenever governance changes anything the agent must obey, and `spend_check` rejects
//! actions planned against a stale hash.

use clawdao_common::{
    assert_caller, emit_event, Asset, Tier, DAY_NS, TIER_CONSTITUTION, TIER_CORE, TIER_PARAMS,
};
use near_sdk::json_types::{U128, U64};
use near_sdk::serde::Serialize;
use near_sdk::store::{IterableMap, Lazy, LookupMap, Vector};
use near_sdk::{env, near, require, serde_json, AccountId, BorshStorageKey, PanicOnDefault};

const MAX_ARTICLE_BYTES: usize = 8_192;
const MAX_ID: usize = 64;

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub struct Article {
    pub id: String,
    pub title: String,
    pub body: String,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct TieredArticle {
    pub tier: Tier,
    #[serde(flatten)]
    pub article: Article,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct Params {
    /// How often the agent loop runs.
    pub loop_interval_minutes: u32,
    /// Max total spend per asset per UTC day across all missions. An asset with no
    /// entry has a cap of zero: the agent cannot spend it at all.
    pub daily_caps: Vec<(Asset, U128)>,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct MissionInput {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub asset: Asset,
    pub budget: U128,
    pub max_per_action: U128,
    /// Nanosecond timestamp after which the mission cannot spend.
    pub deadline: U64,
    /// How success is measured, in plain words (e.g. "unique bounty payees").
    pub metric: String,
    /// May this mission request Chain Signatures for other chains?
    pub allow_chain_sig: bool,
    /// Present for Agent Market work missions (earn, never spend).
    #[serde(default)]
    pub work: Option<WorkTerms>,
}

/// What jobs the agent may take on the NEAR AI Agent Market under one work mission.
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub struct WorkTerms {
    /// Job tags it may bid on (at least one must match).
    pub tags: Vec<String>,
    /// Lowest price it may bid, in US cents (USD and USDC are 1:1 on Agent Market).
    pub min_price_cents: u64,
    /// Most jobs open at once (pending bids plus in-progress assignments).
    pub max_open: u32,
    /// Accepted payout currencies, e.g. ["USDC", "USD"].
    pub currencies: Vec<String>,
}

/// An Agent Market job a buyer accepted. Reported by an attested worker; the assignment id
/// lets anyone check it against the marketplace.
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct Earning {
    pub id: u64,
    pub mission_id: String,
    pub assignment_id: String,
    pub job_id: String,
    /// Escrow amount as the marketplace states it (before its 5% fee).
    pub amount: String,
    pub amount_cents: u64,
    pub currency: String,
    pub deliverable_hash: Option<String>,
    pub block_height: U64,
    pub timestamp: U64,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct Mission {
    #[serde(flatten)]
    pub def: MissionInput,
    pub spent: U128,
    pub cancelled: bool,
}

#[near(serializers = [json])]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MissionStatus {
    Active,
    Exhausted,
    Expired,
    Cancelled,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct MissionView {
    #[serde(flatten)]
    pub mission: Mission,
    pub status: MissionStatus,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct Receipt {
    pub id: u64,
    pub task_id: String,
    pub mission_id: String,
    pub constitution_hash: String,
    pub action_hash: String,
    pub reasoning_uri: String,
    pub amount: U128,
    pub block_height: U64,
    pub timestamp: U64,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct ConstitutionView {
    pub version: u64,
    pub hash: String,
    pub articles: Vec<TieredArticle>,
    pub params: Params,
}

#[near]
#[derive(BorshStorageKey)]
enum StorageKey {
    Articles,
    Missions,
    DailySpent,
    Receipts,
    Earnings,
    EarnedAssignments,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    governor_id: AccountId,
    agent_id: AccountId,
    version: u64,
    hash: String,
    articles: Lazy<Vec<TieredArticle>>,
    params: Params,
    missions: IterableMap<String, Mission>,
    daily_spent: LookupMap<(Asset, u64), u128>,
    receipts: Vector<Receipt>,
    earnings: Vector<Earning>,
    earned_assignments: near_sdk::store::LookupSet<String>,
}

#[near]
impl Contract {
    /// Genesis. `articles` holds the Tier 0 to 2 launch text from constitution/genesis.md.
    #[init]
    pub fn new(governor_id: AccountId, agent_id: AccountId, articles: Vec<TieredArticle>, params: Params) -> Self {
        for a in &articles {
            require!(a.tier <= TIER_PARAMS, "Genesis articles must be tiers 0 to 2");
            validate_article(&a.article);
        }
        require!(articles.iter().any(|a| a.tier == TIER_CORE), "Genesis needs Tier 0");
        let mut this = Self {
            governor_id,
            agent_id,
            version: 0,
            hash: String::new(),
            articles: Lazy::new(StorageKey::Articles, articles),
            params,
            missions: IterableMap::new(StorageKey::Missions),
            daily_spent: LookupMap::new(StorageKey::DailySpent),
            receipts: Vector::new(StorageKey::Receipts),
            earnings: Vector::new(StorageKey::Earnings),
            earned_assignments: near_sdk::store::LookupSet::new(StorageKey::EarnedAssignments),
        };
        this.bump("genesis");
        this
    }

    // ---- Governor only (after the timelock) ----

    /// Add or replace an article in tier 1 or 2. Tier 0 is frozen.
    pub fn amend(&mut self, tier: Tier, article: Article) {
        assert_caller(&self.governor_id, "the Governor");
        require!(tier == TIER_CONSTITUTION || tier == TIER_PARAMS, "Only tiers 1 and 2 can be amended");
        validate_article(&article);
        let articles = self.articles.get_mut();
        match articles.iter_mut().find(|a| a.article.id == article.id) {
            Some(existing) => {
                require!(existing.tier == tier, "Article belongs to another tier");
                existing.article = article.clone();
            }
            None => articles.push(TieredArticle { tier, article: article.clone() }),
        }
        emit_event("article_amended", &serde_json::json!({"tier": tier, "article": article}));
        self.bump("amend");
    }

    pub fn remove_article(&mut self, tier: Tier, article_id: String) {
        assert_caller(&self.governor_id, "the Governor");
        require!(tier == TIER_CONSTITUTION || tier == TIER_PARAMS, "Only tiers 1 and 2 can be amended");
        let articles = self.articles.get_mut();
        let before = articles.len();
        articles.retain(|a| !(a.tier == tier && a.article.id == article_id));
        require!(articles.len() < before, "No such article in that tier");
        emit_event("article_removed", &serde_json::json!({"tier": tier, "article_id": article_id}));
        self.bump("remove_article");
    }

    pub fn set_params(&mut self, params: Params) {
        assert_caller(&self.governor_id, "the Governor");
        require!(params.loop_interval_minutes >= 1, "Loop interval must be at least a minute");
        self.params = params.clone();
        emit_event("params_set", &params);
        self.bump("set_params");
    }

    /// Create or update a mission. Updating keeps what was already spent.
    pub fn set_mission(&mut self, mission: MissionInput) {
        assert_caller(&self.governor_id, "the Governor");
        require!(!mission.id.is_empty() && mission.id.len() <= MAX_ID, "Bad mission id");
        require!(mission.max_per_action.0 <= mission.budget.0, "max_per_action above budget");
        if let Some(w) = &mission.work {
            require!(mission.budget.0 == 0 && !mission.allow_chain_sig, "Work missions earn and never spend: budget must be 0");
            require!(!w.tags.is_empty() && w.tags.len() <= 10, "Work missions need 1 to 10 tags");
            require!(w.max_open >= 1 && w.max_open <= 20, "max_open must be 1 to 20");
            require!(!w.currencies.is_empty(), "Name at least one currency");
        }
        let spent = self.missions.get(&mission.id).map(|m| m.spent).unwrap_or(U128(0));
        require!(mission.budget.0 >= spent.0, "Budget below amount already spent");
        emit_event("mission_set", &mission);
        self.missions.insert(mission.id.clone(), Mission { def: mission, spent, cancelled: false });
        self.bump("set_mission");
    }

    pub fn cancel_mission(&mut self, mission_id: String) {
        assert_caller(&self.governor_id, "the Governor");
        let m = self.missions.get_mut(&mission_id).expect("No mission");
        m.cancelled = true;
        emit_event("mission_cancelled", &serde_json::json!({"mission_id": mission_id}));
        self.bump("cancel_mission");
    }

    // ---- Agent contract only ----

    /// Guardrail run before every signature request. Panics (and so blocks the action)
    /// if the hash is stale, the mission is not active, or any cap would be exceeded.
    /// On success the amount is counted as spent.
    pub fn spend_check(&mut self, mission_id: String, asset: Asset, amount: U128, constitution_hash: String, chain_sig: bool) -> bool {
        assert_caller(&self.agent_id, "the agent contract");
        require!(constitution_hash == self.hash, "Stale constitution hash");
        let now = env::block_timestamp();
        let m = self.missions.get(&mission_id).cloned().expect("No mission");
        require!(status_of(&m, now) == MissionStatus::Active, "Mission not active");
        require!(m.def.asset == asset, "Asset does not match mission");
        require!(!chain_sig || m.def.allow_chain_sig, "Mission may not use Chain Signatures");
        require!(amount.0 <= m.def.max_per_action.0, "Above per-action cap");
        let new_spent = m.spent.0.checked_add(amount.0).expect("overflow");
        require!(new_spent <= m.def.budget.0, "Over mission budget");
        let cap = self.daily_cap(&asset);
        let day_key = (asset.clone(), now / DAY_NS);
        let day_spent = self.daily_spent.get(&day_key).copied().unwrap_or(0) + amount.0;
        require!(day_spent <= cap, "Over daily cap");

        self.daily_spent.insert(day_key, day_spent);
        self.missions.get_mut(&mission_id).unwrap().spent = U128(new_spent);
        emit_event("spend_checked", &serde_json::json!({
            "mission_id": mission_id, "asset": asset, "amount": amount, "mission_spent": U128(new_spent),
        }));
        true
    }

    /// Give back budget when an approved action failed downstream.
    pub fn refund_spend(&mut self, mission_id: String, amount: U128) {
        assert_caller(&self.agent_id, "the agent contract");
        let m = self.missions.get_mut(&mission_id).expect("No mission");
        m.spent = U128(m.spent.0.saturating_sub(amount.0));
        let day_key = (m.def.asset.clone(), env::block_timestamp() / DAY_NS);
        if let Some(d) = self.daily_spent.get_mut(&day_key) {
            *d = d.saturating_sub(amount.0);
        }
        emit_event("spend_refunded", &serde_json::json!({"mission_id": mission_id, "amount": amount}));
    }

    /// Tier 4: the public audit trail.
    pub fn record_receipt(
        &mut self,
        task_id: String,
        mission_id: String,
        constitution_hash: String,
        action_hash: String,
        reasoning_uri: String,
        amount: U128,
    ) -> u64 {
        assert_caller(&self.agent_id, "the agent contract");
        let id = self.receipts.len() as u64;
        let receipt = Receipt {
            id,
            task_id,
            mission_id,
            constitution_hash,
            action_hash,
            reasoning_uri,
            amount,
            block_height: U64(env::block_height()),
            timestamp: U64(env::block_timestamp()),
        };
        emit_event("receipt", &receipt);
        self.receipts.push(receipt);
        id
    }

    // ---- views ----

    pub fn get_constitution(&self) -> ConstitutionView {
        ConstitutionView {
            version: self.version,
            hash: self.hash.clone(),
            articles: self.articles.get().clone(),
            params: self.params.clone(),
        }
    }

    pub fn get_hash(&self) -> (u64, String) {
        (self.version, self.hash.clone())
    }

    pub fn get_mission(&self, mission_id: String) -> Option<MissionView> {
        let now = env::block_timestamp();
        self.missions.get(&mission_id).map(|m| MissionView { status: status_of(m, now), mission: m.clone() })
    }

    pub fn get_missions(&self) -> Vec<MissionView> {
        let now = env::block_timestamp();
        self.missions.values().map(|m| MissionView { status: status_of(m, now), mission: m.clone() }).collect()
    }

    pub fn get_active_missions(&self) -> Vec<MissionView> {
        self.get_missions().into_iter().filter(|m| m.status == MissionStatus::Active).collect()
    }

    pub fn get_receipts(&self, from_index: Option<u64>, limit: Option<u64>) -> Vec<Receipt> {
        let from = from_index.unwrap_or(0) as u32;
        let limit = limit.unwrap_or(50).min(100) as u32;
        (from..self.receipts.len().min(from.saturating_add(limit)))
            .filter_map(|i| self.receipts.get(i).cloned())
            .collect()
    }

    /// Tier 4: an Agent Market assignment the buyer accepted, once per assignment.
    #[allow(clippy::too_many_arguments)]
    pub fn record_earning(
        &mut self,
        mission_id: String,
        assignment_id: String,
        job_id: String,
        amount: String,
        amount_cents: u64,
        currency: String,
        deliverable_hash: Option<String>,
    ) -> u64 {
        assert_caller(&self.agent_id, "the agent contract");
        let m = self.missions.get(&mission_id).expect("No mission");
        let w = m.def.work.as_ref().expect("Not a work mission");
        require!(w.currencies.contains(&currency), "Currency not allowed by the mission");
        require!(self.earned_assignments.insert(assignment_id.clone()), "Earning already recorded");
        let id = self.earnings.len() as u64;
        let e = Earning {
            id,
            mission_id,
            assignment_id,
            job_id,
            amount,
            amount_cents,
            currency,
            deliverable_hash,
            block_height: U64(env::block_height()),
            timestamp: U64(env::block_timestamp()),
        };
        emit_event("earning", &e);
        self.earnings.push(e);
        id
    }

    pub fn get_earnings(&self, from_index: Option<u64>, limit: Option<u64>) -> Vec<Earning> {
        let from = from_index.unwrap_or(0) as u32;
        let limit = limit.unwrap_or(50).min(100) as u32;
        (from..self.earnings.len().min(from.saturating_add(limit)))
            .filter_map(|i| self.earnings.get(i).cloned())
            .collect()
    }

    pub fn is_earning_recorded(&self, assignment_id: String) -> bool {
        self.earned_assignments.contains(&assignment_id)
    }

    pub fn receipt_count(&self) -> u64 {
        self.receipts.len() as u64
    }

    pub fn get_daily_spent(&self, asset: Asset) -> (U128, U128) {
        let spent = self.daily_spent.get(&(asset.clone(), env::block_timestamp() / DAY_NS)).copied().unwrap_or(0);
        (U128(spent), U128(self.daily_cap(&asset)))
    }

    pub fn get_config(&self) -> (AccountId, AccountId) {
        (self.governor_id.clone(), self.agent_id.clone())
    }
}

impl Contract {
    fn daily_cap(&self, asset: &Asset) -> u128 {
        self.params.daily_caps.iter().find(|(a, _)| a == asset).map(|(_, c)| c.0).unwrap_or(0)
    }

    /// Recompute the hash over tiers 0 to 3 and increment the version.
    fn bump(&mut self, reason: &str) {
        let mut articles = self.articles.get().clone();
        articles.sort_by(|a, b| (a.tier, &a.article.id).cmp(&(b.tier, &b.article.id)));
        let mut missions: Vec<(MissionInput, bool)> =
            self.missions.values().map(|m| (m.def.clone(), m.cancelled)).collect();
        missions.sort_by(|a, b| a.0.id.cmp(&b.0.id));
        let canonical = near_sdk::borsh::to_vec(&(&articles, &self.params, &missions)).unwrap();
        self.hash = hex::encode(env::sha256(&canonical));
        self.version += 1;
        emit_event("constitution_updated", &serde_json::json!({
            "version": self.version, "hash": self.hash, "reason": reason,
        }));
    }
}

fn status_of(m: &Mission, now: u64) -> MissionStatus {
    if m.cancelled {
        MissionStatus::Cancelled
    } else if now >= m.def.deadline.0 {
        MissionStatus::Expired
    } else if m.def.work.is_none() && m.spent.0 >= m.def.budget.0 {
        MissionStatus::Exhausted
    } else {
        MissionStatus::Active
    }
}

fn validate_article(a: &Article) {
    require!(!a.id.is_empty() && a.id.len() <= MAX_ID, "Bad article id");
    require!(a.title.len() + a.body.len() <= MAX_ARTICLE_BYTES, "Article too long");
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    const GOV: usize = 0;
    const AGENT: usize = 1;

    fn art(id: &str, tier: Tier) -> TieredArticle {
        TieredArticle { tier, article: Article { id: id.into(), title: id.into(), body: "text".into() } }
    }

    fn setup() -> (VMContextBuilder, Contract) {
        let mut ctx = VMContextBuilder::new();
        testing_env!(ctx.block_timestamp(DAY_NS / 2).build());
        let c = Contract::new(
            accounts(GOV),
            accounts(AGENT),
            vec![art("core", 0), art("purpose", 1)],
            Params { loop_interval_minutes: 10, daily_caps: vec![("near".into(), U128(1_000))] },
        );
        (ctx, c)
    }

    fn mission(id: &str) -> MissionInput {
        MissionInput {
            id: id.into(),
            title: "t".into(),
            goal: "g".into(),
            asset: "near".into(),
            budget: U128(500),
            max_per_action: U128(200),
            deadline: U64(10 * DAY_NS),
            metric: "m".into(),
            allow_chain_sig: false,
            work: None,
        }
    }

    fn work_mission(id: &str) -> MissionInput {
        MissionInput {
            budget: U128(0),
            max_per_action: U128(0),
            work: Some(WorkTerms {
                tags: vec!["research".into()],
                min_price_cents: 500,
                max_open: 3,
                currencies: vec!["USDC".into()],
            }),
            ..mission(id)
        }
    }

    #[test]
    fn work_mission_stays_active_and_records_earnings_once() {
        let (mut ctx, mut c) = setup();
        as_gov(&mut ctx);
        c.set_mission(work_mission("w1"));
        assert_eq!(c.get_mission("w1".into()).unwrap().status, MissionStatus::Active);
        as_agent(&mut ctx);
        let id = c.record_earning("w1".into(), "a-1".into(), "j-1".into(), "12.500000".into(), 1250, "USDC".into(), None);
        assert_eq!(id, 0);
        assert!(c.is_earning_recorded("a-1".into()));
        assert_eq!(c.get_earnings(None, None)[0].amount_cents, 1250);
    }

    #[test]
    #[should_panic(expected = "Earning already recorded")]
    fn earning_dedupe() {
        let (mut ctx, mut c) = setup();
        as_gov(&mut ctx);
        c.set_mission(work_mission("w1"));
        as_agent(&mut ctx);
        c.record_earning("w1".into(), "a-1".into(), "j".into(), "1".into(), 100, "USDC".into(), None);
        c.record_earning("w1".into(), "a-1".into(), "j".into(), "1".into(), 100, "USDC".into(), None);
    }

    #[test]
    #[should_panic(expected = "budget must be 0")]
    fn work_missions_cannot_spend() {
        let (mut ctx, mut c) = setup();
        as_gov(&mut ctx);
        c.set_mission(MissionInput { budget: U128(10), max_per_action: U128(1), ..work_mission("w1") });
    }

    #[test]
    #[should_panic(expected = "Not a work mission")]
    fn earnings_need_work_mission() {
        let (mut ctx, mut c) = setup();
        as_gov(&mut ctx);
        c.set_mission(mission("m1"));
        as_agent(&mut ctx);
        c.record_earning("m1".into(), "a".into(), "j".into(), "1".into(), 100, "USDC".into(), None);
    }

    #[test]
    #[should_panic(expected = "Only the agent contract")]
    fn earnings_agent_only() {
        let (mut ctx, mut c) = setup();
        as_gov(&mut ctx);
        c.set_mission(work_mission("w1"));
        c.record_earning("w1".into(), "a".into(), "j".into(), "1".into(), 100, "USDC".into(), None);
    }

    fn as_gov(ctx: &mut VMContextBuilder) {
        testing_env!(ctx.predecessor_account_id(accounts(GOV)).build());
    }
    fn as_agent(ctx: &mut VMContextBuilder) {
        testing_env!(ctx.predecessor_account_id(accounts(AGENT)).build());
    }

    #[test]
    fn hash_changes_on_amend_and_mission() {
        let (mut ctx, mut c) = setup();
        let h0 = c.get_hash();
        as_gov(&mut ctx);
        c.amend(1, Article { id: "purpose".into(), title: "p".into(), body: "new".into() });
        let h1 = c.get_hash();
        assert_ne!(h0.1, h1.1);
        assert_eq!(h1.0, h0.0 + 1);
        c.set_mission(mission("m1"));
        assert_ne!(c.get_hash().1, h1.1);
    }

    #[test]
    #[should_panic(expected = "Only tiers 1 and 2 can be amended")]
    fn tier_zero_frozen() {
        let (mut ctx, mut c) = setup();
        as_gov(&mut ctx);
        c.amend(0, Article { id: "core".into(), title: "x".into(), body: "x".into() });
    }

    #[test]
    #[should_panic(expected = "Only the Governor")]
    fn amend_governor_only() {
        let (mut ctx, mut c) = setup();
        as_agent(&mut ctx);
        c.amend(1, Article { id: "x".into(), title: "x".into(), body: "x".into() });
    }

    #[test]
    fn spend_check_counts_spend() {
        let (mut ctx, mut c) = setup();
        as_gov(&mut ctx);
        c.set_mission(mission("m1"));
        let hash = c.get_hash().1;
        as_agent(&mut ctx);
        assert!(c.spend_check("m1".into(), "near".into(), U128(150), hash.clone(), false));
        assert_eq!(c.get_mission("m1".into()).unwrap().mission.spent.0, 150);
        assert_eq!(c.get_daily_spent("near".into()).0 .0, 150);
    }

    #[test]
    #[should_panic(expected = "Stale constitution hash")]
    fn stale_hash_rejected() {
        let (mut ctx, mut c) = setup();
        as_gov(&mut ctx);
        c.set_mission(mission("m1"));
        let old = c.get_hash().1;
        c.amend(1, Article { id: "purpose".into(), title: "p".into(), body: "changed".into() });
        as_agent(&mut ctx);
        c.spend_check("m1".into(), "near".into(), U128(1), old, false);
    }

    #[test]
    #[should_panic(expected = "Over mission budget")]
    fn budget_enforced() {
        let (mut ctx, mut c) = setup();
        as_gov(&mut ctx);
        c.set_mission(mission("m1"));
        let h = c.get_hash().1;
        as_agent(&mut ctx);
        for _ in 0..3 {
            c.spend_check("m1".into(), "near".into(), U128(200), h.clone(), false);
        }
    }

    #[test]
    #[should_panic(expected = "Over daily cap")]
    fn daily_cap_across_missions() {
        let (mut ctx, mut c) = setup();
        as_gov(&mut ctx);
        let big = |id: &str| MissionInput { budget: U128(5_000), max_per_action: U128(600), ..mission(id) };
        c.set_mission(big("a"));
        c.set_mission(big("b"));
        let h = c.get_hash().1;
        as_agent(&mut ctx);
        c.spend_check("a".into(), "near".into(), U128(600), h.clone(), false);
        c.spend_check("b".into(), "near".into(), U128(600), h, false);
    }

    #[test]
    #[should_panic(expected = "Over daily cap")]
    fn uncapped_asset_cannot_be_spent() {
        let (mut ctx, mut c) = setup();
        as_gov(&mut ctx);
        c.set_mission(MissionInput { asset: "usdc.near".into(), ..mission("m1") });
        let h = c.get_hash().1;
        as_agent(&mut ctx);
        c.spend_check("m1".into(), "usdc.near".into(), U128(1), h, false);
    }

    #[test]
    #[should_panic(expected = "Mission not active")]
    fn expired_mission_rejected() {
        let (mut ctx, mut c) = setup();
        as_gov(&mut ctx);
        c.set_mission(mission("m1"));
        let h = c.get_hash().1;
        testing_env!(ctx.predecessor_account_id(accounts(AGENT)).block_timestamp(11 * DAY_NS).build());
        c.spend_check("m1".into(), "near".into(), U128(1), h, false);
    }

    #[test]
    #[should_panic(expected = "may not use Chain Signatures")]
    fn chain_sig_needs_permission() {
        let (mut ctx, mut c) = setup();
        as_gov(&mut ctx);
        c.set_mission(mission("m1"));
        let h = c.get_hash().1;
        as_agent(&mut ctx);
        c.spend_check("m1".into(), "near".into(), U128(1), h, true);
    }

    #[test]
    fn receipts_and_refunds() {
        let (mut ctx, mut c) = setup();
        as_gov(&mut ctx);
        c.set_mission(mission("m1"));
        let h = c.get_hash().1;
        as_agent(&mut ctx);
        c.spend_check("m1".into(), "near".into(), U128(100), h.clone(), false);
        c.refund_spend("m1".into(), U128(100));
        assert_eq!(c.get_mission("m1".into()).unwrap().mission.spent.0, 0);
        let id = c.record_receipt("t1".into(), "m1".into(), h, "ab".into(), "ipfs://log".into(), U128(100));
        assert_eq!(id, 0);
        assert_eq!(c.get_receipts(None, None).len(), 1);
    }
}
