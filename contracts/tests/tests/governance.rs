//! End-to-end governance paths on a NEAR sandbox.
//!
//! Needs the contract wasm files: run `contracts/build.sh` (or
//! `cargo near build non-reproducible-wasm` in each contract) first, then
//! `cargo test -p clawdao-integration-tests -- --ignored`.

use near_sdk::json_types::U128;
use near_sdk::NearToken;
use near_workspaces::network::Sandbox;
use near_workspaces::{Account, Contract, Worker};
use serde_json::{json, Value};
use std::path::PathBuf;

const DAY_NS: u64 = 86_400 * 1_000_000_000;
const SUPPLY: u128 = 1_000_000_000 * 10u128.pow(24);

fn wasm(name: &str) -> Vec<u8> {
    let dir = std::env::var("WASM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/near"));
    for p in [dir.join(name).join(format!("{name}.wasm")), dir.join(format!("{name}.wasm"))] {
        if let Ok(bytes) = std::fs::read(&p) {
            return bytes;
        }
    }
    panic!("{name}.wasm not found under {dir:?}; run contracts/build.sh first");
}

struct Dao {
    worker: Worker<Sandbox>,
    root: Account,
    token: Contract,
    ve: Contract,
    gov: Contract,
    constitution: Contract,
    agent: Contract,
}

async fn sub(root: &Account, name: &str, near: u128) -> anyhow::Result<Account> {
    Ok(root.create_subaccount(name).initial_balance(NearToken::from_near(near)).transact().await?.into_result()?)
}

/// Tier settings: Tier 1 must keep >= 7 day timelock (enforced on-chain); tiers 2-3 are short for tests.
fn tiers() -> Value {
    let t = |period: u64, lock: u64, bps: u16| json!({"voting_period_ns": period.to_string(), "timelock_ns": lock.to_string(), "quorum": "1000", "threshold_bps": bps});
    json!([[1, t(60_000_000_000, 7 * DAY_NS, 6_667)], [2, t(60_000_000_000, 60_000_000_000, 6_000)], [3, t(60_000_000_000, 60_000_000_000, 5_001)]])
}

async fn deploy() -> anyhow::Result<Dao> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let token = sub(&root, "claw", 20).await?.deploy(&wasm("claw_token")).await?.into_result()?;
    let ve = sub(&root, "ve", 20).await?.deploy(&wasm("claw_ve")).await?.into_result()?;
    let gov = sub(&root, "gov", 50).await?.deploy(&wasm("claw_governor")).await?.into_result()?;
    let constitution = sub(&root, "constitution", 20).await?.deploy(&wasm("claw_constitution")).await?.into_result()?;
    let agent = sub(&root, "agent", 100).await?.deploy(&wasm("claw_agent")).await?.into_result()?;

    token.call("new").args_json(json!({
        "distribution_id": root.id(), "total_supply": U128(SUPPLY),
        "metadata": {"spec": "ft-1.0.0", "name": "IronClaw Coin", "symbol": "CLAW", "decimals": 24},
    })).transact().await?.into_result()?;
    ve.call("new").args_json(json!({
        "token_id": token.id(), "governor_id": gov.id(),
        "params": {"min_months": 1, "max_months": 24, "min_lock_amount": "1", "max_power_per_account": U128(SUPPLY)},
    })).transact().await?.into_result()?;
    gov.call("new").args_json(json!({
        "ve_id": ve.id(), "constitution_id": constitution.id(), "agent_id": agent.id(), "hos_voting_period_ns": "60000000000", "proposal_bond": "1000", "tiers": tiers(),
    })).transact().await?.into_result()?;
    constitution.call("new").args_json(json!({
        "governor_id": gov.id(), "agent_id": agent.id(),
        "articles": [{"tier": 0, "id": "core", "title": "Core", "body": "No human keys."}],
        "params": {"loop_interval_minutes": 10, "daily_caps": [["near", "5000000000000000000000000"]]},
    })).transact().await?.into_result()?;
    agent.call("new").args_json(json!({
        "requires_tee": false, "attestation_expiration_time_ms": "604800000", "governor_id": gov.id(),
        "constitution_id": constitution.id(), "mpc_contract_id": "v1.signer-prod.testnet",
        "guardian_id": null, "guardian_expiry_block": "0",
    })).transact().await?.into_result()?;
    Ok(Dao { worker, root, token, ve, gov, constitution, agent })
}

impl Dao {
    async fn lock(&self, who: &Account, amount: u128, months: u32) -> anyhow::Result<()> {
        who.call(self.token.id(), "storage_deposit").args_json(json!({"account_id": self.ve.id()}))
            .deposit(NearToken::from_millinear(10)).transact().await?.into_result()?;
        who.call(self.token.id(), "ft_transfer_call")
            .args_json(json!({"receiver_id": self.ve.id(), "amount": U128(amount), "msg": format!("{{\"months\":{months}}}")}))
            .deposit(NearToken::from_yoctonear(1)).max_gas().transact().await?.into_result()?;
        Ok(())
    }

    async fn pass(&self, tier: u8, kind: Value) -> anyhow::Result<u64> {
        let id: u64 = self.root.call(self.gov.id(), "propose")
            .args_json(json!({"tier": tier, "kind": kind, "title": "test", "body_uri": "ipfs://test"}))
            .deposit(NearToken::from_yoctonear(1000)).max_gas().transact().await?.json()?;
        self.root.call(self.gov.id(), "vote").args_json(json!({"proposal_id": id, "choice": "For"}))
            .max_gas().transact().await?.into_result()?;
        self.worker.fast_forward(90).await?;
        let status: String = self.root.call(self.gov.id(), "finalize").args_json(json!({"proposal_id": id}))
            .max_gas().transact().await?.json()?;
        assert_eq!(status, "Queued");
        Ok(id)
    }

    async fn execute(&self, id: u64) -> anyhow::Result<String> {
        self.root.call(self.gov.id(), "execute").args_json(json!({"proposal_id": id}))
            .max_gas().transact().await?.into_result()?;
        let p: Value = self.gov.view("get_proposal").args_json(json!({"proposal_id": id})).await?.json()?;
        Ok(p["status"].as_str().unwrap().to_string())
    }

    async fn hash(&self) -> anyhow::Result<String> {
        let (_, h): (u64, String) = self.constitution.view("get_hash").await?.json()?;
        Ok(h)
    }
}

fn mission(budget: &str) -> Value {
    json!({"SetMission": {"mission_json": json!({
        "id": "bounties-q1", "title": "Bounties", "goal": "Pay for merged docs fixes", "asset": "near",
        "budget": budget, "max_per_action": budget, "deadline": "9000000000000000000",
        "metric": "merged PRs", "allow_chain_sig": false,
    }).to_string()}})
}

#[tokio::test]
#[ignore]
async fn mission_proposal_reaches_constitution_after_timelock() -> anyhow::Result<()> {
    let dao = deploy().await?;
    dao.lock(&dao.root, SUPPLY / 10, 24).await?;
    let before = dao.hash().await?;
    let id = dao.pass(3, mission("2000000000000000000000000")).await?;

    // Timelock not over yet.
    let early = dao.root.call(dao.gov.id(), "execute").args_json(json!({"proposal_id": id})).max_gas().transact().await?;
    assert!(early.is_failure(), "execute must fail before eta");

    dao.worker.fast_forward(90).await?;
    assert_eq!(dao.execute(id).await?, "Executed");
    let missions: Vec<Value> = dao.constitution.view("get_active_missions").await?.json()?;
    assert_eq!(missions.len(), 1);
    assert_ne!(dao.hash().await?, before, "hash must change when a mission is set");
    Ok(())
}

#[tokio::test]
#[ignore]
async fn function_call_rejected_outside_tier_one() -> anyhow::Result<()> {
    let dao = deploy().await?;
    let res = dao.root.call(dao.gov.id(), "propose")
        .args_json(json!({"tier": 3, "title": "x", "body_uri": "", "kind": {"FunctionCall": {
            "receiver_id": dao.agent.id(), "actions": [{"method_name": "unpause", "args": "", "deposit": "0", "gas": "10000000000000"}],
        }}}))
        .deposit(NearToken::from_yoctonear(1000)).transact().await?;
    assert!(res.is_failure());
    Ok(())
}

#[tokio::test]
#[ignore]
async fn agent_spend_respects_hash_and_budget() -> anyhow::Result<()> {
    let dao = deploy().await?;
    dao.lock(&dao.root, SUPPLY / 10, 24).await?;
    let worker_acc = sub(&dao.root, "worker", 10).await?;
    let payee = sub(&dao.root, "payee", 1).await?;

    // Tier 1: approve mock measurements and whitelist the local worker (7 day timelock).
    let b64 = |v: Value| near_sdk::base64::Engine::encode(&near_sdk::base64::engine::general_purpose::STANDARD, v.to_string());
    let mock: Value = serde_json::from_str(include_str!("mock_measurements.json"))?;
    let setup = dao.pass(1, json!({"FunctionCall": {"receiver_id": dao.agent.id(), "actions": [
        {"method_name": "approve_measurements", "args": b64(json!({"measurements": mock})), "deposit": "0", "gas": "20000000000000"},
        {"method_name": "approve_ppids", "args": b64(json!({"ppids": ["00000000000000000000000000000000"]})), "deposit": "0", "gas": "20000000000000"},
        {"method_name": "whitelist_agent_for_local", "args": b64(json!({"account_id": worker_acc.id()})), "deposit": "0", "gas": "20000000000000"},
    ]}})).await?;
    let m = dao.pass(3, mission("1000000000000000000000000")).await?;
    dao.worker.fast_forward(7 * 86_400 + 100).await?;
    assert_eq!(dao.execute(setup).await?, "Executed");
    assert_eq!(dao.execute(m).await?, "Executed");

    // The mock attestation shape comes from shade-attestation; local mode ignores its contents.
    let attestation: Value = serde_json::from_str(include_str!("mock_attestation.json"))?;
    worker_acc.call(dao.agent.id(), "register_agent").args_json(json!({"attestation": attestation}))
        .deposit(NearToken::from_near(1)).max_gas().transact().await?.into_result()?;

    let hash = dao.hash().await?;
    let action = |amount: &str| json!({"NativeTransfer": {"receiver_id": payee.id(), "amount": amount}});
    let propose = |task: &str, h: &str, amount: &str| {
        worker_acc.call(dao.agent.id(), "propose_action")
            .args_json(json!({"task_id": task, "mission_id": "bounties-q1", "constitution_hash": h, "action": action(amount), "reasoning_uri": "ipfs://log"}))
            .max_gas()
    };

    let before = payee.view_account().await?.balance;
    propose("t1", &hash, "400000000000000000000000").transact().await?.into_result()?;
    assert!(payee.view_account().await?.balance > before, "payment should land");

    // Same task_id twice: rejected.
    assert!(propose("t1", &hash, "1").transact().await?.is_failure());
    // Stale hash: no transfer.
    let bal = payee.view_account().await?.balance;
    let _ = propose("t2", "deadbeef", "1").transact().await?;
    assert_eq!(payee.view_account().await?.balance, bal);
    // Over budget: no transfer.
    let _ = propose("t3", &hash, "700000000000000000000000").transact().await?;
    assert_eq!(payee.view_account().await?.balance, bal);

    let receipts: Vec<Value> = dao.constitution.view("get_receipts").args_json(json!({})).await?.json()?;
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0]["constitution_hash"], hash);
    Ok(())
}
