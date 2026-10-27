import type { Deps } from "./deps.js";

/**
 * Pull public social metrics (X followers / post count) into a short-lived cache that
 * missions can cite as a metric. A cache, not a store of record.
 */
export async function pullSocialMetrics({ redis }: Deps) {
  const token = process.env.X_BEARER_TOKEN;
  const handle = process.env.X_HANDLE;
  if (!token || !handle) return null;
  const res = await fetch(`https://api.x.com/2/users/by/username/${encodeURIComponent(handle)}?user.fields=public_metrics`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!res.ok) return null;
  const body = (await res.json()) as { data?: { public_metrics?: Record<string, number> } };
  const metrics = { ...body.data?.public_metrics, taken_at: new Date().toISOString() };
  await redis.set("metrics:social:x", JSON.stringify(metrics), "EX", 2 * 3600);
  return metrics;
}
