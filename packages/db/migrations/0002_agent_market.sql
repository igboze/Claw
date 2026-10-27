-- Agent Market earnings (Tier 4), mirrored from `earning` events.
CREATE TABLE IF NOT EXISTS market_earnings (
  id                bigint PRIMARY KEY,
  mission_id        text NOT NULL,
  assignment_id     text NOT NULL UNIQUE,
  job_id            text NOT NULL,
  amount            text NOT NULL,         -- escrow as the marketplace states it, before its 5% fee
  amount_cents      bigint NOT NULL,
  currency          text NOT NULL,
  deliverable_hash  text,
  height            bigint NOT NULL,
  block_ts          timestamptz NOT NULL
);
CREATE INDEX IF NOT EXISTS market_earnings_mission ON market_earnings (mission_id, height DESC);

-- Work terms for missions that earn on Agent Market.
ALTER TABLE missions ADD COLUMN IF NOT EXISTS work jsonb;
