import { Hono } from "hono";
import { z } from "zod";
import type { ShadeClient } from "@neardefi/shade-agent-js";
import type { ClawViews } from "@clawdao/near";
import type { AgentAction } from "@clawdao/types";
import type { BridgeConfig } from "./config.js";
import { EvmSigner } from "./evm.js";
import { taskId } from "./task.js";
import { MarketClient } from "./market.js";
import { marketRoutes } from "./market-routes.js";

const U128 = z.string().regex(/^\d+$/);
const accountId = z.string().min(2).max(64);

const Action = z.union([
  z.object({ NativeTransfer: z.object({ receiver_id: accountId, amount: U128 }) }),
  z.object({ FtTransfer: z.object({ token_id: accountId, receiver_id: accountId, amount: U128 }) }),
  z.object({
    ChainSignature: z.object({
      path: z.string().max(128),
      payload_hex: z.string().regex(/^[0-9a-f]+$/),
      key_type: z.enum(["Ecdsa", "Eddsa"]),
      asset: z.string(),
      value: U128,
    }),
  }),
]);

const ProposeBody = z.object({
  mission_id: z.string().min(1).max(64),
  constitution_hash: z.string().regex(/^[0-9a-f]{64}$/),
  loop_epoch: z.number().int().nonnegative(),
  seq: z.number().int().min(0).max(31).default(0),
  action: Action,
  reasoning_uri: z.string().min(1).max(512),
});

const PrepareEvmBody = z.object({
  chain: z.string(),
  path: z.string().max(128),
  to: z.string().regex(/^0x[0-9a-fA-F]{40}$/),
  data: z.string().regex(/^0x[0-9a-fA-F]*$/).optional(),
  value: U128.default("0"),
});

const GAS_300T = "300000000000000";

export function buildApp(deps: { shade: ShadeClient; views: ClawViews; cfg: BridgeConfig }) {
  const { shade, views, cfg } = deps;
  const evm = new EvmSigner(cfg.network, cfg.mpcContractId, cfg.evmRpcUrls, cfg.accounts.agent);
  const app = new Hono();

  app.onError((err, c) => {
    console.error(err);
    return c.json({ error: err.message }, 500);
  });

  app.get("/health", (c) => c.json({ ok: true, worker: shade.accountId(), market: !!cfg.marketToken }));

  // Agent Market (self-hosted seller). Bids are checked against on-chain work missions.
  if (cfg.marketToken) {
    app.route("/market", marketRoutes(new MarketClient(cfg.marketToken, cfg.marketApiUrl), views));
  } else {
    app.all("/market/*", (c) => c.json({ error: "Agent Market not configured (MARKET_AGENT_TOKEN)" }, 503));
  }

  // ---- reads (chain is the source of truth; no cache) ----
  app.get("/constitution", async (c) => c.json(await views.constitution()));
  app.get("/missions", async (c) => c.json(await views.activeMissions()));
  app.get("/agent", async (c) => c.json(await views.agentInfo()));
  app.post("/view", async (c) => {
    const body = z
      .object({ contract: accountId, method: z.string().regex(/^(get_|ft_|storage_balance|power_|is_)\w*$/), args: z.record(z.unknown()).default({}) })
      .parse(await c.req.json());
    return c.json(await views.rpc.view(body.contract, body.method, body.args));
  });

  // ---- writes ----

  /** Build an unsigned EVM tx from the treasury's derived address; returns the payload to sign. */
  app.post("/prepare-evm", async (c) => {
    const b = PrepareEvmBody.parse(await c.req.json());
    const p = await evm.prepare(b.chain, b.path, b.to, b.data, BigInt(b.value));
    return c.json({ payload_hex: p.payloadHex, from: await evm.address(b.chain, b.path) });
  });

  /**
   * The only way to move value. The agent contract runs spend_check and either executes
   * (NEAR transfers) or returns an MPC signature, which we broadcast and link back.
   */
  app.post("/propose-action", async (c) => {
    const b = ProposeBody.parse(await c.req.json());
    const id = taskId(b.mission_id, b.loop_epoch, b.seq);
    const action = b.action as AgentAction;
    if ("ChainSignature" in action && !evm.has(action.ChainSignature.payload_hex)) {
      return c.json({ error: "Unknown payload: call /prepare-evm first" }, 400);
    }
    const result = await shade.call<Record<string, unknown>>({
      methodName: "propose_action",
      args: {
        task_id: id,
        mission_id: b.mission_id,
        constitution_hash: b.constitution_hash,
        action,
        reasoning_uri: b.reasoning_uri,
      },
      gas: GAS_300T,
    });

    let foreign: { chain: string; txHash: string } | undefined;
    if ("ChainSignature" in action) {
      if (!result) return c.json({ task_id: id, status: "rejected_or_failed" }, 409);
      foreign = await evm.broadcast(action.ChainSignature.payload_hex, result as any);
      await shade.call({
        methodName: "post_receipt",
        args: { task_id: id, chain: foreign.chain, tx_hash: foreign.txHash },
      });
    }
    return c.json({ task_id: id, status: "executed", foreign_tx: foreign ?? null });
  });

  app.post("/heartbeat", async (c) => {
    const { loop_epoch } = z.object({ loop_epoch: z.number().int() }).parse(await c.req.json());
    await shade.call({ methodName: "heartbeat", args: { loop_epoch: String(loop_epoch) } });
    return c.json({ ok: true });
  });

  /** Pin a reasoning log; its URI goes into the receipt. */
  app.post("/publish-log", async (c) => {
    const body = z.object({ name: z.string().max(128), content: z.unknown() }).parse(await c.req.json());
    if (!cfg.ipfsApiUrl || !cfg.ipfsJwt) return c.json({ error: "IPFS not configured" }, 503);
    const res = await fetch(`${cfg.ipfsApiUrl}/pinning/pinJSONToIPFS`, {
      method: "POST",
      headers: { "content-type": "application/json", authorization: `Bearer ${cfg.ipfsJwt}` },
      body: JSON.stringify({ pinataMetadata: { name: body.name }, pinataContent: body.content }),
    });
    if (!res.ok) return c.json({ error: `IPFS pin failed: ${res.status}` }, 502);
    const { IpfsHash } = (await res.json()) as { IpfsHash: string };
    return c.json({ uri: `ipfs://${IpfsHash}` });
  });

  return app;
}
