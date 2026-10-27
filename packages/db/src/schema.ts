// Drizzle mirror of migrations/*.sql. The SQL files are the source of truth (the
// Rust indexer applies the same schema); keep this file in step with them.

import {
  bigint, bigserial, boolean, index, integer, jsonb, numeric, pgTable, primaryKey, smallint,
  text, timestamp, unique,
} from "drizzle-orm/pg-core";

const big = (name: string) => bigint(name, { mode: "number" });
const amount = (name: string) => numeric(name, { precision: 40, scale: 0 });
const ts = (name: string) => timestamp(name, { withTimezone: true });

export const blocksCursor = pgTable("blocks_cursor", {
  service: text("service").primaryKey(),
  lastHeight: big("last_height").notNull(),
  updatedAt: ts("updated_at").notNull().defaultNow(),
});

export const events = pgTable(
  "events",
  {
    id: bigserial("id", { mode: "number" }).primaryKey(),
    height: big("height").notNull(),
    txHash: text("tx_hash"),
    receiptId: text("receipt_id").notNull(),
    logIndex: integer("log_index").notNull(),
    contract: text("contract").notNull(),
    kind: text("kind").notNull(),
    payload: jsonb("payload").notNull(),
    blockTs: ts("block_ts").notNull(),
  },
  (t) => [unique().on(t.receiptId, t.logIndex), index("events_kind_height").on(t.kind, t.height)],
);

export const proposals = pgTable("proposals", {
  id: big("id").primaryKey(),
  proposer: text("proposer").notNull(),
  tier: smallint("tier").notNull(),
  kind: text("kind").notNull(),
  kindPayload: jsonb("kind_payload").notNull(),
  title: text("title").notNull(),
  bodyUri: text("body_uri").notNull(),
  status: text("status").notNull(),
  snapshotTs: ts("snapshot_ts").notNull(),
  endsAt: ts("ends_at").notNull(),
  eta: ts("eta"),
  votesFor: amount("votes_for").notNull().default("0"),
  votesAgainst: amount("votes_against").notNull().default("0"),
  votesAbstain: amount("votes_abstain").notNull().default("0"),
  aiSummary: text("ai_summary"),
  outcome: text("outcome"),
  height: big("height").notNull(),
  updatedHeight: big("updated_height").notNull(),
});

export const votes = pgTable(
  "votes",
  {
    proposalId: big("proposal_id").notNull().references(() => proposals.id),
    voter: text("voter").notNull(),
    choice: text("choice").notNull(),
    power: amount("power").notNull(),
    height: big("height").notNull(),
  },
  (t) => [primaryKey({ columns: [t.proposalId, t.voter] })],
);

export const veLocks = pgTable("ve_locks", {
  account: text("account").primaryKey(),
  amount: amount("amount").notNull(),
  unlockAt: ts("unlock_at").notNull(),
  power: amount("power").notNull(),
  height: big("height").notNull(),
});

export const constitutionVersions = pgTable("constitution_versions", {
  version: integer("version").primaryKey(),
  hash: text("hash").notNull().unique(),
  reason: text("reason").notNull(),
  articles: jsonb("articles"),
  params: jsonb("params"),
  height: big("height").notNull(),
});

export const missions = pgTable("missions", {
  id: text("id").primaryKey(),
  title: text("title").notNull(),
  goal: text("goal").notNull(),
  asset: text("asset").notNull(),
  budget: amount("budget").notNull(),
  maxPerAction: amount("max_per_action").notNull(),
  spent: amount("spent").notNull().default("0"),
  deadline: ts("deadline").notNull(),
  status: text("status").notNull(),
  metric: text("metric").notNull(),
  allowChainSig: boolean("allow_chain_sig").notNull().default(false),
  height: big("height").notNull(),
});

export const receipts = pgTable("receipts", {
  id: big("id").primaryKey(),
  taskId: text("task_id").notNull().unique(),
  missionId: text("mission_id").notNull(),
  constitutionHash: text("constitution_hash").notNull(),
  actionHash: text("action_hash").notNull(),
  amount: amount("amount").notNull(),
  txHash: text("tx_hash"),
  chain: text("chain").notNull().default("near"),
  foreignTxHash: text("foreign_tx_hash"),
  reasoningUri: text("reasoning_uri").notNull(),
  height: big("height").notNull(),
  blockTs: ts("block_ts").notNull(),
});

export const treasurySnapshots = pgTable("treasury_snapshots", {
  id: bigserial("id", { mode: "number" }).primaryKey(),
  chain: text("chain").notNull(),
  asset: text("asset").notNull(),
  amount: amount("amount").notNull(),
  usdValue: numeric("usd_value", { precision: 20, scale: 2 }),
  takenAt: ts("taken_at").notNull().defaultNow(),
});

export const agentWorkers = pgTable("agent_workers", {
  accountId: text("account_id").primaryKey(),
  measurements: jsonb("measurements").notNull(),
  validUntil: ts("valid_until").notNull(),
  lastHeartbeat: ts("last_heartbeat"),
  lastEpoch: big("last_epoch"),
  height: big("height").notNull(),
});

export const bountySubmissions = pgTable("bounty_submissions", {
  id: bigserial("id", { mode: "number" }).primaryKey(),
  missionId: text("mission_id").notNull(),
  account: text("account").notNull(),
  contentUri: text("content_uri").notNull(),
  note: text("note"),
  verdict: text("verdict"),
  paidTx: text("paid_tx"),
  createdAt: ts("created_at").notNull().defaultNow(),
});

export const hosProposals = pgTable("hos_proposals", {
  id: integer("id").primaryKey(),
  title: text("title"),
  description: text("description"),
  link: text("link"),
  proposer: text("proposer").notNull(),
  status: text("status").notNull(),
  snapshotBlock: big("snapshot_block"),
  votingStart: ts("voting_start"),
  votingEnds: ts("voting_ends"),
  votes: jsonb("votes"),
  updatedAt: ts("updated_at").notNull().defaultNow(),
});

export const hosMandates = pgTable("hos_mandates", {
  hosProposalId: integer("hos_proposal_id").primaryKey(),
  vote: text("vote").notNull(),
  clawdaoProposalId: big("clawdao_proposal_id").notNull(),
  castVote: text("cast_vote"),
  height: big("height").notNull(),
});
