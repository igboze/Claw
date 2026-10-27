"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { Suspense } from "react";
import { api, useQuery } from "@/lib/api";
import { EXPLORER } from "@/lib/config";
import { formatAmount, pct, short, uriToHttp, when } from "@/lib/format";
import { Badge, Card, CardTitle, Progress, State } from "@/components/ui";

function MissionDetail({ id }: { id: string }) {
  const m = useQuery(() => api.missions.get.query({ id }), [id]);
  if (!m.data) return <State loading={m.loading} error={m.error} empty="Mission not found." />;
  const d = m.data;
  return (
    <div className="space-y-6">
      <Link href="/missions" className="text-sm text-muted-foreground hover:underline">
        ← All missions
      </Link>
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="text-2xl font-semibold">{d.title}</h1>
        <Badge>{d.status}</Badge>
      </div>
      <p className="max-w-3xl text-muted-foreground">{d.goal}</p>
      <div className="grid gap-4 sm:grid-cols-3">
        <Card>
          <CardTitle>Budget</CardTitle>
          <Progress value={pct(d.spent, d.budget)} />
          <p className="mt-2 text-sm tabular-nums">
            {formatAmount(d.spent)} of {formatAmount(d.budget)} {d.asset}
          </p>
          <p className="text-xs text-muted-foreground">max {formatAmount(d.maxPerAction)} per action</p>
        </Card>
        <Card>
          <CardTitle>Deadline</CardTitle>
          <p className="text-sm">{when(d.deadline)}</p>
          <p className="text-xs text-muted-foreground">Chain Signatures: {d.allowChainSig ? "allowed" : "not allowed"}</p>
        </Card>
        <Card>
          <CardTitle>Metric</CardTitle>
          <p className="text-sm">{d.metric}</p>
        </Card>
      </div>
      <Card>
        <CardTitle>Receipts</CardTitle>
        <State empty={d.receipts.length ? undefined : "No actions on this mission yet."} />
        <table className="w-full text-left text-sm">
          <thead className="text-xs text-muted-foreground">
            <tr>
              <th className="py-2">#</th>
              <th>Amount</th>
              <th>Constitution</th>
              <th>Action hash</th>
              <th>Tx</th>
              <th>Reasoning</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border">
            {d.receipts.map((r) => (
              <tr key={r.id}>
                <td className="py-2">{r.id}</td>
                <td className="tabular-nums">{formatAmount(r.amount)}</td>
                <td className="font-mono text-xs">{short(r.constitutionHash, 6)}</td>
                <td className="font-mono text-xs">{short(r.actionHash, 6)}</td>
                <td className="text-xs">
                  {r.foreignTxHash ? `${r.chain}: ${short(r.foreignTxHash, 6)}` : r.txHash ? <a className="text-primary hover:underline" href={`${EXPLORER}/txns/${r.txHash}`}>{short(r.txHash, 6)}</a> : `block ${r.height}`}
                </td>
                <td>
                  <a className="text-primary hover:underline" href={uriToHttp(r.reasoningUri)} target="_blank" rel="noreferrer">
                    log
                  </a>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </Card>
    </div>
  );
}

function MissionList() {
  const list = useQuery(() => api.missions.list.query(), []);
  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold">Missions</h1>
      <p className="max-w-2xl text-sm text-muted-foreground">
        Missions are Tier 3 of the constitution. Each has a budget, a per-action cap, a deadline and a metric. The agent
        contract rejects any spend outside them.
      </p>
      <State loading={list.loading} error={list.error} empty={list.data?.length === 0 ? "No missions yet." : undefined} />
      <div className="grid gap-4 md:grid-cols-2">
        {list.data?.map((m) => (
          <Link key={m.id} href={`/missions?id=${encodeURIComponent(m.id)}`}>
            <Card className="h-full transition-colors hover:border-primary/50">
              <div className="flex items-center justify-between gap-2">
                <h2 className="font-medium">{m.title}</h2>
                <span className="flex gap-1">
                  {m.work ? <Badge tone="Queued">Work</Badge> : null}
                  <Badge>{m.status}</Badge>
                </span>
              </div>
              <p className="mt-1 line-clamp-2 text-sm text-muted-foreground">{m.goal}</p>
              <div className="mt-3">
                <Progress value={pct(m.spent, m.budget)} />
              </div>
              <p className="mt-2 text-xs tabular-nums text-muted-foreground">
                {formatAmount(m.spent)} / {formatAmount(m.budget)} {m.asset} · ends {when(m.deadline)}
              </p>
            </Card>
          </Link>
        ))}
      </div>
    </div>
  );
}

function Page() {
  const id = useSearchParams().get("id");
  return id ? <MissionDetail id={id} /> : <MissionList />;
}

export default function MissionsPage() {
  return (
    <Suspense>
      <Page />
    </Suspense>
  );
}
