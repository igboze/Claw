// The on-chain work mission is the only authority for what the agent may take on Agent
// Market. The marketplace cannot enforce it, so the bridge does, before every bid.

import type { MissionView } from "@clawdao/types";
import { toCents, type MarketBid, type MarketJob, type MarketAssignmentRow } from "./market.js";

export type WorkMission = MissionView & { work: NonNullable<MissionView["work"]> };

export function workMissions(missions: MissionView[]): WorkMission[] {
  return missions.filter((m): m is WorkMission => m.status === "Active" && !!m.work);
}

const norm = (t: string) => t.trim().toLowerCase();

export function tagsMatch(mission: WorkMission, job: Pick<MarketJob, "tags">): boolean {
  const allowed = new Set(mission.work.tags.map(norm));
  return job.tags.some((t) => allowed.has(norm(t)));
}

/** The work mission a job belongs to: the first active one whose tags match. Deterministic,
 * so every worker attributes the same earning to the same mission. */
export function missionForJob(missions: WorkMission[], job: Pick<MarketJob, "tags">): WorkMission | undefined {
  return [...missions].sort((a, b) => a.id.localeCompare(b.id)).find((m) => tagsMatch(m, job));
}

export interface BidCheck {
  mission: WorkMission;
  job: MarketJob;
  amount: string;
  openAssignments: MarketAssignmentRow[];
  pendingBids: MarketBid[];
}

/** Returns why a bid is not allowed, or null when it is. */
export function checkBid({ mission, job, amount, openAssignments, pendingBids }: BidCheck): string | null {
  if (job.jobKind !== "standard" || !["open", "filling"].includes(job.status)) return "Job is not taking bids";
  if (!tagsMatch(mission, job)) return `Job tags [${job.tags.join(", ")}] do not match mission ${mission.id}`;
  if (!mission.work.currencies.includes(job.budgetToken)) return `Currency ${job.budgetToken} not allowed by mission ${mission.id}`;
  let cents: number;
  try {
    cents = toCents(amount);
  } catch {
    return "Amount must be a decimal number";
  }
  if (cents < mission.work.min_price_cents) return `Bid below the mission minimum of $${(mission.work.min_price_cents / 100).toFixed(2)}`;
  const open = openAssignments.filter((a) => a.assignment.status === "in_progress" || a.assignment.status === "submitted").length;
  if (open + pendingBids.length >= mission.work.max_open) return `Already at max_open (${mission.work.max_open})`;
  if (pendingBids.some((b) => b.jobId === job.jobId)) return "Already bid on this job";
  return null;
}
