import Fastify from "fastify";
import cors from "@fastify/cors";
import rateLimit from "@fastify/rate-limit";
import { fastifyTRPCPlugin } from "@trpc/server/adapters/fastify";
import { desc, eq } from "drizzle-orm";
import { agentWorkers, blocksCursor, createDb, receipts } from "@clawdao/db";
import { ClawViews, NearRpc, accountsFromEnv } from "@clawdao/near";
import { appRouter } from "./router.js";

export type { AppRouter } from "./router.js";

const { db } = createDb();
const views = new ClawViews(new NearRpc(process.env.NEAR_RPC_URL ?? "https://rpc.testnet.near.org"), accountsFromEnv());

const app = Fastify({ logger: true, maxParamLength: 5000 });
await app.register(cors, { origin: true });
await app.register(rateLimit, { max: 300, timeWindow: "1 minute" });

await app.register(fastifyTRPCPlugin, {
  prefix: "/trpc",
  trpcOptions: { router: appRouter, createContext: () => ({ db, views }) },
});

app.get("/health", async () => ({ ok: true }));

/**
 * Public status page data: indexer lag, last receipt, worker heartbeats.
 * Alerts fire when there is no receipt for 2 loops or lag exceeds 60 seconds.
 */
app.get("/status", async () => {
  const [[cursor], [lastReceipt], workers, chain, constitution] = await Promise.all([
    db.select().from(blocksCursor).where(eq(blocksCursor.service, "indexer")),
    db.select().from(receipts).orderBy(desc(receipts.id)).limit(1),
    db.select().from(agentWorkers),
    views.rpc.block("final"),
    views.constitution(),
  ]);
  const loopMs = constitution.params.loop_interval_minutes * 60_000;
  const lagSeconds = cursor ? Math.max(0, (Date.now() - cursor.updatedAt.getTime()) / 1000) : null;
  const sinceReceiptMs = lastReceipt ? Date.now() - lastReceipt.blockTs.getTime() : null;
  return {
    chainHeight: chain.header.height,
    indexedHeight: cursor?.lastHeight ?? null,
    indexerLagSeconds: lagSeconds,
    lastReceiptAt: lastReceipt?.blockTs ?? null,
    workers: workers.map((w) => ({
      account: w.accountId,
      validUntil: w.validUntil,
      lastHeartbeat: w.lastHeartbeat,
      healthy: !!w.lastHeartbeat && Date.now() - w.lastHeartbeat.getTime() < 2 * loopMs,
    })),
    alerts: {
      noReceiptFor2Loops: sinceReceiptMs !== null && sinceReceiptMs > 2 * loopMs,
      indexerLagOver60s: lagSeconds !== null && lagSeconds > 60,
    },
  };
});

await app.listen({ host: "0.0.0.0", port: Number(process.env.API_PORT ?? 4000) });
