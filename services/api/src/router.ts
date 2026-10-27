import { initTRPC } from "@trpc/server";
import { and, asc, desc, eq, lt, sql } from "drizzle-orm";
import { z } from "zod";
import {
  agentWorkers, blocksCursor, bountySubmissions, constitutionVersions, missions, proposals,
  receipts, treasurySnapshots, veLocks, votes, type Db,
} from "@clawdao/db";
import type { ClawViews } from "@clawdao/near";

export interface Context {
  db: Db;
  views: ClawViews;
}

const t = initTRPC.context<Context>().create();
const page = z.object({ limit: z.number().int().min(1).max(100).default(25), cursor: z.number().int().optional() });

/**
 * Read-only API. Every fact (votes, spend, receipts) comes from indexed chain data with
 * the block height it was seen at, so the UI can show "confirmed" vs "pending".
 */
export const appRouter = t.router({
  /** Indexed head vs chain head; the UI marks rows above `indexedHeight` as pending. */
  head: t.procedure.query(async ({ ctx }) => {
    const [cursor] = await ctx.db.select().from(blocksCursor).where(eq(blocksCursor.service, "indexer"));
    const chain = await ctx.views.rpc.block("final");
    return {
      indexedHeight: cursor?.lastHeight ?? 0,
      chainHeight: chain.header.height,
      lagBlocks: chain.header.height - (cursor?.lastHeight ?? 0),
    };
  }),

  proposals: t.router({
    list: t.procedure
      .input(page.extend({ status: z.string().optional() }))
      .query(({ ctx, input }) =>
        ctx.db
          .select()
          .from(proposals)
          .where(and(input.status ? eq(proposals.status, input.status) : undefined, input.cursor !== undefined ? lt(proposals.id, input.cursor) : undefined))
          .orderBy(desc(proposals.id))
          .limit(input.limit),
      ),
    get: t.procedure.input(z.object({ id: z.number().int() })).query(async ({ ctx, input }) => {
      const [p] = await ctx.db.select().from(proposals).where(eq(proposals.id, input.id));
      if (!p) return null;
      const v = await ctx.db.select().from(votes).where(eq(votes.proposalId, input.id)).orderBy(desc(votes.power));
      return { ...p, votes: v };
    }),
  }),

  missions: t.router({
    list: t.procedure.query(({ ctx }) => ctx.db.select().from(missions).orderBy(asc(missions.deadline))),
    get: t.procedure.input(z.object({ id: z.string() })).query(async ({ ctx, input }) => {
      const [m] = await ctx.db.select().from(missions).where(eq(missions.id, input.id));
      if (!m) return null;
      const r = await ctx.db.select().from(receipts).where(eq(receipts.missionId, input.id)).orderBy(desc(receipts.id)).limit(100);
      return { ...m, receipts: r };
    }),
  }),

  receipts: t.router({
    list: t.procedure.input(page).query(({ ctx, input }) =>
      ctx.db
        .select()
        .from(receipts)
        .where(input.cursor !== undefined ? lt(receipts.id, input.cursor) : undefined)
        .orderBy(desc(receipts.id))
        .limit(input.limit),
    ),
  }),

  constitution: t.router({
    /** Live from chain: the agent reads the same thing every loop. */
    current: t.procedure.query(({ ctx }) => ctx.views.constitution()),
    versions: t.procedure.query(({ ctx }) =>
      ctx.db.select().from(constitutionVersions).orderBy(desc(constitutionVersions.version)),
    ),
    version: t.procedure.input(z.object({ version: z.number().int() })).query(async ({ ctx, input }) => {
      const [v] = await ctx.db.select().from(constitutionVersions).where(eq(constitutionVersions.version, input.version));
      return v ?? null;
    }),
  }),

  treasury: t.router({
    latest: t.procedure.query(({ ctx }) =>
      ctx.db
        .selectDistinctOn([treasurySnapshots.chain, treasurySnapshots.asset])
        .from(treasurySnapshots)
        .orderBy(treasurySnapshots.chain, treasurySnapshots.asset, desc(treasurySnapshots.takenAt)),
    ),
    history: t.procedure.input(z.object({ days: z.number().int().min(1).max(365).default(30) })).query(({ ctx, input }) =>
      ctx.db
        .select({
          day: sql<string>`date_trunc('day', ${treasurySnapshots.takenAt})`.as("day"),
          usd: sql<string>`sum(${treasurySnapshots.usdValue})`.as("usd"),
        })
        .from(treasurySnapshots)
        .where(sql`${treasurySnapshots.takenAt} > now() - make_interval(days => ${input.days})`)
        .groupBy(sql`1`)
        .orderBy(sql`1`),
    ),
  }),

  holders: t.procedure.input(page).query(({ ctx, input }) =>
    ctx.db.select().from(veLocks).orderBy(desc(veLocks.amount)).limit(input.limit),
  ),

  lock: t.procedure.input(z.object({ account: z.string() })).query(({ ctx, input }) => ctx.views.lock(input.account)),

  agent: t.procedure.query(async ({ ctx }) => {
    const [info, measurements, workers] = await Promise.all([
      ctx.views.agentInfo(),
      ctx.views.approvedMeasurements(),
      ctx.db.select().from(agentWorkers),
    ]);
    return { info, measurements, workers };
  }),

  bounties: t.procedure.input(z.object({ missionId: z.string() })).query(({ ctx, input }) =>
    ctx.db
      .select()
      .from(bountySubmissions)
      .where(eq(bountySubmissions.missionId, input.missionId))
      .orderBy(desc(bountySubmissions.createdAt))
      .limit(100),
  ),
});

export type AppRouter = typeof appRouter;
