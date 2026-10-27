import { initTRPC } from "@trpc/server";
import { and, asc, desc, eq, inArray, lt, sql } from "drizzle-orm";
import { z } from "zod";
import {
  agentWorkers, blocksCursor, bountySubmissions, constitutionVersions, events, hosMandates, hosProposals, missions, proposals,
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

  fees: t.router({
    /** Live fee router state: mode, market-cap gate, totals routed. */
    state: t.procedure.query(({ ctx }) => ctx.views.feeState()),
    /** Recent fee-flow events (distributions, buybacks, mode changes, market-cap checks). */
    events: t.procedure.input(z.object({ limit: z.number().int().min(1).max(200).default(50) })).query(({ ctx, input }) =>
      ctx.db
        .select()
        .from(events)
        .where(
          inArray(events.kind, [
            "fees_distributed", "buyback", "fee_mode_set", "fee_mode_unlocked", "market_cap_observed",
            "rewards_deposited", "hos_lock_fees", "treasury_allocated", "dev_wallet_set",
          ]),
        )
        .orderBy(desc(events.height))
        .limit(input.limit),
    ),
    /** Holder rewards: what an account can claim, and totals ever paid in. */
    rewards: t.procedure.input(z.object({ account: z.string().optional() })).query(async ({ ctx, input }) => {
      const [total, waiting] = await ctx.views.rewardsInfo();
      const pending = input.account ? await ctx.views.pendingRewards(input.account) : "0";
      return { total, waiting, pending };
    }),
  }),

  hos: t.router({
    /** HoS proposals with clawdao's mandate, what was cast, and the clawdao vote that decided it. */
    proposals: t.procedure.input(z.object({ limit: z.number().int().min(1).max(100).default(30) })).query(async ({ ctx, input }) => {
      const rows = await ctx.db
        .select({ p: hosProposals, m: hosMandates })
        .from(hosProposals)
        .leftJoin(hosMandates, eq(hosMandates.hosProposalId, hosProposals.id))
        .orderBy(desc(hosProposals.id))
        .limit(input.limit);
      const claw = await ctx.db
        .select({ id: proposals.id, status: proposals.status, kindPayload: proposals.kindPayload, endsAt: proposals.endsAt })
        .from(proposals)
        .where(eq(proposals.kind, "HosVote"));
      return rows.map(({ p, m }) => ({
        ...p,
        mandate: m,
        clawdaoVotes: claw.filter((c) => (c.kindPayload as { HosVote?: { hos_proposal_id: number } }).HosVote?.hos_proposal_id === p.id),
      }));
    }),
    /** Live treasury position from chain. */
    position: t.procedure.query(async ({ ctx }) => {
      const info = await ctx.views.hosInfo();
      const venear = info.config
        ? await ctx.views.rpc.view<string>(info.config.venear_id, "ft_balance_of", { account_id: ctx.views.accounts.agent }).catch(() => "0")
        : "0";
      const lockupBalance = info.lockup_account_id
        ? await ctx.views.rpc.view<string>(info.lockup_account_id, "get_balance").catch(() => null)
        : null;
      const stakingPool = info.lockup_account_id
        ? await ctx.views.rpc.view<string | null>(info.lockup_account_id, "get_staking_pool_account_id").catch(() => null)
        : null;
      return { ...info, venear, lockupBalance, stakingPool };
    }),
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
