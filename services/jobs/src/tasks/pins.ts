import { desc } from "drizzle-orm";
import { receipts } from "@clawdao/db";
import { gatewayUrl, type Deps } from "./deps.js";

/** Every receipt cites a reasoning log; alert if a recent one stops resolving. */
export async function checkPins({ db, notify }: Deps) {
  const recent = await db.select().from(receipts).orderBy(desc(receipts.id)).limit(50);
  const missing: string[] = [];
  for (const r of recent) {
    const url = gatewayUrl(r.reasoningUri);
    const ok = url ? await fetch(url, { method: "HEAD" }).then((x) => x.ok).catch(() => false) : false;
    if (!ok) missing.push(`${r.id}:${r.reasoningUri}`);
  }
  if (missing.length) {
    await notify.add("ops", { text: `Reasoning logs not resolving: ${missing.join(", ")}` });
  }
  return missing.length;
}
