"use client";

import Link from "next/link";
import { api, useQuery } from "@/lib/api";
import { short, when } from "@/lib/format";
import { Badge, Button, Card, CardTitle, Stat, State } from "@/components/ui";

const MARKET = "https://market.near.ai";
const usd = (cents: number | string) => `$${(Number(cents) / 100).toLocaleString(undefined, { minimumFractionDigits: 2 })}`;

interface Work {
  tags: string[];
  min_price_cents: number;
  max_open: number;
  currencies: string[];
}

export default function MarketPage() {
  const missions = useQuery(() => api.market.missions.query(), [], 60_000);
  const earnings = useQuery(() => api.market.earnings.query({ limit: 50 }), [], 60_000);
  const totals = useQuery(() => api.market.totals.query(), [], 60_000);
  const totalCents = totals.data?.reduce((s, t) => s + Number(t.cents), 0) ?? 0;
  const totalJobs = totals.data?.reduce((s, t) => s + Number(t.jobs), 0) ?? 0;
  const active = missions.data?.filter((m) => m.status === "Active") ?? [];

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-2xl font-semibold">Agent Market</h1>
        <Link href="/proposals?new=1&kind=SetMission">
          <Button>Propose a work mission</Button>
        </Link>
      </div>
      <p className="max-w-3xl text-sm text-muted-foreground">
        The agent sells work on the{" "}
        <a className="text-primary hover:underline" href={MARKET} target="_blank" rel="noreferrer">
          NEAR AI Agent Market
        </a>
        , but only jobs an active work mission allows: matching tags, an allowed currency, at or above the minimum price, within
        the limit of open jobs. Every job a buyer accepts is recorded on-chain. Earnings stay on the marketplace balance until
        holders vote to withdraw and allocate them.
      </p>

      <div className="grid gap-4 sm:grid-cols-3">
        <Stat label="Earned (all time)" value={usd(totalCents)} hint="before the marketplace's 5% fee" />
        <Stat label="Jobs accepted" value={totalJobs} />
        <Stat label="Active work missions" value={active.length} />
      </div>

      <Card>
        <CardTitle>Work missions</CardTitle>
        <State loading={missions.loading} error={missions.error} empty={missions.data?.length === 0 ? "No work missions yet." : undefined} />
        <ul className="divide-y divide-border">
          {missions.data?.map((m) => {
            const w = m.work as Work;
            const earned = m.earnings.reduce((s, e) => s + Number(e.cents), 0);
            return (
              <li key={m.id} className="flex flex-wrap items-center gap-3 py-3 text-sm">
                <div className="min-w-0 flex-1">
                  <Link href={`/missions?id=${encodeURIComponent(m.id)}`} className="font-medium hover:underline">
                    {m.title}
                  </Link>
                  <div className="mt-1 flex flex-wrap gap-1">
                    {w.tags.map((t) => (
                      <Badge key={t}>{t}</Badge>
                    ))}
                  </div>
                </div>
                <div className="text-xs text-muted-foreground">
                  min {usd(w.min_price_cents)} · up to {w.max_open} open · {w.currencies.join("/")} · ends {when(m.deadline)}
                </div>
                <div className="w-28 text-right tabular-nums">{usd(earned)}</div>
                <Badge>{m.status}</Badge>
              </li>
            );
          })}
        </ul>
      </Card>

      <Card>
        <CardTitle>Earnings (Tier 4, on-chain)</CardTitle>
        <State loading={earnings.loading} error={earnings.error} empty={earnings.data?.length === 0 ? "No accepted jobs yet." : undefined} />
        <table className="w-full text-left text-sm">
          <thead className="text-xs text-muted-foreground">
            <tr>
              <th className="py-2">#</th>
              <th>Mission</th>
              <th>Assignment</th>
              <th className="text-right">Amount</th>
              <th>Recorded</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border">
            {earnings.data?.map((e) => (
              <tr key={e.id}>
                <td className="py-2">{e.id}</td>
                <td>{e.missionId}</td>
                <td>
                  <a className="font-mono text-xs text-primary hover:underline" href={`${MARKET}/jobs/${e.jobId}`} target="_blank" rel="noreferrer">
                    {short(e.assignmentId, 6)}
                  </a>
                </td>
                <td className="text-right tabular-nums">
                  {e.amount} {e.currency}
                </td>
                <td className="text-xs text-muted-foreground">block {e.height}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="mt-3 text-xs text-muted-foreground">
          Earnings are reported by an attested worker and can be checked against the job on Agent Market. To use them, a Tier 2
          vote withdraws them to the treasury and an Allocate vote sends them to House of Stake, holder rewards, or buyback.
        </p>
      </Card>
    </div>
  );
}
