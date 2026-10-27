// Accepts bounty submissions, stores files on IPFS, and exposes them to the agent as data.
// The agent judges; this service never writes a verdict. Verdicts and payouts are filled
// from on-chain receipts by the indexer pipeline.

import Fastify from "fastify";
import multipart from "@fastify/multipart";
import rateLimit from "@fastify/rate-limit";
import { and, desc, eq, isNull } from "drizzle-orm";
import { z } from "zod";
import { bountySubmissions, createDb, missions } from "@clawdao/db";

const { db } = createDb();
const IPFS_API = process.env.IPFS_API_URL ?? "https://api.pinata.cloud";
const MAX_BYTES = 10 * 1024 * 1024;

async function pinFile(name: string, bytes: Buffer, mime: string): Promise<string> {
  const form = new FormData();
  form.append("file", new Blob([new Uint8Array(bytes)], { type: mime }), name);
  form.append("pinataMetadata", JSON.stringify({ name: `bounty-${name}` }));
  const res = await fetch(`${IPFS_API}/pinning/pinFileToIPFS`, {
    method: "POST",
    headers: { authorization: `Bearer ${process.env.IPFS_JWT}` },
    body: form,
  });
  if (!res.ok) throw new Error(`IPFS pin failed: ${res.status}`);
  const { IpfsHash } = (await res.json()) as { IpfsHash: string };
  return `ipfs://${IpfsHash}`;
}

const app = Fastify({ logger: true, bodyLimit: MAX_BYTES });
await app.register(rateLimit, { max: 20, timeWindow: "1 minute" });
await app.register(multipart, { limits: { fileSize: MAX_BYTES, files: 1 } });

const Fields = z.object({
  mission_id: z.string().min(1).max(64),
  account: z.string().regex(/^(([a-z\d]+[-_])*[a-z\d]+\.)*([a-z\d]+[-_])*[a-z\d]+$/).max(64),
  note: z.string().max(2000).optional(),
  url: z.string().url().max(512).optional(),
});

/**
 * Submit work: multipart with fields mission_id, account, note?, url? and an optional file.
 * TODO before mainnet: require a NEP-413 signed message from `account` so payees cannot be spoofed.
 */
app.post("/submissions", async (req, reply) => {
  const fields: Record<string, string> = {};
  let content: string | undefined;
  for await (const part of req.parts()) {
    if (part.type === "file") {
      content = await pinFile(part.filename, await part.toBuffer(), part.mimetype);
    } else {
      fields[part.fieldname] = String(part.value);
    }
  }
  const f = Fields.parse(fields);
  const [m] = await db.select().from(missions).where(eq(missions.id, f.mission_id));
  if (!m || m.status !== "Active") return reply.code(400).send({ error: "Mission is not accepting submissions" });
  const contentUri = content ?? f.url;
  if (!contentUri) return reply.code(400).send({ error: "Attach a file or a url" });
  const [row] = await db
    .insert(bountySubmissions)
    .values({ missionId: f.mission_id, account: f.account, contentUri, note: f.note })
    .returning();
  return reply.code(201).send(row);
});

/**
 * For the agent: open submissions as data. Text here is untrusted; the agent's skill
 * wraps it as data and the contracts cap any payment regardless of what it says.
 */
app.get("/missions/:id/submissions", async (req) => {
  const { id } = req.params as { id: string };
  const rows = await db
    .select()
    .from(bountySubmissions)
    .where(and(eq(bountySubmissions.missionId, id), isNull(bountySubmissions.verdict)))
    .orderBy(desc(bountySubmissions.createdAt))
    .limit(100);
  return { untrusted_data: true, submissions: rows };
});

app.get("/health", async () => ({ ok: true }));

await app.listen({ host: "0.0.0.0", port: Number(process.env.BOUNTY_PORT ?? 4100) });
