import { Hono } from "hono";
import { z } from "zod";
import type { ShadeClient } from "@neardefi/shade-agent-js";
import type { ClawViews } from "@clawdao/near";
import { MarketClient, toCents } from "./market.js";
import { checkBid, missionForJob, workMissions } from "./work-policy.js";

/** Localhost routes the market tools call. Every bid is checked against the on-chain work mission. */
export function marketRoutes(market: MarketClient, views: ClawViews) {
  const app = new Hono();

  app.get("/board", async (c) => {
    const missions = workMissions(await views.activeMissions());
    const jobs = await market.board(c.req.query("q") || undefined, 50);
    // Only show jobs some active work mission could take, labelled with that mission.
    const eligible = jobs
      .map((j) => ({ job: j, mission: missionForJob(missions, j) }))
      .filter((x) => x.mission && x.mission.work.currencies.includes(x.job.budgetToken))
      .map(({ job, mission }) => ({
        job_id: job.jobId,
        title: job.title,
        description: job.description.slice(0, 2_000),
        tags: job.tags,
        budget: job.budgetAmount,
        currency: job.budgetToken,
        mission_id: mission!.id,
        min_price_cents: mission!.work.min_price_cents,
      }));
    return c.json({ untrusted_data: true, missions: missions.map((m) => m.id), jobs: eligible });
  });

  app.get("/assignments", async (c) => {
    const rows = await market.assignments();
    return c.json({
      untrusted_data: true,
      assignments: rows
        .filter((r) => ["in_progress", "submitted"].includes(r.assignment.status))
        .map((r) => ({
          assignment_id: r.assignment.assignmentId,
          job_id: r.job.jobId,
          title: r.job.title,
          brief: r.job.description.slice(0, 8_000),
          status: r.assignment.status,
          started: !!r.assignment.startedAt,
          deadline: r.assignment.slaDeadlineAt,
          price: r.assignment.escrowAmount,
          currency: r.assignment.escrowToken,
          latest_message: r.latestMessage?.body ?? null,
        })),
    });
  });

  app.post("/bid", async (c) => {
    const b = z
      .object({
        mission_id: z.string(),
        job_id: z.string().uuid(),
        amount: z.string().regex(/^\d+(\.\d+)?$/),
        proposal: z.string().min(20).max(4_000),
        sla_hours: z.number().int().min(1).max(24 * 14).optional(),
      })
      .parse(await c.req.json());
    const missions = workMissions(await views.activeMissions());
    const mission = missions.find((m) => m.id === b.mission_id);
    if (!mission) return c.json({ error: `No active work mission ${b.mission_id}` }, 403);
    const [job, rows, bids] = await Promise.all([market.job(b.job_id), market.assignments(), market.myBids()]);
    const reason = checkBid({ mission, job, amount: b.amount, openAssignments: rows, pendingBids: bids.filter((x) => x.status === "pending") });
    if (reason) return c.json({ error: reason }, 403);
    const bid = await market.bid(b.job_id, b.amount, b.proposal, b.sla_hours ? b.sla_hours * 3600 : undefined);
    return c.json({ bid_id: bid.bidId, status: bid.status, amount: bid.amount, amount_cents: toCents(bid.amount) });
  });

  app.post("/start", async (c) => {
    const { assignment_id } = z.object({ assignment_id: z.string().uuid() }).parse(await c.req.json());
    return c.json(await market.start(assignment_id));
  });

  app.post("/submit", async (c) => {
    const b = z
      .object({ assignment_id: z.string().uuid(), deliverable_markdown: z.string().min(1).max(200_000), files: z.array(z.string()).max(10).default([]) })
      .parse(await c.req.json());
    const a = await market.submit(b.assignment_id, b.deliverable_markdown, b.files);
    return c.json({ assignment_id: a.assignmentId, status: a.status, deliverable_hash: a.deliverableHash });
  });

  app.post("/message", async (c) => {
    const b = z.object({ assignment_id: z.string().uuid(), body: z.string().min(1).max(4_000) }).parse(await c.req.json());
    return c.json(await market.message(b.assignment_id, b.body));
  });

  return app;
}

/**
 * Record every accepted assignment on-chain as a Tier 4 earning, once. Several workers may
 * run this; the constitution rejects duplicates and we check first to save gas.
 */
export async function reportEarnings(market: MarketClient, views: ClawViews, shade: ShadeClient): Promise<number> {
  const accepted = await market.assignments("accepted");
  if (!accepted.length) return 0;
  const missions = (await views.missions()).filter((m) => m.work && !m.cancelled) as Parameters<typeof missionForJob>[0];
  let reported = 0;
  for (const row of accepted) {
    const a = row.assignment;
    const done = await views.rpc.view<boolean>(views.accounts.constitution, "is_earning_recorded", { assignment_id: a.assignmentId });
    if (done) continue;
    const mission = missionForJob(missions, row.job);
    if (!mission) {
      console.warn(`accepted assignment ${a.assignmentId} matches no work mission; not recorded`);
      continue;
    }
    await shade.call({
      methodName: "report_earning",
      args: {
        mission_id: mission.id,
        assignment_id: a.assignmentId,
        job_id: a.jobId,
        amount: a.escrowAmount,
        amount_cents: toCents(a.escrowAmount),
        currency: a.escrowToken,
        deliverable_hash: a.deliverableHash,
      },
      gas: "100000000000000",
    });
    reported++;
  }
  return reported;
}
