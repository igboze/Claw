//! clawdao indexer: streams NEAR blocks from NEAR Lake, decodes NEP-297 events from the
//! five clawdao contracts and writes them to Postgres.
//!
//! - `events` gets every raw event (append-only, idempotent on receipt_id + log_index).
//! - Projection tables (proposals, votes, missions, receipts, ...) are updated in the
//!   same transaction, so a crash never leaves them out of step with `events`.
//! - `blocks_cursor` stores the last processed height; restarts resume from there.
//!   Wiping the database and starting from the deploy height rebuilds everything.

mod events;
mod project;

use anyhow::Context;
use near_lake_framework::near_indexer_primitives::views::ExecutionStatusView;
use near_lake_framework::near_indexer_primitives::StreamerMessage;
use near_lake_framework::LakeConfigBuilder;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashSet;

const SERVICE: &str = "indexer";

struct Config {
    database_url: String,
    network: String,
    start_height: u64,
    contracts: HashSet<String>,
}

fn config() -> anyhow::Result<Config> {
    let env = |k: &str| std::env::var(k).with_context(|| format!("missing env {k}"));
    let contracts = ["TOKEN_CONTRACT", "VE_CONTRACT", "GOVERNOR_CONTRACT", "CONSTITUTION_CONTRACT", "AGENT_CONTRACT"]
        .iter()
        .map(|k| env(k))
        .collect::<anyhow::Result<HashSet<_>>>()?;
    Ok(Config {
        database_url: env("DATABASE_URL")?,
        network: std::env::var("NEAR_NETWORK").unwrap_or_else(|_| "testnet".into()),
        start_height: env("INDEXER_START_HEIGHT")?.parse()?,
        contracts,
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let cfg = config()?;
    let pool = PgPoolOptions::new().max_connections(5).connect(&cfg.database_url).await?;

    let resume: Option<i64> = sqlx::query_scalar("SELECT last_height FROM blocks_cursor WHERE service = $1")
        .bind(SERVICE)
        .fetch_optional(&pool)
        .await?;
    let start = resume.map(|h| h as u64 + 1).unwrap_or(cfg.start_height);
    tracing::info!(start, network = %cfg.network, "starting indexer");

    let builder = LakeConfigBuilder::default().start_block_height(start);
    let lake = match cfg.network.as_str() {
        "mainnet" => builder.mainnet(),
        _ => builder.testnet(),
    }
    .build()?;
    let (handle, mut stream) = near_lake_framework::streamer(lake);

    while let Some(msg) = stream.recv().await {
        if let Err(e) = handle_block(&pool, &cfg.contracts, msg).await {
            // Stop rather than skip: a gap would make the cache wrong. Restart resumes here.
            tracing::error!(error = ?e, "failed to process block; exiting");
            drop(stream);
            handle.abort();
            return Err(e);
        }
    }
    handle.await??;
    Ok(())
}

async fn handle_block(pool: &PgPool, contracts: &HashSet<String>, msg: StreamerMessage) -> anyhow::Result<()> {
    let height = msg.block.header.height as i64;
    let block_ts_ns = msg.block.header.timestamp_nanosec;
    let mut tx = pool.begin().await?;
    let mut n = 0usize;

    for shard in &msg.shards {
        for o in &shard.receipt_execution_outcomes {
            let executor = o.execution_outcome.outcome.executor_id.to_string();
            if !contracts.contains(&executor) {
                continue;
            }
            // Logs from failed receipts describe state changes that were rolled back.
            if matches!(o.execution_outcome.outcome.status, ExecutionStatusView::Failure(_) | ExecutionStatusView::Unknown) {
                continue;
            }
            let receipt_id = o.receipt.receipt_id.to_string();
            for (i, log) in o.execution_outcome.outcome.logs.iter().enumerate() {
                let Some(ev) = events::parse(log) else { continue };
                let ctx = project::Ctx { height, block_ts_ns, contract: &executor, receipt_id: &receipt_id, log_index: i as i32 };
                if project::apply(&mut tx, &ctx, &ev).await? {
                    n += 1;
                }
            }
        }
    }

    sqlx::query(
        "INSERT INTO blocks_cursor (service, last_height) VALUES ($1, $2)
         ON CONFLICT (service) DO UPDATE SET last_height = EXCLUDED.last_height, updated_at = now()",
    )
    .bind(SERVICE)
    .bind(height)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    if n > 0 {
        tracing::info!(height, events = n, "indexed");
    }
    Ok(())
}
