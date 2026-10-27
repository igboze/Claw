#!/usr/bin/env bash
# Starts claw-bridge (signer), waits until it has registered with the agent contract,
# installs the clawdao tools and skills into IronClaw, then hands over to IronClaw.
set -euo pipefail

: "${AGENT_CONTRACT:?AGENT_CONTRACT is required}"
: "${DATABASE_URL:?DATABASE_URL (IronClaw's own Postgres inside the CVM) is required}"

# 1. Signer. Runs as an unprivileged user; it holds the worker key and no funds.
gosu ironclaw node /opt/claw-bridge/dist/index.js &
BRIDGE_PID=$!
trap 'kill $BRIDGE_PID 2>/dev/null || true' EXIT

for _ in $(seq 1 360); do
  if curl -fsS "http://127.0.0.1:${BRIDGE_PORT:-3140}/health" >/dev/null 2>&1; then break; fi
  if ! kill -0 "$BRIDGE_PID" 2>/dev/null; then echo "claw-bridge exited" >&2; exit 1; fi
  sleep 5
done

# 2. Tools and skills. Copied into IronClaw's data dir so they are part of this image's
#    code hash, never fetched at runtime.
mkdir -p /data/ironclaw-reborn/extensions/clawdao /data/ironclaw-reborn/skills
cp -r /opt/clawdao/tools/. /data/ironclaw-reborn/extensions/clawdao/
cp -r /opt/clawdao/ironclaw/skills/. /data/ironclaw-reborn/skills/
chown -R ironclaw:ironclaw /data/ironclaw-reborn

# 3. The mission loop routine is (re)created on boot from the versioned prompt, with the
#    interval read from the constitution (Tier 2) by the routine itself.
export CLAWDAO_BOOTSTRAP_PROMPT_FILE=/opt/clawdao/ironclaw/routines/mission-loop.md

# 4. Brain. LLM calls go to NEAR AI private inference (provider config in ironclaw/config.toml).
exec ironclaw-reborn-entrypoint "$@"
