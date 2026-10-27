# House of Stake integration

clawdao holds veNEAR in [House of Stake](https://github.com/houseofstake/house-of-stake-contracts)
(mainnet: `venear.dao`, `vote.dao`) and CLAW holders decide how it votes.

## How it works

1. **Lockup.** The agent contract registers on `venear.dao` and deploys its own lockup
   (`hos_register`, Tier 1). The lockup is non-upgradable and only its owner, the agent
   contract, can move funds.
2. **Funding.** NEAR enters the lockup through `hos_lock` and is locked (`lock_near`), up to
   `max_locked`. Locked NEAR can be staked with a whitelisted pool (`hos_staking`). Unlocking
   takes 45 days on mainnet and forfeits accrued veNEAR growth.
   *Planned:* 80% of trading-fee revenue flows here automatically (see the fee design).
3. **Voting.** For any HoS proposal in `Voting`, anyone opens a clawdao `HosVote` proposal
   (Tier 3 quorum, 2-day vote by default, no timelock). If quorum is met, the option with the
   most CLAW voting power becomes the mandate; ties go to Abstain. Executing the proposal calls
   `agent.hos_set_mandate`.
4. **Casting.** The jobs service relayer (or anyone) calls `agent.hos_vote(proposal_id,
   merkle_proof, v_account)` with the proof from `venear.get_proof(agent)` at the HoS snapshot
   block. The agent contract casts only the stored mandate and `vote.dao` verifies the proof.
   A later clawdao vote can replace the mandate while HoS voting is open.
5. **Delegation.** `hos_set_delegations` (Tier 1) can split the treasury's veNEAR among
   delegates.

| Action | Who | Proposal |
| --- | --- | --- |
| `hos_configure`, `hos_register`, `hos_set_delegations` | Governor | Tier 1 `FunctionCall` |
| `hos_lock`, `hos_begin_unlock`, `hos_end_unlock`, `hos_withdraw`, `hos_staking` | Governor | Tier 2 `HosManage` |
| `hos_set_mandate` | Governor | Tier 3 `HosVote` |
| `hos_vote` | Anyone (relayer) | — |

## What the position earns

Read from mainnet on 2026-10-27:

| Source | What | Paid as |
| --- | --- | --- |
| veNEAR growth | `annual_growth_rate_ns` = 1.585e-17/ns ≈ **50% per year** of locked NEAR, added as extra veNEAR | Voting power only; forfeited on unlock |
| Validator staking | Locked NEAR staked through the lockup to a whitelisted pool | NEAR in the lockup, withdrawable without unlocking |
| HoS veNEAR holder rewards (HSP-003) | Two-week campaigns, proportional to amount locked; launched at ~7.5% annualized, expected to fall | NEAR to the lockup when claimed |

Network snapshot: about 3.94M NEAR locked, 5.32M total veNEAR, 497 accounts, 30 proposals.

Open: confirm a contract-owned lockup is eligible for HSP-003 rewards and how claiming works.

## Operations

- `jobs` syncs `vote.dao` proposals every 10 minutes (`hos_proposals` table), relays due
  mandates (`RELAYER_ACCOUNT_ID` / `RELAYER_PRIVATE_KEY`, a gas-only key), and snapshots the
  position hourly. Proofs need an archival RPC (`ARCHIVAL_RPC_URL`).
- The web app's House of Stake page lists HoS proposals, opens clawdao votes, and shows the
  mandate and whether it was cast.
