"use client";

import Link from "next/link";
import { api, useQuery } from "@/lib/api";
import { API_URL } from "@/lib/config";
import { formatAmount, pct, short, uriToHttp, when } from "@/lib/format";
import { Badge, Card, CardTitle, Progress, Stat, State } from "@/components/ui";

interface Status {
  chainHeight: number;
  indexedHeight: number | null;
  indexerLagSeconds: number | null;
  lastReceiptAt: string | null;
  workers: { account: string; healthy: boolean; lastHeartbeat: string | null }[];
  alerts: { noReceiptFor2Loops: boolean; indexerLagOver60s: boolean };
}

export default function Home() {
  const status = useQuery<Status>(() => fetch(`${API_URL}/status`).then((r) => r.json()), [], 15_000);
  const treasury = useQuery(() => api.treasury.latest.query(), []);
  const missions = useQuery(() => api.missions.list.query(), []);
  const receipts = useQuery(() => api.receipts.list.query({ limit: 8 }), [], 30_000);
  const constitution = useQuery(() => api.constitution.current.query(), []);

  const usd = treasury.data?.reduce((s, r) => s + Number(r.usdValue ?? 0), 0) ?? 0;
  const healthy = status.data?.workers.filter((w) => w.healthy).length ?? 0;
  const active = missions.data?.filter((m) => m.status === "Active") ?? [];

  return (
    <div className="space-y-8">
      <section>
        <h1 className="text-3xl font-semibold tracking-tight">An agent no one can steer but its holders.</h1>
        <p className="mt-2 max-w-2xl text-muted-foreground">
          CLAW holders vote on a tiered constitution. The IronClaw agent reads it every loop, and contracts cap anything it
          can spend. No human key can change it.
        </p>
      </section>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Stat label="Treasury" value={`$${usd.toLocaleString(undefined, { maximumFractionDigits: 0 })}`} hint="hourly snapshot" />
        <Stat
          label="Agent workers"
          value={`${healthy} / ${status.data?.workers.length ?? 0}`}
          hint={status.data?.alerts.noReceiptFor2Loops ? "⚠ no receipt for 2 loops" : "healthy = heartbeat within 2 loops"}
        />
        <Stat
          label="Constitution"
          value={constitution.data ? `v${constitution.data.version}` : "…"}
          hint={constitution.data ? <span className="font-mono">{short(constitution.data.hash, 10)}</span> : null}
        />
        <Stat
          label="Indexer"
          value={status.data?.indexedHeight?.toLocaleString() ?? "…"}
          hint={status.data?.alerts.indexerLagOver60s ? "⚠ lagging over 60s" : `chain ${status.data?.chainHeight?.toLocaleString() ?? "…"}`}
        />
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardTitle>Active missions</CardTitle>
          <State loading={missions.loading} error={missions.error} empty={active.length ? undefined : "No active missions."} />
          <ul className="space-y-4">
            {active.map((m) => (
              <li key={m.id}>
                <div className="flex items-baseline justify-between gap-3">
                  <Link href={`/missions?id=${encodeURIComponent(m.id)}`} className="font-medium hover:underline">
                    {m.title}
                  </Link>
                  <span className="text-xs text-muted-foreground">ends {when(m.deadline)}</span>
                </div>
                <div className="mt-2 flex items-center gap-3">
                  <Progress value={pct(m.spent, m.budget)} />
                  <span className="whitespace-nowrap text-xs tabular-nums text-muted-foreground">
                    {formatAmount(m.spent)} / {formatAmount(m.budget)} {m.asset}
                  </span>
                </div>
              </li>
            ))}
          </ul>
        </Card>

        <Card>
          <CardTitle>Latest receipts</CardTitle>
          <State loading={receipts.loading} error={receipts.error} empty={receipts.data?.length ? undefined : "No agent actions yet."} />
          <ul className="divide-y divide-border text-sm">
            {receipts.data?.map((r) => (
              <li key={r.id} className="flex items-center justify-between gap-3 py-2">
                <div>
                  <div className="font-medium">{r.missionId}</div>
                  <div className="font-mono text-xs text-muted-foreground">hash {short(r.constitutionHash, 6)}</div>
                </div>
                <div className="text-right">
                  <div className="tabular-nums">{formatAmount(r.amount)}</div>
                  <a className="text-xs text-primary hover:underline" href={uriToHttp(r.reasoningUri)} target="_blank" rel="noreferrer">
                    reasoning
                  </a>
                </div>
              </li>
            ))}
          </ul>
        </Card>
      </div>

      {status.data && status.data.workers.length > 0 && (
        <Card>
          <CardTitle>Workers</CardTitle>
          <ul className="grid gap-2 sm:grid-cols-3 text-sm">
            {status.data.workers.map((w) => (
              <li key={w.account} className="flex items-center justify-between rounded-md border border-border px-3 py-2">
                <span className="font-mono">{short(w.account, 6)}</span>
                <Badge tone={w.healthy ? "Active" : "Failed"}>{w.healthy ? "up" : "down"}</Badge>
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}
