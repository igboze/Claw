# Tokenomics: fees, staking and allocation

## Trading fee

- **2% on every buy and sell.** The CLAW token takes `FEE_BPS = 200` on any transfer to or
  from a listed DEX contract (Rhea/Ref `v2.ref-finance.near` at launch). The rate is fixed in
  code. Only a Tier 1 vote can list or delist DEX contracts (`token.set_fee_pools`).
- Wallet-to-wallet transfers, locking in `ve`, and the fee router's own swaps pay nothing.
- The fee arrives as CLAW in the fee router (`contracts/fees`).

### From CLAW fee to NEAR

The fee router runs permissionless steps, driven every 30 minutes by the jobs keeper:

1. `swap_fees`: sell up to `max_swap_per_call` CLAW for wNEAR on the CLAW/wNEAR pool, with a
   minimum output of the on-chain quote less `max_slippage_bps` (1% at launch).
2. `unwrap`: wNEAR → NEAR.
3. `distribute`: **20% to the dev wallet, 80% by mode.**
4. `buyback` (Buyback mode only): NEAR → CLAW on the same pool, then `ft_burn`.

| Mode | 80% goes to | Who can choose it |
| --- | --- | --- |
| `StakeHoS` (default) | Agent contract `hos_lock_fees` → locked in the treasury's House of Stake lockup | Fixed until the gate opens |
| `HolderRewards` | `ve.deposit_rewards` → claimable NEAR for locked-CLAW holders, pro-rata by amount | Tier 2 `SetFeeMode` after the gate |
| `Buyback` | Buy CLAW and burn it | Tier 2 `SetFeeMode` after the gate |

**The $100k gate.** `observe_market_cap` prices CLAW from the CLAW/wNEAR and wNEAR/USD pools:
market cap = supply × NEAR per CLAW × USD per NEAR. When it is at or above $100,000 on
**3 consecutive UTC days**, the fee-mode vote unlocks permanently. Before that, `set_mode`
reverts. A pump only unlocks the *ability* to vote; holders still decide.

**Dev wallet.** 20% of fee NEAR. A Tier 1 vote can change the address or lower the share; the
contract rejects anything above 20%.

## What House of Stake staking earns

Read from mainnet `venear.dao` on 2026-10-27 (details in [house-of-stake.md](house-of-stake.md)):

| Source | Rate | Paid as | Usable for |
| --- | --- | --- | --- |
| veNEAR growth | ≈50%/year of locked NEAR (`annual_growth_rate_ns` 1.585e-17) | Voting power only, forfeited on unlock | More weight in HoS votes; not money |
| Validator staking | Network staking yield less pool commission; low single digits after the 2025 inflation cut | NEAR in the lockup, withdrawable without unlocking | Allocation (below) |
| HoS holder rewards (HSP-003) | Two-week campaigns; ~7.5% annualized at launch, expected to fall | NEAR to the lockup when claimed | Allocation (below) |

Only the second and third produce NEAR. Eligibility of a contract-owned lockup for HSP-003
rewards still needs confirming with House of Stake.

## Allocation votes (staking rewards, marketplace earnings, any treasury NEAR)

1. Rewards are withdrawn from the lockup to the agent contract with a Tier 2 `HosManage`
   proposal (`hos_withdraw`; `hos_staking` with `RefreshStakingPoolBalance` first).
   Agent marketplace earnings reach the treasury the same way once converted to NEAR.
2. A Tier 2 `Allocate { amount, destination }` proposal sends it to:
   - `StakeHoS`: lock it too (compounding the position, within `max_locked`),
   - `HolderRewards`: pay locked-CLAW holders,
   - `Buyback`: buy and burn CLAW.

Suggested default policy for holders to adopt: compound staking rewards into HoS until the
fee gate opens, then split rewards between holder rewards and buyback.

## Holder rewards mechanics

`ve.deposit_rewards` adds NEAR to a reward-per-token accumulator over **locked CLAW amount**
(not decaying voting power), so every locker earns in proportion to what they locked, from
the moment they locked. `claim_rewards` pays it out; the Lock page shows the claimable amount.

## Caveats to resolve before launch

- **DEX sell screens.** When a seller sends X CLAW to Rhea/Ref, the DEX receives 0.98·X. A swap
  message that names `amount_in = X` fails and refunds. Sells must quote with
  `token.net_of_fee(seller, dex, X)`. Ship a trade page, or work with Rhea so its UI reads the
  fee. Buys work unchanged (the buyer receives 98%). Adding and removing liquidity also pays
  the fee.
- **Rhea/Ref token whitelist.** CLAW needs Rhea to whitelist it (or users register it per
  account) before pools are usable.
- **Swap message format.** The router uses `{"force":0,"actions":[{pool_id, token_in,
  token_out, amount_in, min_amount_out}]}`. Verify on testnet against the deployed Rhea
  version.
- **Sandwiching.** The router's swaps can be front-run within the slippage bound. That's why
  `max_swap_per_call` and `max_slippage_bps` are small; tune them by Tier 1 vote.
- **Legal.** A trading fee that pays a dev wallet and can pay holders may change how CLAW is
  classified. The launch checklist's legal opinion (UK, Nigeria) must cover the fee, the dev
  share and holder rewards specifically.
