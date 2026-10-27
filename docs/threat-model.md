# Threat model

Top risks first, with where each defence lives in the code.

| # | Threat | Defence | Code |
| --- | --- | --- | --- |
| 1 | **Governance capture**: a whale buys and locks enough to amend Tier 1 | 66.67% supermajority, ≥7-day (14 at genesis) Tier 1 timelock that config cannot weaken, per-account voting power cap, ve snapshot at proposal creation | `governor::internal_set_tier`, `ve::Params.max_power_per_account`, `ve::power_at` |
| 2 | **Prompt injection** via bounty submissions or social replies | Outside text is passed as data (skill + config); value moves are capped by `spend_check` regardless of what the model decides; tools validate inputs | `SKILL.md`, `bounty-intake` `untrusted_data`, `constitution::spend_check`, `claw-tool-core` |
| 3 | **Malicious code hash** approved by proposal | Reproducible builds, image SHA256 in release notes, base image pinned by digest, long Tier 1 timelock for review | `agent-image.yml`, `agent/Dockerfile`, `contracts/build.sh` |
| 4 | **TEE compromise** | On-chain guardrails still hold (per-action, per-mission, per-day caps); guardian may pause until its expiry block; revoking measurements disables workers immediately | `agent::require_valid_agent`, `agent::pause` |
| 5 | **Double actions** from multiple workers | One-time `task_id = sha256(mission:epoch)` | `agent::propose_action`, `bridge/src/task.ts` |
| 6 | **Flash-loan votes** | Only locked CLAW (≥1 month) has power; power read at snapshot | `ve` |
| 7 | **Stale plan** after governance changes rules | Every action carries the hash it was planned under; stale hashes rejected | `spend_check` |
| 8 | **Indexer lies or lags** | Chain is the source of truth; rows carry block height; anyone can re-run the indexer from deploy height | `services/indexer`, `api /status` |
| 9 | **HoS vote not matching holders' choice** | Agent contract casts only the Governor-set mandate; `vote.dao` verifies the snapshot proof; relayer key pays gas only | `agent::hos_vote`, `jobs/tasks/hos.ts` |
| 10 | **HoS dependency risk**: `venear.dao` upgraded or paused by its owner/guardians | Funds sit in a non-upgradable lockup owned by the agent contract; lock size capped by `max_locked` | `agent/src/hos.rs` |
| 11 | **Fee swap sandwiching** | Router swaps use an on-chain quote less `max_slippage_bps` (1%) and at most `max_swap_per_call` per call; both tunable only by Tier 1 | `fees::swap_fees`, `fees::buyback` |
| 12 | **Market-cap gate pumped** | Needs 3 consecutive UTC days at ≥ $100k from pool prices; unlocking only enables a vote, holders still decide | `fees::record_observation` |
| 13 | **Fee diverted** | Rate fixed in code; dev share capped at 20% in code; dev wallet and DEX list change only by Tier 1; mode changes only by Tier 2 after the gate | `token`, `fees::set_dev_wallet` |
| 14 | **Agent takes off-mission market work** | Bridge checks every bid against the on-chain work mission (tags, currency, min price, max open); token can't edit listing or payouts; job text treated as data | `bridge/work-policy.ts`, `clawdao-market` skill |
| 15 | **False earnings reported** | Only attested workers can report; once per assignment id; each earning names the assignment, so anyone can check it on Agent Market | `agent::report_earning`, `constitution::record_earning` |
| 16 | **Spoofed bounty payee** | TODO before mainnet: NEP-413 signed submission | `bounty-intake` |

## Assumptions

- NEAR consensus, the MPC network (`v1.signer`), and Intel TDX/Dstack attestation hold.
- At least one honest reviewer checks each Tier 1 proposal during its timelock.
- The upstream IronClaw image at the pinned digest is reviewed before approval.

## Out of scope

Price oracle manipulation (no on-chain oracle is used for caps; caps are in asset units),
social engineering of holders, availability of RPC providers.
