// Scheduled and queued off-chain helpers. None of them can move value or change what
// the agent does; they only fill caches and send notifications.

import { Queue, Worker, type Job } from "bullmq";
import { Redis } from "ioredis";
import { createDb } from "@clawdao/db";
import { ClawViews, NearRpc, accountsFromEnv } from "@clawdao/near";
import { snapshotTreasury } from "./tasks/treasury.js";
import { fillConstitutionVersions } from "./tasks/constitution.js";
import { summarizeProposals } from "./tasks/summaries.js";
import { checkPins } from "./tasks/pins.js";
import { queueClosingVotes } from "./tasks/closing.js";
import { pullSocialMetrics } from "./tasks/social.js";
import { relayHosVotes, snapshotHosPosition, syncHosProposals } from "./tasks/hos.js";

const connection = new Redis(process.env.REDIS_URL ?? "redis://localhost:6379", { maxRetriesPerRequest: null });
const { db } = createDb();
const views = new ClawViews(new NearRpc(process.env.NEAR_RPC_URL ?? "https://rpc.testnet.near.org"), accountsFromEnv());
const notify = new Queue("notify", { connection });
const deps = { db, views, redis: connection, notify };

const tasks: Record<string, { every: number; run: (d: typeof deps) => Promise<unknown> }> = {
  "treasury-snapshot": { every: 60 * 60_000, run: snapshotTreasury },
  "constitution-versions": { every: 5 * 60_000, run: fillConstitutionVersions },
  "proposal-summaries": { every: 10 * 60_000, run: summarizeProposals },
  "ipfs-pin-check": { every: 30 * 60_000, run: checkPins },
  "votes-closing": { every: 15 * 60_000, run: queueClosingVotes },
  "social-metrics": { every: 60 * 60_000, run: pullSocialMetrics },
  "hos-sync": { every: 10 * 60_000, run: syncHosProposals },
  "hos-relay": { every: 10 * 60_000, run: relayHosVotes },
  "hos-position": { every: 60 * 60_000, run: snapshotHosPosition },
};

const queue = new Queue("jobs", { connection });
for (const [name, { every }] of Object.entries(tasks)) {
  await queue.upsertJobScheduler(name, { every }, { name, opts: { removeOnComplete: 100, removeOnFail: 500 } });
}

new Worker(
  "jobs",
  async (job: Job) => {
    const task = tasks[job.name];
    if (!task) throw new Error(`unknown job ${job.name}`);
    return task.run(deps);
  },
  { connection, concurrency: 2 },
).on("failed", (job, err) => console.error(`job ${job?.name} failed:`, err));

console.log(`jobs worker running: ${Object.keys(tasks).join(", ")}`);
