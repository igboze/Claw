# Proposal: NEAR AI Agent Market and more agent use cases

Status: **proposal, not built.** Researched 2026-10-27 from [docs.market.near.ai](https://docs.market.near.ai)
and the [launch post](https://near.ai/blog/introducing-near-ai-agent-market).

## What Agent Market is

NEAR AI's marketplace at `market.near.ai` where people and agents hire agents.

- **Job types:** Instant (hire a listed agent at its price), Standard (post a job, agents bid,
  you award), Competition (prize pool, a judge scores entries).
- **Money:** the marketplace holds payment from award until the buyer accepts. Auto-accept
  happens 3–7 days after delivery. Disputes need a 5% deposit and go to an automated resolver,
  then optional human review. Platform fee: 5% of the builder's price.
- **Balances are off-chain:** USD (card) and USDC (crypto deposit, withdrawable to a wallet).
- **x402 on NEAR:** every listed agent is an A2A server at `https://<handle>.market.near.ai`. A
  call without payment returns `402` with an `accepts` row (`network: near:mainnet`, USDC
  asset, amount, `payTo`). The caller pays by signing **one delegated `ft_transfer`** (a
  relayer pays gas) and gets the result in the same call. No account needed; settles on
  delivery, no rework or dispute.
- **Selling:** a self-hosted agent gets a per-agent token (`aat_…`), polls
  `GET /v1/agents/me/assignments`, bids via `POST /v1/jobs/{id}/bids`, starts with
  `POST /v1/assignments/{id}/start` and submits a deliverable. Payouts go to the owner's balance.

## How clawdao plugs in

### 1. Assign tasks (clawdao hires agents) — via x402, fully on-chain controlled

- The agent contract controls a **market wallet**: a NEAR implicit account derived through
  Chain Signatures (Eddsa domain) from `agent.claw.near`. No human or TEE holds its key; only
  the MPC network signs, and only after `spend_check`.
- Holders fund work with a **Tier 3 mission**, e.g. "Hire security reviewers for the fee router,
  budget 2,000 USDC, max 300 per hire, allowed handles: [...]". The mission's asset is USDC and
  `allow_chain_sig = true`.
- Per hire: a new tool `market_hire(handle, brief)` asks the agent for the 402 quote. claw-bridge
  builds the delegated `ft_transfer` matching the `accepts` row. The agent contract runs
  `spend_check(mission, USDC, amount)` and requests the MPC signature. The bridge sends
  `X-PAYMENT`, and the result plus `task.id` go into the reasoning log and receipt.
- The hard caps are the mission budget, the per-action cap, the daily cap and the market wallet's
  balance (top up only what a mission needs).
- **Contract work needed:** in `propose_action`, verify that a `ChainSignature` payload is a
  delegate `ft_transfer` of exactly the declared USDC amount to an allowlisted `payTo`. Today
  the value is declared, not parsed. This is the main audit item.

Standard jobs and competitions need a marketplace account with an off-chain balance. That is
custodial (the account credential would live in the TEE), so treat it as phase 2 and cap the
deposit as a mission spend.

### 2. Do tasks (clawdao earns) — list the agent as a self-hosted seller

- List "clawdao agent" on Agent Market. The `aat_` token lives only in the TEE env.
- A **Tier 3 "work" mission** sets what it may take on: tags (e.g. NEAR governance analysis,
  contract review summaries, data reports), minimum price, maximum open jobs, deadline.
- New tools: `market_board` (read jobs), `market_bid`, `market_start`, `market_submit`. Briefs
  are untrusted input, the same as bounty submissions.
- Earnings land in the marketplace USDC balance. A Tier 2 vote withdraws them to the treasury's
  NEAR account. A new governed `treasury_swap` (USDC → NEAR on Rhea) converts them, then a Tier 2
  `Allocate` sends them where holders choose: stake in House of Stake, holder rewards, or
  buyback and burn. Earnings are never spent without a vote.

### Build order

1. Earn side (low risk, no treasury exposure): listing, polling tools, work mission, receipts.
2. `treasury_swap` + withdrawal flow so earnings reach `Allocate`.
3. x402 hire with on-chain payload verification, behind a small first mission.
4. Optional: Standard jobs via a capped marketplace account.

## More use cases for the agent

Each one is a mission type holders can fund; most reuse existing tools.

| Use case | What the agent does | Funded / measured by | New pieces |
| --- | --- | --- | --- |
| Bounty manager | Judge submissions, pay accepted work | Mission budget; merged/accepted items | Exists |
| House of Stake analyst | Publish a neutral brief on every HoS proposal before the CLAW vote | Ops mission; briefs published | `hos_read` tool |
| Marketplace contractor | Hire specialist agents (audits, research, design, translation) | USDC mission; deliverables accepted | Section 1 |
| Marketplace worker | Sell analysis and reports, earn USDC for holders | Work mission; USDC earned | Section 2 |
| Pays its own bills | Pay TEE hosting, inference, indexer invoices | Standing ops mission, monthly cap | Exists (operations mission) |
| Builder grants | Small milestone grants to IronClaw/NEAR builders; checks repos before paying | Grants mission; milestones verified | `github_read` tool |
| Liquidity manager | Add or rebalance CLAW/NEAR liquidity within caps | Tier 2 mission; depth and volume | `dex_liquidity` tool, contract action type |
| Buyback executor | Spread buybacks over time (TWAP) to cut slippage | Buyback mode; average price vs spot | Keeper schedule |
| Treasury reporter | Weekly public report: fees, staking, rewards, spend, receipts | Ops mission | Exists (`publish_log`, `post_social`) |
| Security watcher | Watch contract events and RPC for anomalies, alert holders and the guardian | Ops mission | Event-watch tool |
| Community concierge | Answer holder questions on Telegram from docs and chain data | Ops mission; response quality | Telegram tool |
| Content and education | Commission tutorials, translations, videos via bounties | Bounty mission | Exists |
| Cross-chain payouts | Pay contributors on other chains via Chain Signatures | Mission with `allow_chain_sig` | Exists (EVM via bridge) |
| Ecosystem data | Publish NEAR and IronClaw metrics as public datasets | Ops mission | Data tools |
| Hackathon organiser | Post challenges, judge entries, pay prizes (or run Agent Market competitions) | Prize mission | Competition flow |
| Agent-to-agent partnerships | Buy or sell services with other DAO agents on Agent Market | Missions both ways | Sections 1–2 |

All of these keep the same rule: the agent proposes and the contracts cap. Value moves only
through `propose_action` → `spend_check`, with a receipt and a public reasoning log.
