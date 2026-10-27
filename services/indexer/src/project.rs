//! Projections: turn each event into rows. U128 values arrive as strings and are cast to
//! numeric in SQL; nanosecond timestamps are converted with to_timestamp(ns / 1e9).
//! Queries use runtime-checked `sqlx::query`; switch to `query!` with `cargo sqlx prepare`
//! in CI once a database is available there.

use crate::events::Event;
use serde_json::Value;
use sqlx::{Postgres, Transaction};

pub struct Ctx<'a> {
    pub height: i64,
    pub block_ts_ns: u64,
    pub contract: &'a str,
    pub receipt_id: &'a str,
    pub log_index: i32,
}

fn s(v: &Value, k: &str) -> String {
    match &v[k] {
        Value::String(x) => x.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn opt(v: &Value, k: &str) -> Option<String> {
    match &v[k] {
        Value::Null => None,
        _ => Some(s(v, k)),
    }
}

fn i(v: &Value, k: &str) -> i64 {
    v[k].as_i64().or_else(|| v[k].as_str().and_then(|x| x.parse().ok())).unwrap_or_default()
}

const TS: &str = "to_timestamp(($X)::numeric / 1e9)";
fn ts(n: usize) -> String {
    TS.replace("$X", &format!("${n}"))
}

/// Returns true if the event was new (not a replay).
pub async fn apply(tx: &mut Transaction<'_, Postgres>, c: &Ctx<'_>, ev: &Event) -> anyhow::Result<bool> {
    let payload = serde_json::to_value(ev.data.first().cloned().unwrap_or(Value::Null))?;
    let inserted = sqlx::query(&format!(
        "INSERT INTO events (height, receipt_id, log_index, contract, kind, payload, block_ts)
         VALUES ($1, $2, $3, $4, $5, $6, {}) ON CONFLICT (receipt_id, log_index) DO NOTHING",
        ts(7)
    ))
    .bind(c.height)
    .bind(c.receipt_id)
    .bind(c.log_index)
    .bind(c.contract)
    .bind(&ev.event)
    .bind(&payload)
    .bind(c.block_ts_ns.to_string())
    .execute(&mut **tx)
    .await?
    .rows_affected()
        == 1;
    if !inserted {
        return Ok(false);
    }

    let d = &payload;
    let h = c.height;
    match ev.event.as_str() {
        "proposal_created" => {
            let kind_name = d["kind"].as_object().and_then(|o| o.keys().next().cloned()).unwrap_or_default();
            sqlx::query(&format!(
                "INSERT INTO proposals (id, proposer, tier, kind, kind_payload, title, body_uri, status, snapshot_ts, ends_at, height, updated_height)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, {}, {}, $11, $11) ON CONFLICT (id) DO NOTHING",
                ts(9),
                ts(10)
            ))
            .bind(i(d, "id"))
            .bind(s(d, "proposer"))
            .bind(i(d, "tier") as i16)
            .bind(kind_name)
            .bind(&d["kind"])
            .bind(s(d, "title"))
            .bind(s(d, "body_uri"))
            .bind(s(d, "status"))
            .bind(s(d, "snapshot_ts"))
            .bind(s(d, "ends_at"))
            .bind(h)
            .execute(&mut **tx)
            .await?;
        }
        "vote_cast" => {
            let col = match s(d, "choice").as_str() {
                "For" => "votes_for",
                "Against" => "votes_against",
                _ => "votes_abstain",
            };
            sqlx::query("INSERT INTO votes (proposal_id, voter, choice, power, height) VALUES ($1, $2, $3, $4::numeric, $5) ON CONFLICT DO NOTHING")
                .bind(i(d, "proposal_id"))
                .bind(s(d, "voter"))
                .bind(s(d, "choice"))
                .bind(s(d, "power"))
                .bind(h)
                .execute(&mut **tx)
                .await?;
            sqlx::query(&format!("UPDATE proposals SET {col} = {col} + $1::numeric, updated_height = $2 WHERE id = $3"))
                .bind(s(d, "power"))
                .bind(h)
                .bind(i(d, "proposal_id"))
                .execute(&mut **tx)
                .await?;
        }
        "proposal_finalized" => {
            sqlx::query(&format!(
                "UPDATE proposals SET status = $1, eta = CASE WHEN $2::text IS NULL THEN NULL ELSE {} END,
                 votes_for = $3::numeric, votes_against = $4::numeric, votes_abstain = $5::numeric, updated_height = $6,
                 outcome = $8 WHERE id = $7",
                ts(2)
            ))
            .bind(s(d, "status"))
            .bind(opt(d, "eta"))
            .bind(s(d, "votes_for"))
            .bind(s(d, "votes_against"))
            .bind(s(d, "votes_abstain"))
            .bind(h)
            .bind(i(d, "proposal_id"))
            .bind(opt(d, "outcome"))
            .execute(&mut **tx)
            .await?;
        }
        "proposal_executed" => {
            let status = if d["success"].as_bool() == Some(true) { "Executed" } else { "Failed" };
            sqlx::query("UPDATE proposals SET status = $1, updated_height = $2 WHERE id = $3")
                .bind(status)
                .bind(h)
                .bind(i(d, "proposal_id"))
                .execute(&mut **tx)
                .await?;
        }
        "ve_lock" | "ve_extend" => {
            // Power shown here assumes the genesis 24-month max; the API recomputes live.
            let amount_sql = if ev.event == "ve_lock" { "$2::numeric" } else { "COALESCE((SELECT amount FROM ve_locks WHERE account = $1), 0)" };
            sqlx::query(&format!(
                "INSERT INTO ve_locks (account, amount, unlock_at, power, height)
                 VALUES ($1, {amount_sql}, {unlock}, 0, $4)
                 ON CONFLICT (account) DO UPDATE SET amount = EXCLUDED.amount, unlock_at = EXCLUDED.unlock_at, height = EXCLUDED.height",
                unlock = ts(3)
            ))
            .bind(s(d, "account_id"))
            .bind(s(d, "amount"))
            .bind(s(d, "unlock_at"))
            .bind(h)
            .execute(&mut **tx)
            .await?;
            sqlx::query(&format!(
                "UPDATE ve_locks SET power = floor(amount * LEAST(GREATEST(extract(epoch FROM unlock_at - {now}), 0), 62208000) / 62208000) WHERE account = $1",
                now = ts(2)
            ))
            .bind(s(d, "account_id"))
            .bind(c.block_ts_ns.to_string())
            .execute(&mut **tx)
            .await?;
        }
        "ve_withdraw" => {
            sqlx::query("DELETE FROM ve_locks WHERE account = $1").bind(s(d, "account_id")).execute(&mut **tx).await?;
        }
        "constitution_updated" => {
            sqlx::query("INSERT INTO constitution_versions (version, hash, reason, height) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING")
                .bind(i(d, "version") as i32)
                .bind(s(d, "hash"))
                .bind(s(d, "reason"))
                .bind(h)
                .execute(&mut **tx)
                .await?;
        }
        "mission_set" => {
            sqlx::query(&format!(
                "INSERT INTO missions (id, title, goal, asset, budget, max_per_action, deadline, status, metric, allow_chain_sig, height)
                 VALUES ($1, $2, $3, $4, $5::numeric, $6::numeric, {}, 'Active', $8, $9, $10)
                 ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title, goal = EXCLUDED.goal, asset = EXCLUDED.asset,
                   budget = EXCLUDED.budget, max_per_action = EXCLUDED.max_per_action, deadline = EXCLUDED.deadline,
                   status = 'Active', metric = EXCLUDED.metric, allow_chain_sig = EXCLUDED.allow_chain_sig, height = EXCLUDED.height",
                ts(7)
            ))
            .bind(s(d, "id"))
            .bind(s(d, "title"))
            .bind(s(d, "goal"))
            .bind(s(d, "asset"))
            .bind(s(d, "budget"))
            .bind(s(d, "max_per_action"))
            .bind(s(d, "deadline"))
            .bind(s(d, "metric"))
            .bind(d["allow_chain_sig"].as_bool().unwrap_or(false))
            .bind(h)
            .execute(&mut **tx)
            .await?;
        }
        "mission_cancelled" => {
            sqlx::query("UPDATE missions SET status = 'Cancelled', height = $1 WHERE id = $2")
                .bind(h)
                .bind(s(d, "mission_id"))
                .execute(&mut **tx)
                .await?;
        }
        "spend_checked" => {
            sqlx::query("UPDATE missions SET spent = $1::numeric, status = CASE WHEN $1::numeric >= budget THEN 'Exhausted' ELSE status END, height = $2 WHERE id = $3")
                .bind(s(d, "mission_spent"))
                .bind(h)
                .bind(s(d, "mission_id"))
                .execute(&mut **tx)
                .await?;
        }
        "spend_refunded" => {
            sqlx::query("UPDATE missions SET spent = GREATEST(spent - $1::numeric, 0), status = CASE WHEN status = 'Exhausted' THEN 'Active' ELSE status END, height = $2 WHERE id = $3")
                .bind(s(d, "amount"))
                .bind(h)
                .bind(s(d, "mission_id"))
                .execute(&mut **tx)
                .await?;
        }
        "receipt" => {
            sqlx::query(&format!(
                "INSERT INTO receipts (id, task_id, mission_id, constitution_hash, action_hash, amount, reasoning_uri, height, block_ts)
                 VALUES ($1, $2, $3, $4, $5, $6::numeric, $7, $8, {}) ON CONFLICT (id) DO NOTHING",
                ts(9)
            ))
            .bind(i(d, "id"))
            .bind(s(d, "task_id"))
            .bind(s(d, "mission_id"))
            .bind(s(d, "constitution_hash"))
            .bind(s(d, "action_hash"))
            .bind(s(d, "amount"))
            .bind(s(d, "reasoning_uri"))
            .bind(h)
            .bind(s(d, "timestamp"))
            .execute(&mut **tx)
            .await?;
        }
        "foreign_tx" => {
            sqlx::query("UPDATE receipts SET chain = $1, foreign_tx_hash = $2 WHERE task_id = $3")
                .bind(s(d, "chain"))
                .bind(s(d, "tx_hash"))
                .bind(s(d, "task_id"))
                .execute(&mut **tx)
                .await?;
        }
        "agent_registered" => {
            sqlx::query(
                "INSERT INTO agent_workers (account_id, measurements, valid_until, height)
                 VALUES ($1, $2, to_timestamp(($3)::numeric / 1e3), $4)
                 ON CONFLICT (account_id) DO UPDATE SET measurements = EXCLUDED.measurements, valid_until = EXCLUDED.valid_until, height = EXCLUDED.height",
            )
            .bind(s(d, "account_id"))
            .bind(&d["measurements"])
            .bind(s(d, "valid_until_ms"))
            .bind(h)
            .execute(&mut **tx)
            .await?;
        }
        "heartbeat" => {
            sqlx::query(&format!("UPDATE agent_workers SET last_heartbeat = {}, last_epoch = $2 WHERE account_id = $3", ts(1)))
                .bind(c.block_ts_ns.to_string())
                .bind(i(d, "loop_epoch"))
                .bind(s(d, "agent"))
                .execute(&mut **tx)
                .await?;
        }
        "agent_removed" => {
            sqlx::query("DELETE FROM agent_workers WHERE account_id = $1").bind(s(d, "account_id")).execute(&mut **tx).await?;
        }
        "hos_mandate" => {
            sqlx::query(
                "INSERT INTO hos_mandates (hos_proposal_id, vote, clawdao_proposal_id, height) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (hos_proposal_id) DO UPDATE SET vote = EXCLUDED.vote, clawdao_proposal_id = EXCLUDED.clawdao_proposal_id, height = EXCLUDED.height",
            )
            .bind(i(d, "hos_proposal_id") as i32)
            .bind(s(d, "vote"))
            .bind(i(d, "clawdao_proposal_id"))
            .bind(h)
            .execute(&mut **tx)
            .await?;
        }
        "hos_vote" if d["success"].as_bool() == Some(true) => {
            sqlx::query("UPDATE hos_mandates SET cast_vote = $1, height = $2 WHERE hos_proposal_id = $3")
                .bind(s(d, "vote"))
                .bind(h)
                .bind(i(d, "hos_proposal_id") as i32)
                .execute(&mut **tx)
                .await?;
        }
        _ => {} // Stored in `events` only.
    }
    Ok(true)
}
