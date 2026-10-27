"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { Suspense, useState } from "react";
import type { Choice } from "@clawdao/types";
import { api, useQuery } from "@/lib/api";
import { CONTRACTS, TIER_NAMES } from "@/lib/config";
import { countdown, formatAmount, short, uriToHttp, when } from "@/lib/format";
import { useWallet } from "@/lib/wallet";
import { Badge, Button, Card, CardTitle, State } from "@/components/ui";
import { ProposalForm } from "./form";

function Bar({ f, a, x }: { f: string; a: string; x: string }) {
  const total = Number(f) + Number(a) + Number(x) || 1;
  const w = (v: string) => `${(Number(v) / total) * 100}%`;
  return (
    <div className="flex h-2 w-full overflow-hidden rounded-full bg-muted">
      <div className="bg-success" style={{ width: w(f) }} />
      <div className="bg-danger" style={{ width: w(a) }} />
      <div className="bg-muted-foreground/40" style={{ width: w(x) }} />
    </div>
  );
}

function Detail({ id }: { id: number }) {
  const p = useQuery(() => api.proposals.get.query({ id }), [id], 20_000);
  const { accountId, call } = useWallet();
  const [msg, setMsg] = useState<string>();

  if (!p.data) return <State loading={p.loading} error={p.error} empty="Proposal not found." />;
  const d = p.data;
  const run = async (method: string, args: Record<string, unknown>, gasTgas = 100) => {
    setMsg(undefined);
    try {
      await call({ receiverId: CONTRACTS.governor, method, args, gasTgas });
      setMsg("Sent. The page updates once the indexer sees it.");
      p.reload();
    } catch (e) {
      setMsg((e as Error).message);
    }
  };
  const ended = new Date(d.endsAt).getTime() <= Date.now();
  const etaPassed = d.eta && new Date(d.eta).getTime() <= Date.now();
  const myVote = d.votes.find((v) => v.voter === accountId);

  return (
    <div className="space-y-6">
      <Link href="/proposals" className="text-sm text-muted-foreground hover:underline">
        ← All proposals
      </Link>
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="text-2xl font-semibold">
          #{d.id} {d.title}
        </h1>
        <Badge>{d.status}</Badge>
        <span className="text-sm text-muted-foreground">{TIER_NAMES[d.tier]}</span>
      </div>

      {d.aiSummary && (
        <Card className="border-dashed">
          <CardTitle>AI summary · not authoritative</CardTitle>
          <p className="text-sm leading-relaxed">{d.aiSummary}</p>
          <p className="mt-2 text-xs text-muted-foreground">Generated off-chain. What executes is the on-chain action below.</p>
        </Card>
      )}

      <div className="grid gap-4 md:grid-cols-3">
        <Card className="md:col-span-2">
          <CardTitle>On-chain action · {d.kind}</CardTitle>
          <pre className="max-h-80 overflow-auto rounded-md bg-muted p-3 text-xs">{JSON.stringify(d.kindPayload, null, 2)}</pre>
          <a className="mt-3 inline-block text-sm text-primary hover:underline" href={uriToHttp(d.bodyUri)} target="_blank" rel="noreferrer">
            Full proposal text and forum discussion →
          </a>
        </Card>
        <Card>
          <CardTitle>Vote</CardTitle>
          <Bar f={d.votesFor} a={d.votesAgainst} x={d.votesAbstain} />
          <dl className="mt-3 grid grid-cols-2 gap-1 text-sm tabular-nums">
            <dt className="text-success">For</dt>
            <dd>{formatAmount(d.votesFor, 24, 0)}</dd>
            <dt className="text-danger">Against</dt>
            <dd>{formatAmount(d.votesAgainst, 24, 0)}</dd>
            <dt className="text-muted-foreground">Abstain</dt>
            <dd>{formatAmount(d.votesAbstain, 24, 0)}</dd>
          </dl>
          <p className="mt-3 text-xs text-muted-foreground">
            Voting {ended ? "ended" : `ends in ${countdown(d.endsAt)}`} ({when(d.endsAt)}). Power is your ve lock at {when(d.snapshotTs)}.
          </p>
          {d.status === "Active" && !ended && (
            <div className="mt-3 flex flex-wrap gap-2">
              {myVote ? (
                <p className="text-sm">You voted {myVote.choice}.</p>
              ) : (
                (["For", "Against", "Abstain"] as Choice[]).map((c) => (
                  <Button key={c} variant={c === "For" ? "default" : "outline"} disabled={!accountId} onClick={() => run("vote", { proposal_id: d.id, choice: c })}>
                    {c}
                  </Button>
                ))
              )}
            </div>
          )}
          {d.status === "Active" && ended && (
            <Button className="mt-3" onClick={() => run("finalize", { proposal_id: d.id })} disabled={!accountId}>
              Finalize
            </Button>
          )}
          {d.status === "Queued" && (
            <div className="mt-3 space-y-2">
              <p className="text-sm">Timelock: {etaPassed ? "over" : `${countdown(d.eta)} left`}</p>
              <Button onClick={() => run("execute", { proposal_id: d.id }, 300)} disabled={!accountId || !etaPassed}>
                Execute
              </Button>
            </div>
          )}
          {!accountId && <p className="mt-2 text-xs text-muted-foreground">Connect a wallet to vote.</p>}
          {msg && <p className="mt-2 text-xs">{msg}</p>}
        </Card>
      </div>

      <Card>
        <CardTitle>Voters</CardTitle>
        <State empty={d.votes.length ? undefined : "No votes yet."} />
        <ul className="divide-y divide-border text-sm">
          {d.votes.map((v) => (
            <li key={v.voter} className="flex justify-between py-1.5">
              <span className="font-mono">{short(v.voter, 12)}</span>
              <span>
                {v.choice} · <span className="tabular-nums">{formatAmount(v.power, 24, 0)}</span>
              </span>
            </li>
          ))}
        </ul>
      </Card>
    </div>
  );
}

function List() {
  const [status, setStatus] = useState<string>("");
  const list = useQuery(() => api.proposals.list.query({ limit: 50, status: status || undefined }), [status]);
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-2xl font-semibold">Proposals</h1>
        <Link href="/proposals?new=1">
          <Button>New proposal</Button>
        </Link>
      </div>
      <div className="flex gap-2 text-sm">
        {["", "Active", "Queued", "Executed", "Defeated"].map((s) => (
          <Button key={s} variant={status === s ? "default" : "outline"} onClick={() => setStatus(s)}>
            {s || "All"}
          </Button>
        ))}
      </div>
      <State loading={list.loading} error={list.error} empty={list.data?.length === 0 ? "No proposals." : undefined} />
      <ul className="space-y-3">
        {list.data?.map((p) => (
          <li key={p.id}>
            <Link href={`/proposals?id=${p.id}`}>
              <Card className="flex flex-wrap items-center gap-4 transition-colors hover:border-primary/50">
                <span className="w-10 text-muted-foreground">#{p.id}</span>
                <div className="min-w-0 flex-1">
                  <div className="truncate font-medium">{p.title}</div>
                  <div className="text-xs text-muted-foreground">
                    {TIER_NAMES[p.tier]} · {p.kind}
                  </div>
                </div>
                <div className="w-40">
                  <Bar f={p.votesFor} a={p.votesAgainst} x={p.votesAbstain} />
                </div>
                <Badge>{p.status}</Badge>
                <span className="w-24 text-right text-xs text-muted-foreground">
                  {p.status === "Active" ? countdown(p.endsAt) : p.status === "Queued" ? `eta ${countdown(p.eta)}` : ""}
                </span>
              </Card>
            </Link>
          </li>
        ))}
      </ul>
    </div>
  );
}

function Page() {
  const q = useSearchParams();
  if (q.get("new")) return <ProposalForm />;
  const id = q.get("id");
  return id ? <Detail id={Number(id)} /> : <List />;
}

export default function ProposalsPage() {
  return (
    <Suspense>
      <Page />
    </Suspense>
  );
}

