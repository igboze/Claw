// Telegram bot and X posts for new proposals, votes closing, and agent receipts.
// Reads indexed events (never the agent's own claims). Voting always links to the wallet.

import { Bot } from "grammy";
import { Worker } from "bullmq";
import { Redis } from "ioredis";
import { and, asc, gt, inArray } from "drizzle-orm";
import { createDb, events, missions } from "@clawdao/db";
import { ClawViews, NearRpc, accountsFromEnv, formatAmount } from "@clawdao/near";
import { formatEvent } from "./format.js";

const WEB = process.env.PUBLIC_WEB_URL ?? "https://clawdao.org";
const chatId = process.env.TELEGRAM_CHAT_ID;
const redis = new Redis(process.env.REDIS_URL ?? "redis://localhost:6379", { maxRetriesPerRequest: null });
const { db } = createDb();
const views = new ClawViews(new NearRpc(process.env.NEAR_RPC_URL ?? "https://rpc.testnet.near.org"), accountsFromEnv());
const bot = process.env.TELEGRAM_BOT_TOKEN ? new Bot(process.env.TELEGRAM_BOT_TOKEN) : null;

async function broadcast(text: string) {
  if (bot && chatId) await bot.api.sendMessage(chatId, text, { link_preview_options: { is_disabled: true } });
  if (process.env.X_USER_TOKEN) {
    await fetch("https://api.x.com/2/tweets", {
      method: "POST",
      headers: { "content-type": "application/json", authorization: `Bearer ${process.env.X_USER_TOKEN}` },
      body: JSON.stringify({ text: text.slice(0, 280) }),
    }).catch((e) => console.error("x post failed", e));
  }
  if (!bot) console.log(`[notify] ${text}`);
}

// ---- event feed: poll new indexed events of the kinds people care about ----
const KINDS = ["proposal_created", "proposal_finalized", "proposal_executed", "receipt", "mission_set", "paused", "constitution_updated", "hos_mandate", "hos_vote"];

async function pollEvents() {
  const last = Number((await redis.get("notifier:last_event_id")) ?? 0);
  const rows = await db
    .select()
    .from(events)
    .where(and(gt(events.id, last), inArray(events.kind, KINDS)))
    .orderBy(asc(events.id))
    .limit(50);
  for (const e of rows) {
    const text = formatEvent(e.kind, e.payload as Record<string, unknown>, WEB);
    if (text) await broadcast(text);
    await redis.set("notifier:last_event_id", String(e.id));
  }
}
setInterval(() => pollEvents().catch((e) => console.error("poll failed", e)), 15_000);

// ---- queued alerts from jobs (votes closing, ops) ----
new Worker(
  "notify",
  async (job) => {
    if (job.name === "votes-closing") {
      const { proposalId, title, endsAt } = job.data as { proposalId: number; title: string; endsAt: string };
      await broadcast(`⏳ Voting closes ${new Date(endsAt).toUTCString()} on #${proposalId} "${title}". Vote in your wallet: ${WEB}/proposals/${proposalId}`);
    } else if (job.name === "ops" && process.env.TELEGRAM_OPS_CHAT_ID && bot) {
      await bot.api.sendMessage(process.env.TELEGRAM_OPS_CHAT_ID, String(job.data.text));
    }
  },
  { connection: redis },
);

// ---- bot commands mirror read-only flows ----
if (bot) {
  bot.command("start", (c) => c.reply("clawdao bot. /missions /status /vote. Voting itself always happens in your wallet."));
  bot.command("missions", async (c) => {
    const ms = await db.select().from(missions).where(inArray(missions.status, ["Active"]));
    if (!ms.length) return c.reply("No active missions.");
    await c.reply(
      ms.map((m) => `• ${m.title}: ${formatAmount(m.spent)} / ${formatAmount(m.budget)} ${m.asset}, ends ${m.deadline.toISOString().slice(0, 10)}`).join("\n"),
    );
  });
  bot.command("status", async (c) => {
    const [version, hash] = await views.hash();
    const info = await views.agentInfo();
    await c.reply(`Constitution v${version} (${hash.slice(0, 12)}…)\nAgent paused: ${info.paused ? "yes" : "no"}\n${WEB}/agent`);
  });
  bot.command("vote", (c) => c.reply(`Open proposals: ${WEB}/proposals`));
  bot.start();
}

console.log("notifier running");
