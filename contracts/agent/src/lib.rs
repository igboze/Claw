//! Agent contract (Shade agent): attestation, approved code hashes, guardrails,
//! Chain Signatures and the treasury.
//!
//! Workers prove with a TEE attestation that they run an approved image (its
//! measurements, i.e. code hash). A valid worker can only *propose* an action; this
//! contract asks the constitution's `spend_check` first, and only then moves funds
//! from its own balance or requests a Chain Signature from the MPC network.
//! Every action carries a one-time `task_id` so several workers cannot double-act.
//!
//! Code hashes, PPIDs, attestation expiry and upgrades: Governor only (Tier 1).
//! Guardian (launch phase 4): may pause, never move funds, and only until
//! `guardian_expiry_block`, which is fixed at init and written into Tier 0.

use clawdao_common::{assert_caller, emit_event, Asset};
use near_sdk::json_types::{U128, U64};
use near_sdk::serde::Serialize;
use near_sdk::store::{IterableMap, IterableSet, LookupSet};
use near_sdk::{
    env, ext_contract, near, require, serde_json, AccountId, BorshStorageKey, Gas, NearToken,
    PanicOnDefault, Promise, PromiseError,
};
use shade_attestation::attestation::{AcceptedDstackAttestation, DstackAttestation};
use shade_attestation::measurements::{
    create_mock_full_measurements_hex, FullMeasurements, FullMeasurementsHex,
};
use shade_attestation::report_data::ReportData;
use shade_attestation::tcb_info::HexBytes;

mod hos;
pub use hos::*;

pub type Ppid = HexBytes<16>;

const GAS_SPEND_CHECK: Gas = Gas::from_tgas(20);
const GAS_ON_SPEND_CHECKED: Gas = Gas::from_tgas(150);
const GAS_FT_TRANSFER: Gas = Gas::from_tgas(15);
const GAS_MPC_SIGN: Gas = Gas::from_tgas(50);
const GAS_ON_ACTION_DONE: Gas = Gas::from_tgas(40);
const GAS_RECEIPT: Gas = Gas::from_tgas(15);
const STORAGE_BYTES_TO_REGISTER: u128 = 600;
const MAX_TASK_ID: usize = 128;

#[ext_contract(ext_constitution)]
#[allow(dead_code)]
trait Constitution {
    fn spend_check(&mut self, mission_id: String, asset: Asset, amount: U128, constitution_hash: String, chain_sig: bool) -> bool;
    fn refund_spend(&mut self, mission_id: String, amount: U128);
    fn record_receipt(
        &mut self,
        task_id: String,
        mission_id: String,
        constitution_hash: String,
        action_hash: String,
        reasoning_uri: String,
        amount: U128,
    ) -> u64;
}

#[ext_contract(ext_ft)]
#[allow(dead_code)]
trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum MpcPayload {
    Ecdsa(String),
    Eddsa(String),
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct SignRequest {
    pub payload_v2: MpcPayload,
    pub path: String,
    pub domain_id: u64,
}

#[ext_contract(ext_mpc)]
#[allow(dead_code)]
trait Mpc {
    fn sign(&self, request: SignRequest);
}

/// The only things a worker can ask for. Each maps to one asset and amount that
/// `spend_check` can verify.
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub enum AgentAction {
    /// Send NEAR from the treasury.
    NativeTransfer { receiver_id: AccountId, amount: U128 },
    /// Send a NEP-141 token from the treasury. The receiver must be registered.
    FtTransfer { token_id: AccountId, receiver_id: AccountId, amount: U128 },
    /// Sign a payload for another chain. `asset`/`value` declare what it moves and are
    /// checked against the mission; the mission must allow Chain Signatures.
    ChainSignature { path: String, payload_hex: String, key_type: KeyType, asset: Asset, value: U128 },
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, Debug)]
pub enum KeyType {
    Ecdsa,
    Eddsa,
}

impl AgentAction {
    fn asset_and_amount(&self) -> (Asset, u128, bool) {
        match self {
            AgentAction::NativeTransfer { amount, .. } => ("near".into(), amount.0, false),
            AgentAction::FtTransfer { token_id, amount, .. } => (token_id.to_string(), amount.0, false),
            AgentAction::ChainSignature { asset, value, .. } => (asset.clone(), value.0, true),
        }
    }

    fn hash(&self) -> String {
        hex::encode(env::sha256(near_sdk::borsh::to_vec(self).unwrap()))
    }
}

#[near(serializers = [borsh])]
pub struct Agent {
    pub measurements: FullMeasurementsHex,
    pub ppid: Ppid,
    pub valid_until_ms: u64,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct AgentView {
    pub account_id: AccountId,
    pub measurements: FullMeasurementsHex,
    pub ppid: Ppid,
    pub valid_until_ms: U64,
    pub valid: bool,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct ContractInfo {
    pub requires_tee: bool,
    pub attestation_expiration_time_ms: U64,
    pub governor_id: AccountId,
    pub constitution_id: AccountId,
    pub mpc_contract_id: AccountId,
    pub guardian_id: Option<AccountId>,
    pub guardian_expiry_block: U64,
    pub paused: bool,
    pub treasury_balance: NearToken,
}

#[near]
#[derive(BorshStorageKey)]
enum StorageKey {
    ApprovedMeasurements,
    ApprovedPpids,
    Agents,
    WhitelistedAgentsForLocal,
    UsedTasks,
    HosMandates,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    requires_tee: bool,
    attestation_expiration_time_ms: u64,
    governor_id: AccountId,
    constitution_id: AccountId,
    mpc_contract_id: AccountId,
    approved_measurements: IterableSet<FullMeasurementsHex>,
    approved_ppids: IterableSet<Ppid>,
    agents: IterableMap<AccountId, Agent>,
    whitelisted_agents_for_local: IterableSet<AccountId>,
    used_tasks: LookupSet<String>,
    guardian_id: Option<AccountId>,
    guardian_expiry_block: u64,
    paused: bool,
    hos: HosState,
}

#[near]
impl Contract {
    /// `requires_tee = false` is for local/sandbox runs only (mock TEE + whitelist).
    /// It is fixed at init, so a mainnet deploy with `true` can never be downgraded
    /// except by a Tier 1 upgrade.
    #[init]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        requires_tee: bool,
        attestation_expiration_time_ms: U64,
        governor_id: AccountId,
        constitution_id: AccountId,
        mpc_contract_id: AccountId,
        guardian_id: Option<AccountId>,
        guardian_expiry_block: U64,
    ) -> Self {
        Self {
            requires_tee,
            attestation_expiration_time_ms: attestation_expiration_time_ms.0,
            governor_id,
            constitution_id,
            mpc_contract_id,
            approved_measurements: IterableSet::new(StorageKey::ApprovedMeasurements),
            approved_ppids: IterableSet::new(StorageKey::ApprovedPpids),
            agents: IterableMap::new(StorageKey::Agents),
            whitelisted_agents_for_local: IterableSet::new(StorageKey::WhitelistedAgentsForLocal),
            used_tasks: LookupSet::new(StorageKey::UsedTasks),
            guardian_id,
            guardian_expiry_block: guardian_expiry_block.0,
            paused: false,
            hos: HosState::new(),
        }
    }

    /// Called by the worker itself (claw-bridge) with a fresh Dstack attestation.
    #[payable]
    pub fn register_agent(&mut self, attestation: DstackAttestation) -> bool {
        let predecessor = env::predecessor_account_id();
        if self.agents.get(&predecessor).is_none() {
            let cost = env::storage_byte_cost().checked_mul(STORAGE_BYTES_TO_REGISTER).unwrap();
            require!(env::attached_deposit() >= cost, "Attach deposit for storage");
        }
        let (measurements, ppid) = self.verify_attestation(attestation);
        let valid_until_ms = env::block_timestamp_ms() + self.attestation_expiration_time_ms;
        emit_event("agent_registered", &serde_json::json!({
            "account_id": predecessor, "measurements": measurements, "ppid": ppid,
            "valid_until_ms": U64(valid_until_ms),
        }));
        self.agents.insert(predecessor, Agent { measurements, ppid, valid_until_ms });
        true
    }

    /// The single entry point for value-moving actions.
    pub fn propose_action(
        &mut self,
        task_id: String,
        mission_id: String,
        constitution_hash: String,
        action: AgentAction,
        reasoning_uri: String,
    ) -> Promise {
        require!(!self.paused, "Agent is paused");
        self.require_valid_agent();
        require!(!task_id.is_empty() && task_id.len() <= MAX_TASK_ID, "Bad task_id");
        require!(self.used_tasks.insert(task_id.clone()), "task_id already used");
        let (asset, amount, chain_sig) = action.asset_and_amount();
        require!(amount > 0, "Zero-value action");
        emit_event("action_proposed", &serde_json::json!({
            "task_id": task_id, "mission_id": mission_id, "agent": env::predecessor_account_id(),
            "asset": asset, "amount": U128(amount), "action_hash": action.hash(),
        }));
        ext_constitution::ext(self.constitution_id.clone())
            .with_static_gas(GAS_SPEND_CHECK)
            .spend_check(mission_id.clone(), asset, U128(amount), constitution_hash.clone(), chain_sig)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_ON_SPEND_CHECKED)
                    .on_spend_checked(task_id, mission_id, constitution_hash, action, reasoning_uri),
            )
    }

    #[private]
    pub fn on_spend_checked(
        &mut self,
        task_id: String,
        mission_id: String,
        constitution_hash: String,
        action: AgentAction,
        reasoning_uri: String,
    ) -> Promise {
        let approved = matches!(
            env::promise_result_checked(0, 16).ok().and_then(|b| serde_json::from_slice::<bool>(&b).ok()),
            Some(true)
        );
        require!(approved, "spend_check rejected the action");
        let (_, amount, _) = action.asset_and_amount();
        let action_hash = action.hash();
        let exec = match &action {
            AgentAction::NativeTransfer { receiver_id, amount } => {
                Promise::new(receiver_id.clone()).transfer(NearToken::from_yoctonear(amount.0))
            }
            AgentAction::FtTransfer { token_id, receiver_id, amount } => ext_ft::ext(token_id.clone())
                .with_attached_deposit(NearToken::from_yoctonear(1))
                .with_static_gas(GAS_FT_TRANSFER)
                .ft_transfer(receiver_id.clone(), *amount, Some(format!("clawdao task {task_id}"))),
            AgentAction::ChainSignature { path, payload_hex, key_type, .. } => {
                let (payload_v2, domain_id) = match key_type {
                    KeyType::Ecdsa => (MpcPayload::Ecdsa(payload_hex.clone()), 0),
                    KeyType::Eddsa => (MpcPayload::Eddsa(payload_hex.clone()), 1),
                };
                ext_mpc::ext(self.mpc_contract_id.clone())
                    .with_static_gas(GAS_MPC_SIGN)
                    .with_attached_deposit(NearToken::from_yoctonear(1))
                    .sign(SignRequest { payload_v2, path: path.clone(), domain_id })
            }
        };
        exec.then(Self::ext(env::current_account_id()).with_static_gas(GAS_ON_ACTION_DONE).on_action_done(
            task_id,
            mission_id,
            constitution_hash,
            action_hash,
            U128(amount),
            reasoning_uri,
        ))
    }

    /// Records the Tier 4 receipt on success, refunds the mission budget on failure.
    /// Returns the MPC signature (if any) so the bridge can broadcast.
    #[private]
    pub fn on_action_done(
        &mut self,
        task_id: String,
        mission_id: String,
        constitution_hash: String,
        action_hash: String,
        amount: U128,
        reasoning_uri: String,
    ) -> Option<serde_json::Value> {
        match env::promise_result_checked(0, 4_096) {
            Err(PromiseError::Failed) => {
                ext_constitution::ext(self.constitution_id.clone())
                    .with_static_gas(GAS_RECEIPT)
                    .refund_spend(mission_id.clone(), amount)
                    .detach();
                emit_event("action_failed", &serde_json::json!({"task_id": task_id, "mission_id": mission_id}));
                None
            }
            result => {
                ext_constitution::ext(self.constitution_id.clone())
                    .with_static_gas(GAS_RECEIPT)
                    .record_receipt(task_id.clone(), mission_id, constitution_hash, action_hash, reasoning_uri, amount)
                    .detach();
                emit_event("action_executed", &serde_json::json!({"task_id": task_id}));
                result.ok().and_then(|b| serde_json::from_slice(&b).ok())
            }
        }
    }

    /// After broadcasting a Chain Signature transaction, the bridge links the foreign tx.
    pub fn post_receipt(&mut self, task_id: String, chain: String, tx_hash: String) {
        self.require_valid_agent();
        require!(self.used_tasks.contains(&task_id), "Unknown task_id");
        emit_event("foreign_tx", &serde_json::json!({"task_id": task_id, "chain": chain, "tx_hash": tx_hash}));
    }

    /// Heartbeat so the status page can show worker uptime.
    pub fn heartbeat(&mut self, loop_epoch: U64) {
        self.require_valid_agent();
        emit_event("heartbeat", &serde_json::json!({"agent": env::predecessor_account_id(), "loop_epoch": loop_epoch}));
    }

    // ---- guardian (phase 4) ----

    pub fn pause(&mut self) {
        let caller = env::predecessor_account_id();
        let guardian_ok = self.guardian_id.as_ref() == Some(&caller) && env::block_height() < self.guardian_expiry_block;
        require!(guardian_ok || caller == self.governor_id, "Only an active guardian or the Governor can pause");
        self.paused = true;
        emit_event("paused", &serde_json::json!({"by": caller}));
    }

    // ---- Governor only (Tier 1) ----

    pub fn unpause(&mut self) {
        self.only_governor();
        self.paused = false;
        emit_event("unpaused", &serde_json::json!({}));
    }

    pub fn approve_measurements(&mut self, measurements: FullMeasurementsHex) {
        self.only_governor();
        emit_event("measurements_approved", &measurements);
        self.approved_measurements.insert(measurements);
    }

    pub fn remove_measurements(&mut self, measurements: FullMeasurementsHex) {
        self.only_governor();
        emit_event("measurements_removed", &measurements);
        self.approved_measurements.remove(&measurements);
    }

    pub fn approve_ppids(&mut self, ppids: Vec<Ppid>) {
        self.only_governor();
        for p in ppids {
            self.approved_ppids.insert(p);
        }
    }

    pub fn remove_ppids(&mut self, ppids: Vec<Ppid>) {
        self.only_governor();
        for p in ppids {
            self.approved_ppids.remove(&p);
        }
    }

    pub fn remove_agent(&mut self, account_id: AccountId) {
        self.only_governor();
        self.agents.remove(&account_id);
        emit_event("agent_removed", &serde_json::json!({"account_id": account_id}));
    }

    pub fn set_attestation_expiration_time(&mut self, ms: U64) {
        self.only_governor();
        self.attestation_expiration_time_ms = ms.0;
    }

    /// Local mode only: let a non-TEE worker register with mock measurements.
    pub fn whitelist_agent_for_local(&mut self, account_id: AccountId) {
        self.only_governor();
        require!(!self.requires_tee, "Whitelist is disabled when TEE is required");
        self.whitelisted_agents_for_local.insert(account_id);
    }

    // ---- views ----

    pub fn get_contract_info(&self) -> ContractInfo {
        ContractInfo {
            requires_tee: self.requires_tee,
            attestation_expiration_time_ms: U64(self.attestation_expiration_time_ms),
            governor_id: self.governor_id.clone(),
            constitution_id: self.constitution_id.clone(),
            mpc_contract_id: self.mpc_contract_id.clone(),
            guardian_id: self.guardian_id.clone(),
            guardian_expiry_block: U64(self.guardian_expiry_block),
            paused: self.paused,
            treasury_balance: env::account_balance(),
        }
    }

    pub fn get_approved_measurements(&self) -> Vec<FullMeasurementsHex> {
        self.approved_measurements.iter().cloned().collect()
    }

    pub fn get_approved_ppids(&self) -> Vec<Ppid> {
        self.approved_ppids.iter().cloned().collect()
    }

    pub fn get_agents(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<AgentView> {
        self.agents
            .iter()
            .skip(from_index.unwrap_or(0) as usize)
            .take(limit.unwrap_or(50).min(100) as usize)
            .map(|(id, a)| self.view_agent(id, a))
            .collect()
    }

    pub fn get_agent(&self, account_id: AccountId) -> Option<AgentView> {
        self.agents.get(&account_id).map(|a| self.view_agent(&account_id, a))
    }

    pub fn is_whitelisted(&self, account_id: AccountId) -> bool {
        self.whitelisted_agents_for_local.contains(&account_id)
    }

    pub fn is_task_used(&self, task_id: String) -> bool {
        self.used_tasks.contains(&task_id)
    }
}

impl Contract {
    fn only_governor(&self) {
        assert_caller(&self.governor_id, "the Governor");
    }

    fn is_valid(&self, a: &Agent) -> bool {
        a.valid_until_ms > env::block_timestamp_ms()
            && self.approved_measurements.contains(&a.measurements)
            && self.approved_ppids.contains(&a.ppid)
    }

    fn view_agent(&self, id: &AccountId, a: &Agent) -> AgentView {
        AgentView {
            account_id: id.clone(),
            measurements: a.measurements.clone(),
            ppid: a.ppid.clone(),
            valid_until_ms: U64(a.valid_until_ms),
            valid: self.is_valid(a),
        }
    }

    fn require_valid_agent(&self) {
        let a = self.agents.get(&env::predecessor_account_id()).expect("Not a registered agent");
        require!(self.is_valid(a), "Agent attestation expired or code hash no longer approved");
    }

    fn verify_attestation(&self, attestation: DstackAttestation) -> (FullMeasurementsHex, Ppid) {
        if !self.requires_tee {
            require!(
                self.whitelisted_agents_for_local.contains(&env::predecessor_account_id()),
                "Agent needs to be whitelisted for local mode"
            );
            let mock = create_mock_full_measurements_hex();
            require!(self.approved_measurements.contains(&mock), "Mock measurements not approved");
            require!(self.approved_ppids.contains(&Ppid::default()), "Default PPID not approved");
            return (mock, Ppid::default());
        }
        // The worker key is derived inside the TEE; binding its implicit account id into
        // report_data proves this account's key lives in the attested image.
        let account = env::predecessor_account_id().to_string();
        require!(
            account.len() == 64 && account.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "Agent account must be an implicit account"
        );
        let mut report_data = [0u8; 64];
        report_data[..32].copy_from_slice(&hex::decode(&account).unwrap());
        let expected: Vec<FullMeasurements> = self.approved_measurements.iter().cloned().map(Into::into).collect();
        let ppids: Vec<Ppid> = self.approved_ppids.iter().cloned().collect();
        match attestation.verify(ReportData::from(report_data), env::block_timestamp_ms() / 1000, &expected, &ppids) {
            Ok(AcceptedDstackAttestation { measurements, ppid, .. }) => (measurements.into(), ppid),
            Err(e) => env::panic_str(&format!("Attestation verification failed: {e}")),
        }
    }
}

/// Tier 1 upgrade: the Governor calls `upgrade` with the raw wasm as input, then the
/// new code's `migrate` runs. Kept outside `#[near]` so the input is not JSON-parsed.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn upgrade() {
    env::setup_panic_hook();
    let state: Contract = env::state_read().expect("No state");
    assert_caller(&state.governor_id, "the Governor");
    let code = env::input().expect("No code");
    Promise::new(env::current_account_id())
        .deploy_contract(code)
        .function_call("migrate".to_string(), vec![], NearToken::from_yoctonear(0), Gas::from_tgas(50))
        .detach();
}

#[near]
impl Contract {
    /// Called after `upgrade`. State layout unchanged in v1.
    #[private]
    #[init(ignore_state)]
    pub fn migrate() -> Self {
        env::state_read().expect("No state")
    }
}

#[cfg(test)]
#[allow(unused_must_use)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    fn setup(ctx: &mut VMContextBuilder) -> Contract {
        testing_env!(ctx.block_height(10).predecessor_account_id(accounts(0)).build());
        let mut c = Contract::new(false, U64(1_000_000), accounts(0), accounts(1), "v1.signer".parse().unwrap(), Some(accounts(2)), U64(100));
        c.approve_measurements(create_mock_full_measurements_hex());
        c.approve_ppids(vec![Ppid::default()]);
        c.whitelist_agent_for_local(accounts(3));
        c
    }

    fn register(ctx: &mut VMContextBuilder, c: &mut Contract) {
        testing_env!(ctx.predecessor_account_id(accounts(3)).attached_deposit(NearToken::from_near(1)).build());
        c.register_agent(shade_attestation::attestation::create_mock_dstack_attestation());
    }

    fn action() -> AgentAction {
        AgentAction::NativeTransfer { receiver_id: accounts(4), amount: U128(5) }
    }

    #[test]
    fn local_agent_registers_and_proposes() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        register(&mut ctx, &mut c);
        assert!(c.get_agent(accounts(3)).unwrap().valid);
        testing_env!(ctx.attached_deposit(NearToken::from_yoctonear(0)).build());
        c.propose_action("t1".into(), "m1".into(), "h".into(), action(), "ipfs://r".into());
        assert!(c.is_task_used("t1".into()));
    }

    #[test]
    #[should_panic(expected = "task_id already used")]
    fn task_id_once() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        register(&mut ctx, &mut c);
        c.propose_action("t1".into(), "m1".into(), "h".into(), action(), "r".into());
        c.propose_action("t1".into(), "m1".into(), "h".into(), action(), "r".into());
    }

    #[test]
    #[should_panic(expected = "Not a registered agent")]
    fn strangers_cannot_propose() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        testing_env!(ctx.predecessor_account_id(accounts(5)).build());
        c.propose_action("t1".into(), "m1".into(), "h".into(), action(), "r".into());
    }

    #[test]
    #[should_panic(expected = "code hash no longer approved")]
    fn revoking_code_hash_disables_agent() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        register(&mut ctx, &mut c);
        testing_env!(ctx.predecessor_account_id(accounts(0)).build());
        c.remove_measurements(create_mock_full_measurements_hex());
        testing_env!(ctx.predecessor_account_id(accounts(3)).build());
        c.propose_action("t1".into(), "m1".into(), "h".into(), action(), "r".into());
    }

    #[test]
    fn guardian_pause_until_expiry() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        testing_env!(ctx.predecessor_account_id(accounts(2)).block_height(50).build());
        c.pause();
        assert!(c.get_contract_info().paused);
        testing_env!(ctx.predecessor_account_id(accounts(0)).build());
        c.unpause();
        assert!(!c.get_contract_info().paused);
    }

    #[test]
    #[should_panic(expected = "Only an active guardian")]
    fn guardian_expires() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        testing_env!(ctx.predecessor_account_id(accounts(2)).block_height(100).build());
        c.pause();
    }

    #[test]
    #[should_panic(expected = "Agent is paused")]
    fn paused_blocks_actions() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        register(&mut ctx, &mut c);
        testing_env!(ctx.predecessor_account_id(accounts(0)).build());
        c.pause();
        testing_env!(ctx.predecessor_account_id(accounts(3)).build());
        c.propose_action("t1".into(), "m1".into(), "h".into(), action(), "r".into());
    }

    #[test]
    #[should_panic(expected = "Only the Governor")]
    fn code_hashes_governor_only() {
        let mut ctx = VMContextBuilder::new();
        let mut c = setup(&mut ctx);
        testing_env!(ctx.predecessor_account_id(accounts(3)).build());
        c.approve_measurements(create_mock_full_measurements_hex());
    }
}

#[cfg(test)]
mod fixtures {
    /// `UPDATE_FIXTURES=1 cargo test -p claw-agent write_fixtures` refreshes the JSON used by integration tests.
    #[test]
    fn write_fixtures() {
        if std::env::var("UPDATE_FIXTURES").is_err() {
            return;
        }
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/tests/");
        let att = shade_attestation::attestation::create_mock_dstack_attestation();
        std::fs::write(format!("{dir}mock_attestation.json"), near_sdk::serde_json::to_string_pretty(&att).unwrap()).unwrap();
        let m = shade_attestation::measurements::create_mock_full_measurements_hex();
        std::fs::write(format!("{dir}mock_measurements.json"), near_sdk::serde_json::to_string_pretty(&m).unwrap()).unwrap();
    }
}
