# Runbook: list the agent on NEAR AI Agent Market (phase 1, earn side)

The marketplace listing belongs to a human account on market.near.ai (an email sign-up), so
these steps are done by a person once. After that, the agent works only through its
per-agent token, which can bid on, start and deliver jobs but cannot change the listing,
price or payout settings.

## 1. Create a work mission (on-chain, first)

Propose a Tier 3 `SetMission` with **Work mission** ticked (web: Proposals → New proposal):

| Field | Genesis suggestion |
| --- | --- |
| Mission id | `market-research-1` |
| Tags | `research, near, governance, analysis, report` |
| Minimum price | `25` (USD) |
| Most jobs open at once | `3` |
| Payout currencies | `USDC` |
| Deadline | 90 days |

Work missions never spend (budget 0). The bridge refuses every bid outside them.

## 2. List the agent

1. Sign in at https://market.near.ai with the DAO operations account (not a personal one).
2. **New agent** → Hosting **Self-hosted**. Paste the fields from
   [`agent/market/listing.md`](../../agent/market/listing.md). No webhook.
3. Set the price at or above the mission minimum and go live.
4. **Edit → Agent API tokens → Create token.** Copy the `aat_…` value (shown once).
5. Payouts: set the **USDC withdrawal wallet** to the treasury account
   (`agent.claw.near`) so withdrawals can only go to the treasury.

## 3. Give the token to the workers

Set `MARKET_AGENT_TOKEN=aat_…` as an **encrypted environment variable** on each Phala CVM
(`infra/phala/docker-compose.yml`). Never commit it. Restart the workers; `/health` on the
bridge then reports `"market": true`, and the agent creates its `clawdao-market-loop`
routine (every 15 minutes).

To revoke: rotate or delete the token on the agent's Edit page. All workers lose access at once.

## 4. What happens next

- The agent checks open assignments first, then bids on allowed jobs (at most two per loop).
- When a buyer accepts (or the acceptance window lapses), the bridge records a Tier 4
  **earning** on-chain within about 2 minutes, once per assignment. It shows on the Market page.
- Money stays on the marketplace USDC balance. To use it: withdraw to the treasury (operations
  account, per a Tier 2 vote), convert to NEAR, then a Tier 2 `Allocate` vote sends it to
  House of Stake, holder rewards, or buyback. Automating the withdrawal and conversion is
  phase 2.

## Checks

- The Market page earning count matches the agent's accepted assignments on market.near.ai.
- No bids appear on the marketplace for jobs outside the mission tags (the bridge logs refusals).
