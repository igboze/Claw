-- clawdao app database. A cache of chain state: everything here can be rebuilt by
-- replaying the indexer from the contracts' deploy blocks.
-- Shared by the Rust indexer (sqlx) and the TypeScript services (Drizzle).

CREATE TABLE IF NOT EXISTS blocks_cursor (
  service      text PRIMARY KEY,
  last_height  bigint NOT NULL,
  updated_at   timestamptz NOT NULL DEFAULT now()
);

-- Raw decoded NEP-297 events, append-only.
CREATE TABLE IF NOT EXISTS events (
  id          bigserial PRIMARY KEY,
  height      bigint NOT NULL,
  tx_hash     text,
  receipt_id  text NOT NULL,
  log_index   int NOT NULL,
  contract    text NOT NULL,
  kind        text NOT NULL,
  payload     jsonb NOT NULL,
  block_ts    timestamptz NOT NULL,
  UNIQUE (receipt_id, log_index)
);
CREATE INDEX IF NOT EXISTS events_kind_height ON events (kind, height DESC);
CREATE INDEX IF NOT EXISTS events_contract_height ON events (contract, height DESC);

CREATE TABLE IF NOT EXISTS proposals (
  id             bigint PRIMARY KEY,
  proposer       text NOT NULL,
  tier           smallint NOT NULL,
  kind           text NOT NULL,
  kind_payload   jsonb NOT NULL,
  title          text NOT NULL,
  body_uri       text NOT NULL,
  status         text NOT NULL,
  snapshot_ts    timestamptz NOT NULL,
  ends_at        timestamptz NOT NULL,
  eta            timestamptz,               -- when the timelock ends
  votes_for      numeric(40,0) NOT NULL DEFAULT 0,
  votes_against  numeric(40,0) NOT NULL DEFAULT 0,
  votes_abstain  numeric(40,0) NOT NULL DEFAULT 0,
  ai_summary     text,                      -- off-chain, clearly labelled in the UI
  height         bigint NOT NULL,
  updated_height bigint NOT NULL
);
CREATE INDEX IF NOT EXISTS proposals_status ON proposals (status);

CREATE TABLE IF NOT EXISTS votes (
  proposal_id  bigint NOT NULL REFERENCES proposals(id),
  voter        text NOT NULL,
  choice       text NOT NULL,
  power        numeric(40,0) NOT NULL,      -- from the ve snapshot
  height       bigint NOT NULL,
  PRIMARY KEY (proposal_id, voter)
);

CREATE TABLE IF NOT EXISTS ve_locks (
  account    text PRIMARY KEY,
  amount     numeric(40,0) NOT NULL,
  unlock_at  timestamptz NOT NULL,
  power      numeric(40,0) NOT NULL,        -- power at `height`; decays, recompute for display
  height     bigint NOT NULL
);

CREATE TABLE IF NOT EXISTS constitution_versions (
  version   int PRIMARY KEY,
  hash      text NOT NULL UNIQUE,
  reason    text NOT NULL,
  articles  jsonb,                          -- full snapshot, filled by jobs from get_constitution at height
  params    jsonb,
  height    bigint NOT NULL
);

CREATE TABLE IF NOT EXISTS missions (
  id               text PRIMARY KEY,
  title            text NOT NULL,
  goal             text NOT NULL,
  asset            text NOT NULL,
  budget           numeric(40,0) NOT NULL,
  max_per_action   numeric(40,0) NOT NULL,
  spent            numeric(40,0) NOT NULL DEFAULT 0,
  deadline         timestamptz NOT NULL,
  status           text NOT NULL,
  metric           text NOT NULL,
  allow_chain_sig  boolean NOT NULL DEFAULT false,
  height           bigint NOT NULL
);

-- The public audit trail (Tier 4).
CREATE TABLE IF NOT EXISTS receipts (
  id                 bigint PRIMARY KEY,
  task_id            text NOT NULL UNIQUE,
  mission_id         text NOT NULL,
  constitution_hash  text NOT NULL,
  action_hash        text NOT NULL,
  amount             numeric(40,0) NOT NULL,
  tx_hash            text,                  -- NEAR tx that carried the receipt
  chain              text NOT NULL DEFAULT 'near',
  foreign_tx_hash    text,                  -- set from foreign_tx event for Chain Signature actions
  reasoning_uri      text NOT NULL,
  height             bigint NOT NULL,
  block_ts           timestamptz NOT NULL
);
CREATE INDEX IF NOT EXISTS receipts_mission ON receipts (mission_id, height DESC);

CREATE TABLE IF NOT EXISTS treasury_snapshots (
  id         bigserial PRIMARY KEY,
  chain      text NOT NULL,
  asset      text NOT NULL,
  amount     numeric(40,0) NOT NULL,
  usd_value  numeric(20,2),
  taken_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS treasury_taken ON treasury_snapshots (taken_at DESC);

CREATE TABLE IF NOT EXISTS agent_workers (
  account_id      text PRIMARY KEY,
  measurements    jsonb NOT NULL,
  valid_until     timestamptz NOT NULL,
  last_heartbeat  timestamptz,
  last_epoch      bigint,
  height          bigint NOT NULL
);

CREATE TABLE IF NOT EXISTS bounty_submissions (
  id           bigserial PRIMARY KEY,
  mission_id   text NOT NULL,
  account      text NOT NULL,
  content_uri  text NOT NULL,
  note         text,
  verdict      text,                        -- written from the receipt, never by the API
  paid_tx      text,
  created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS bounty_mission ON bounty_submissions (mission_id, created_at DESC);
