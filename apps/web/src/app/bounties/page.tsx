"use client";

import { useState, type FormEvent } from "react";
import { api, useQuery } from "@/lib/api";
import { BOUNTY_URL, EXPLORER } from "@/lib/config";
import { formatAmount, short, uriToHttp, when } from "@/lib/format";
import { useWallet } from "@/lib/wallet";
import { Badge, Button, Card, CardTitle, Input, State } from "@/components/ui";

function Submissions({ missionId }: { missionId: string }) {
  const q = useQuery(() => api.bounties.query({ missionId }), [missionId], 30_000);
  return (
    <div className="mt-4">
      <State loading={q.loading} error={q.error} empty={q.data?.length === 0 ? "No submissions yet." : undefined} />
      <ul className="divide-y divide-border text-sm">
        {q.data?.map((s) => (
          <li key={s.id} className="flex flex-wrap items-center gap-3 py-2">
            <span className="font-mono">{short(s.account, 10)}</span>
            <a className="text-primary hover:underline" href={uriToHttp(s.contentUri)} target="_blank" rel="noreferrer">
              work
            </a>
            <span className="text-xs text-muted-foreground">{when(s.createdAt)}</span>
            <span className="ml-auto">
              {s.paidTx ? (
                <a className="text-xs text-primary hover:underline" href={`${EXPLORER}/txns/${s.paidTx}`}>
                  paid
                </a>
              ) : (
                <Badge>{s.verdict ?? "pending"}</Badge>
              )}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export default function BountiesPage() {
  const { accountId } = useWallet();
  const missions = useQuery(() => api.missions.list.query(), []);
  const [open, setOpen] = useState<string | null>(null);
  const [msg, setMsg] = useState<string>();
  const active = missions.data?.filter((m) => m.status === "Active") ?? [];

  async function submit(e: FormEvent<HTMLFormElement>, missionId: string) {
    e.preventDefault();
    setMsg(undefined);
    const form = new FormData(e.currentTarget);
    form.set("mission_id", missionId);
    form.set("account", accountId ?? "");
    const file = form.get("file");
    if (file instanceof File && file.size === 0) form.delete("file");
    const res = await fetch(`${BOUNTY_URL}/submissions`, { method: "POST", body: form });
    setMsg(res.ok ? "Submitted. The agent reviews submissions in its next loop." : `Failed: ${(await res.json()).error ?? res.status}`);
  }

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold">Bounties</h1>
      <p className="max-w-2xl text-sm text-muted-foreground">
        Submit work for an active mission. The agent judges submissions and pays through the agent contract, within the
        mission's caps. Verdicts and payouts come from on-chain receipts.
      </p>
      <State loading={missions.loading} error={missions.error} empty={missions.data && !active.length ? "No open bounties." : undefined} />
      {active.map((m) => (
        <Card key={m.id}>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <CardTitle className="mb-1">{m.title}</CardTitle>
              <p className="text-sm">{m.goal}</p>
              <p className="mt-1 text-xs text-muted-foreground">
                Up to {formatAmount(m.maxPerAction)} {m.asset} per payout · {formatAmount(m.spent)} of {formatAmount(m.budget)} spent · ends {when(m.deadline)}
              </p>
            </div>
            <Button variant="outline" onClick={() => setOpen(open === m.id ? null : m.id)}>
              {open === m.id ? "Close" : "Submit / view"}
            </Button>
          </div>
          {open === m.id && (
            <>
              {accountId ? (
                <form className="mt-4 grid gap-3 sm:grid-cols-2" onSubmit={(e) => submit(e, m.id)}>
                  <Input name="url" placeholder="Link to your work (PR, post, …)" />
                  <Input name="file" type="file" />
                  <Input name="note" placeholder="Short note" className="sm:col-span-2" maxLength={2000} />
                  <Button type="submit" className="sm:col-span-2">
                    Submit as {short(accountId, 10)}
                  </Button>
                </form>
              ) : (
                <p className="mt-4 text-sm text-muted-foreground">Connect a wallet to submit.</p>
              )}
              {msg && <p className="mt-2 text-sm">{msg}</p>}
              <Submissions missionId={m.id} />
            </>
          )}
        </Card>
      ))}
    </div>
  );
}
