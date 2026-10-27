import { eq, isNull } from "drizzle-orm";
import { constitutionVersions } from "@clawdao/db";
import type { ConstitutionView } from "@clawdao/types";
import type { Deps } from "./deps.js";

/**
 * The indexer records each new version and hash; this fills in the full article text by
 * reading `get_constitution` at that block (needs an archival RPC for old heights), so the
 * web app can show history and diffs.
 */
export async function fillConstitutionVersions({ db, views }: Deps) {
  const missing = await db.select().from(constitutionVersions).where(isNull(constitutionVersions.articles)).limit(20);
  let filled = 0;
  for (const v of missing) {
    try {
      const c = await views.rpc.view<ConstitutionView>(views.accounts.constitution, "get_constitution", {}, v.height);
      if (c.hash !== v.hash) continue; // several versions in one block: only the last is readable
      await db
        .update(constitutionVersions)
        .set({ articles: c.articles, params: c.params })
        .where(eq(constitutionVersions.version, v.version));
      filled++;
    } catch (e) {
      console.warn(`constitution v${v.version} at ${v.height}:`, (e as Error).message);
    }
  }
  return filled;
}
