# Architecture

IronClaw Coin (CLAW) is a NEP-141 token whose holders govern a fully autonomous IronClaw
agent through an on-chain, tiered constitution. This document maps the design to the code.

## Layers

```
 Holders ──wallet──▶ Governor ──timelock──▶ Constitution ◀──read every loop── IronClaw (TEE)
    │                   ▲                        ▲                               │
    └──lock CLAW──▶ Vote escrow (snapshots)      │ spend_check / record_receipt  ▼ tools
                                                 └──────── Agent contract ◀── claw-bridge
                                                           │  (attestation, caps,
                                                           │   task_id once, MPC sign)
                                                           ▼
                                               NEAR transfers / Chain Signatures

 Indexer ──NEP-297 events──▶ Postgres ──▶ API ──▶ Web app, Telegram, X     (rebuildable)
```

Only the NEAR contracts can move value. The off-chain app reads indexed events and can be
rebuilt by anyone from the chain.

## Contracts (`contracts/`)

| Crate | Account | Job | Changed by |
| --- | --- | --- | --- |
| `token` | `claw.near` | NEP-141 + NEP-145, fixed supply, no mint; 2% fee on DEX buys/sells to the fee router; holder burn | DEX list: Governor (Tier 1); everything else nobody |
| `ve` | `ve.claw.near` | Lock 1–24 months via `ft_transfer_call`; power = amount × remaining / 24 mo; checkpoints for `power_at(ts)`; per-account power cap | Governor (Tier 1) |
| `governor` | `gov.claw.near` | Proposals per tier, ve-weighted votes at snapshot, quorum, threshold, timelock, execution | Itself via Tier 1 `FunctionCall` |
| `constitution` | `constitution.claw.near` | Tier 0–2 articles, Tier 2 params, Tier 3 missions, Tier 4 receipts, current hash, `spend_check` | Tier 0 nobody; 1–3 Governor; 4 agent contract |
| `agent` | `agent.claw.near` | Shade agent: Dstack attestation, approved measurements (code hashes), `propose_action`, MPC signing, guardian pause, treasury, House of Stake lockup | Governor (Tier 1) |
| `fees` | `fees.claw.near` | Fee router: sells the 2% CLAW trading fee for NEAR, 20% dev / 80% by mode (HoS stake, holder rewards, buyback), $100k market-cap gate | Governor (mode: Tier 2 after gate; dev wallet, limits, DEX: Tier 1) |

`common` holds the NEP-297 event helper and tier constants. Every event is
`EVENT_JSON:{"standard":"clawdao","version":"1.0.0","event":…,"data":[…]}`.

### Tiers and what each may change

| Tier | Proposal kinds | Genesis settings | Enforced where |
| --- | --- | --- | --- |
| 0 | none | — | `constitution.amend` rejects tier 0 |
| 1 | `Amend` (tier-1 article), `FunctionCall` (upgrades, code hashes, guardrails, governor config) | 66.67%, 14-day lock | `governor::validate_kind`; Tier 1 config cannot drop below 66.67% / 7 days |
| 2 | `Amend` (tier-2 article), `RemoveArticle`, `SetParams` | 60%, 7-day lock | `validate_kind` |
| 3 | `SetMission`, `CancelMission` | majority, 2-day lock | `validate_kind` |
| 4 | receipts, written by the agent contract only | — | `assert_caller(agent)` |

### The hash

`constitution.bump()` hashes `(sorted articles, params, sorted mission definitions)` with
sha256 on every change. Mission `spent` is excluded, so spending does not change the hash.
`spend_check` rejects any action carrying an older hash, so the agent cannot act on rules
that have been replaced.

### `propose_action` flow

1. Worker (valid attestation, approved measurements and PPID, not expired) calls
   `propose_action(task_id, mission_id, constitution_hash, action, reasoning_uri)`.
2. Agent contract: not paused; `task_id` unused (marked used immediately).
3. `constitution.spend_check(mission, asset, amount, hash, chain_sig)`: hash current,
   mission active, asset matches, ≤ `max_per_action`, ≤ remaining budget, ≤ daily cap for
   the asset (no cap entry = zero), chain-sig allowed. Counts the spend.
4. On approval: NEAR transfer, NEP-141 transfer, or MPC `sign` on `v1.signer`.
5. On success: `record_receipt` (Tier 4) and return the signature. On failure:
   `refund_spend`.
6. For Chain Signatures the bridge broadcasts and calls `post_receipt` to link the
   foreign tx hash.

## Agent runtime (`agent/`)

One Docker image (`agent/Dockerfile`) = one code hash, run on Phala Cloud (Dstack):

- **IronClaw** (upstream image pinned by digest) is the brain. Routines run the mission
  loop (`agent/ironclaw/routines`, `skills/clawdao-mission/SKILL.md`). LLM calls go to NEAR
  AI private inference. IronClaw's own Postgres runs in the same CVM
  (`infra/phala/docker-compose.yml`).
- **claw-bridge** (`agent/bridge`, TypeScript, `@neardefi/shade-agent-js`) is the signer.
  It registers the worker, re-registers every 6 days, sends heartbeats, and serves a
  localhost-only API: `/constitution`, `/missions`, `/view`, `/prepare-evm`,
  `/propose-action`, `/heartbeat`, `/publish-log`.
- **Tools** (`agent/tools`, Rust → `wasm32-wasip2` components on IronClaw's
  `near:agent@0.4.1` `sandboxed-tool` WIT world): `constitution_read`, `mission_status`,
  `near_view`, `swap_quote`, `pay_bounty`, `post_social`, `publish_log`. Logic lives in
  `claw-tool-core` and is unit-tested on the host. Value tools only call
  `/propose-action`.

`task_id = sha256(mission_id ":" loop_epoch)`, so 2–3 workers on different hosts compute
the same id for the same loop and the contract accepts it once.

## Backend (`services/`)

| Service | Stack | Notes |
| --- | --- | --- |
| `indexer` | Rust, near-lake-framework 0.7, sqlx | Decodes NEP-297 events from the five contracts; writes `events` and projections in one transaction per block; resumes from `blocks_cursor` |
| `api` | Fastify + tRPC | Read-only, rate-limited; `/status` for the public status page and alert conditions |
| `jobs` | BullMQ | Treasury snapshots, constitution text per version, AI proposal summaries (labelled), IPFS pin checks, votes-closing alerts, social metrics |
| `notifier` | grammY + X API | Posts new proposals, results, receipts; bot commands mirror read flows |
| `bounty-intake` | Fastify | Accepts submissions to IPFS; exposes them to the agent as untrusted data; never writes verdicts |

## Database (`packages/db`)

PostgreSQL 16. Plain SQL migrations in `packages/db/migrations` are the source of truth,
applied by `pnpm db:migrate`; Drizzle mirrors them for TypeScript and the indexer uses
sqlx. Tables: `blocks_cursor`, `events`, `proposals`, `votes`, `ve_locks`,
`constitution_versions`, `missions`, `receipts`, `treasury_snapshots`, `agent_workers`,
`bounty_submissions`.

## Web (`apps/web`)

Next.js App Router, Tailwind, shadcn-style components, NEAR Wallet Selector. Pages: Home,
Constitution (tiers, params, version diff), Missions, Proposals (create, vote, finalize,
execute, timelock countdown, labelled AI summary), Lock, Agent, Bounties. Detail views use
query parameters so `pnpm export:ipfs` produces a static mirror for IPFS.

## Fees and tokenomics

See [tokenomics.md](tokenomics.md): the 2% trading fee, the fee router, what House of Stake
staking earns, holder rewards, and `Allocate` votes.

## House of Stake

The agent contract owns a veNEAR lockup on `venear.dao`; CLAW holders mandate how it votes on
`vote.dao` through `HosVote` proposals, and a relayer casts the mandate with a snapshot proof.
See [house-of-stake.md](house-of-stake.md).

## Open question

Confirm with NEAR AI whether IronClaw on NEAR AI Cloud can produce an attestation the
Shade agent contract accepts. If yes, the brain can move there and stake-to-compute can pay
for it; the agent contract would need a second accepted measurement set.
