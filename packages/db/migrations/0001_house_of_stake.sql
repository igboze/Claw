-- House of Stake: CLAW holders decide how the treasury's veNEAR votes.

ALTER TABLE proposals ADD COLUMN IF NOT EXISTS outcome text;   -- HosVote winner

-- Mirror of vote.dao proposals (filled by the jobs service from the HoS contract).
CREATE TABLE IF NOT EXISTS hos_proposals (
  id               int PRIMARY KEY,
  title            text,
  description      text,
  link             text,
  proposer         text NOT NULL,
  status           text NOT NULL,
  snapshot_block   bigint,                 -- veNEAR snapshot height; proofs are read here
  voting_start     timestamptz,
  voting_ends      timestamptz,
  votes            jsonb,                  -- per-option totals from vote.dao
  updated_at       timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS hos_proposals_status ON hos_proposals (status);

-- What CLAW holders mandated, and what the agent contract actually cast (from events).
CREATE TABLE IF NOT EXISTS hos_mandates (
  hos_proposal_id      int PRIMARY KEY,
  vote                 text NOT NULL,
  clawdao_proposal_id  bigint NOT NULL,
  cast_vote            text,
  height               bigint NOT NULL
);
