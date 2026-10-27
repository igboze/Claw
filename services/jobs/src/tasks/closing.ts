import { and, eq, gt, lt } from "drizzle-orm";
import { proposals } from "@clawdao/db";
import type { Deps } from "./deps.js";

/** Queue a "votes closing" alert once per proposal, about 24 hours before voting ends. */
export async function queueClosingVotes({ db, redis, notify }: Deps) {
  const now = new Date();
  const soon = new Date(now.getTime() + 24 * 3600_000);
  const closing = await db
    .select()
    .from(proposals)
    .where(and(eq(proposals.status, "Active"), gt(proposals.endsAt, now), lt(proposals.endsAt, soon)));
  for (const p of closing) {
    const fresh = await redis.set(`notified:closing:${p.id}`, "1", "EX", 7 * 86_400, "NX");
    if (fresh) await notify.add("votes-closing", { proposalId: p.id, title: p.title, endsAt: p.endsAt.toISOString() });
  }
  return closing.length;
}
