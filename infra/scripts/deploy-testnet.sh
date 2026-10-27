#!/usr/bin/env bash
# Deploys and initialises all five contracts on testnet with near-cli-rs (`near`).
# Run from the repo root after `contracts/build.sh`. ROOT must be a testnet account you
# control; sub-accounts are created under it.
#
# Order matters: the token mints to ROOT, ve and governor point at each other, and the
# constitution and agent are wired to the governor. On mainnet the final step deletes
# every deploy key (see docs/runbooks/launch.md); this testnet script keeps them.

set -euo pipefail
: "${ROOT:?set ROOT=<your-account>.testnet}"
NET=testnet
W=contracts/target/near
TOKEN=claw.$ROOT VE=ve.$ROOT GOV=gov.$ROOT CON=constitution.$ROOT AGENT=agent.$ROOT FEES=fees.$ROOT
DEV_WALLET=${DEV_WALLET:?set DEV_WALLET (receives 20% of trading fees)}
DEX=${DEX:-ref-finance-101.testnet} WRAP=${WRAP:-wrap.testnet}
# The CLAW/wNEAR pool exists only after the token is deployed and liquidity added, so the
# fee router starts with pool 0; then, once, as ROOT:
#   near contract call-function as-transaction $FEES launch_set_pools json-args '{"claw_pool_id":N,"usd_pool_id":M}' ...
CLAW_POOL_ID=${CLAW_POOL_ID:-0}
USD_POOL_ID=${USD_POOL_ID:-0} USD_TOKEN=${USD_TOKEN:?set the USD stablecoin account}
SUPPLY=${SUPPLY:-1000000000000000000000000000000000}   # 1e9 CLAW, 24 decimals
GUARDIAN=${GUARDIAN:-$ROOT}
GUARDIAN_EXPIRY_BLOCK=${GUARDIAN_EXPIRY_BLOCK:?set the block height where the guardian expires}
DAY=86400000000000

create() { near account create-account fund-myself "$1" "$2 NEAR" autogenerate-new-keypair save-to-keychain sign-as "$ROOT" network-config $NET sign-with-keychain send; }
deploy() { near contract deploy "$1" use-file "$2" without-init-call network-config $NET sign-with-keychain send; }
call()   { near contract call-function as-transaction "$1" "$2" json-args "$3" prepaid-gas '300.0 Tgas' attached-deposit "${4:-0 NEAR}" sign-as "${5:-$1}" network-config $NET sign-with-keychain send; }

for a in "$TOKEN:5" "$VE:5" "$GOV:10" "$CON:10" "$AGENT:20" "$FEES:5"; do create "${a%%:*}" "${a##*:}"; done

deploy "$TOKEN" $W/claw_token/claw_token.wasm
deploy "$VE" $W/claw_ve/claw_ve.wasm
deploy "$GOV" $W/claw_governor/claw_governor.wasm
deploy "$CON" $W/claw_constitution/claw_constitution.wasm
deploy "$AGENT" $W/claw_agent/claw_agent.wasm
deploy "$FEES" $W/claw_fees/claw_fees.wasm

call "$TOKEN" new "{\"distribution_id\":\"$ROOT\",\"total_supply\":\"$SUPPLY\",\"metadata\":{\"spec\":\"ft-1.0.0\",\"name\":\"IronClaw Coin\",\"symbol\":\"CLAW\",\"decimals\":24},\"governor_id\":\"$GOV\",\"fee_collector\":\"$FEES\",\"fee_pools\":[\"$DEX\"]}"
call "$VE" new "{\"token_id\":\"$TOKEN\",\"governor_id\":\"$GOV\",\"params\":{\"min_months\":1,\"max_months\":24,\"min_lock_amount\":\"1000000000000000000000000\",\"max_power_per_account\":\"50000000000000000000000000000000\"}}"
call "$GOV" new "{\"ve_id\":\"$VE\",\"constitution_id\":\"$CON\",\"agent_id\":\"$AGENT\",\"fees_id\":\"$FEES\",\"hos_voting_period_ns\":\"$((2*DAY))\",\"proposal_bond\":\"1000000000000000000000000\",\"tiers\":[
  [1,{\"voting_period_ns\":\"$((7*DAY))\",\"timelock_ns\":\"$((14*DAY))\",\"quorum\":\"50000000000000000000000000000000\",\"threshold_bps\":6667}],
  [2,{\"voting_period_ns\":\"$((5*DAY))\",\"timelock_ns\":\"$((7*DAY))\",\"quorum\":\"30000000000000000000000000000000\",\"threshold_bps\":6000}],
  [3,{\"voting_period_ns\":\"$((3*DAY))\",\"timelock_ns\":\"$((2*DAY))\",\"quorum\":\"10000000000000000000000000000000\",\"threshold_bps\":5001}]]}"
ARTICLES=$(node infra/scripts/genesis-to-json.mjs constitution/genesis.md)
call "$CON" new "{\"governor_id\":\"$GOV\",\"agent_id\":\"$AGENT\",\"articles\":$ARTICLES,\"params\":{\"loop_interval_minutes\":10,\"daily_caps\":[[\"near\",\"500000000000000000000000000\"]]}}"
call "$AGENT" new "{\"requires_tee\":${REQUIRES_TEE:-true},\"attestation_expiration_time_ms\":\"604800000\",\"governor_id\":\"$GOV\",\"constitution_id\":\"$CON\",\"mpc_contract_id\":\"v1.signer-prod.testnet\",\"guardian_id\":\"$GUARDIAN\",\"guardian_expiry_block\":\"$GUARDIAN_EXPIRY_BLOCK\"}"

call "$FEES" new "{\"token_id\":\"$TOKEN\",\"governor_id\":\"$GOV\",\"agent_id\":\"$AGENT\",\"ve_id\":\"$VE\",\"dex\":{\"dex_id\":\"$DEX\",\"wrap_id\":\"$WRAP\",\"claw_pool_id\":$CLAW_POOL_ID,\"usd_pool_id\":$USD_POOL_ID,\"usd_token_id\":\"$USD_TOKEN\",\"usd_decimals\":6},\"limits\":{\"max_swap_per_call\":\"5000000000000000000000000000000\",\"max_buyback_per_call\":\"100000000000000000000000000\",\"max_slippage_bps\":100},\"dev_wallet\":\"$DEV_WALLET\",\"mcap_threshold_usd\":\"100000\"}"
call "$FEES" setup "{\"wrap_storage\":\"1250000000000000000000\",\"dex_storage\":\"100000000000000000000000\"}" '0.11125 NEAR' "$ROOT"

# Register ve and the agent treasury on the token so locks and payouts work.
for acct in "$VE" "$AGENT"; do
  call "$TOKEN" storage_deposit "{\"account_id\":\"$acct\"}" '0.0125 NEAR' "$ROOT"
done

cat <<EOF
Deployed:
  TOKEN_CONTRACT=$TOKEN
  FEES_CONTRACT=$FEES
  VE_CONTRACT=$VE
  GOVERNOR_CONTRACT=$GOV
  CONSTITUTION_CONTRACT=$CON
  AGENT_CONTRACT=$AGENT
Next: approve the agent image measurements with a Tier 1 proposal (see docs/runbooks/launch.md).
EOF
