// claw-bridge: the signer half of the agent image.
//
// - Holds the worker's NEAR key (derived inside the TEE by shade-agent-js) and no funds.
// - Registers the worker with the agent contract using a fresh Dstack attestation.
// - Serves a localhost-only API to IronClaw's tools. It never decides anything: every
//   value-moving request becomes `propose_action`, and the contracts decide.

import { serve } from "@hono/node-server";
import { ShadeClient } from "@neardefi/shade-agent-js";
import { ClawViews, NearRpc, accountsFromEnv } from "@clawdao/near";
import { loopEpoch } from "@clawdao/types";
import { buildApp } from "./app.js";
import { loadConfig } from "./config.js";
import { MarketClient } from "./market.js";
import { reportEarnings } from "./market-routes.js";

const cfg = loadConfig();

const shade = await ShadeClient.create({
  networkId: cfg.network,
  agentContractId: cfg.accounts.agent,
  sponsor: cfg.sponsor,
  // Only for local runs without a TEE: a fixed derivation path gives a stable account.
  derivationPath: cfg.localDerivationPath,
});
console.log(`worker account: ${shade.accountId()}`);

if (cfg.sponsor && (await shade.balance()) < 0.2) {
  await shade.fund(0.5);
}

async function register(): Promise<boolean> {
  const whitelisted = await shade.isWhitelisted();
  if (whitelisted === false) {
    console.log("local mode: waiting for the Governor to whitelist this worker");
    return false;
  }
  return shade.register();
}

while (!(await register().catch((e) => (console.error("register failed", e), false)))) {
  await new Promise((r) => setTimeout(r, 10_000));
}
console.log("worker registered");

// Attestations expire on-chain (7 days by default); re-register every 6 days.
setInterval(() => register().catch((e) => console.error("re-register failed", e)), 6 * 24 * 3600 * 1000);

const views = new ClawViews(new NearRpc(cfg.rpcUrl), accountsFromEnv());
const app = buildApp({ shade, views, cfg });

// Heartbeat once per loop so the status page can show worker uptime.
async function heartbeat() {
  const { params } = await views.constitution();
  const epoch = loopEpoch(Date.now(), params.loop_interval_minutes);
  await shade.call({ methodName: "heartbeat", args: { loop_epoch: String(epoch) } });
}
heartbeat().catch((e) => console.error("heartbeat failed", e));
setInterval(() => heartbeat().catch((e) => console.error("heartbeat failed", e)), 5 * 60_000);

// Record accepted Agent Market jobs on-chain as Tier 4 earnings.
if (cfg.marketToken) {
  const market = new MarketClient(cfg.marketToken, cfg.marketApiUrl);
  const report = () =>
    reportEarnings(market, views, shade)
      .then((n) => n && console.log(`recorded ${n} Agent Market earning(s)`))
      .catch((e) => console.error("earnings report failed", e));
  report();
  setInterval(report, 2 * 60_000);
}

// Localhost only: IronClaw runs in the same container; nothing outside may call this.
serve({ fetch: app.fetch, hostname: "127.0.0.1", port: cfg.port }, (info) => {
  console.log(`claw-bridge listening on 127.0.0.1:${info.port}`);
});
