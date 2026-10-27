"use client";

import Link from "next/link";
import { useState } from "react";
import { NearRpc } from "@clawdao/near";
import type { ProposalKind, Tier } from "@clawdao/types";
import { CONTRACTS, NETWORK, TIER_NAMES } from "@/lib/config";
import { toRaw } from "@/lib/format";
import { useWallet } from "@/lib/wallet";
import { Button, Card, CardTitle, Input } from "@/components/ui";

const rpc = new NearRpc(`https://rpc.${NETWORK}.near.org`);
type KindName = "SetMission" | "CancelMission" | "Amend" | "SetParams" | "FunctionCall";
const KINDS_BY_TIER: Record<1 | 2 | 3, KindName[]> = {
  1: ["Amend", "FunctionCall"],
  2: ["Amend", "SetParams"],
  3: ["SetMission", "CancelMission"],
};

const field = "space-y-1 text-sm";

export function ProposalForm() {
  const { accountId, call } = useWallet();
  const [tier, setTier] = useState<1 | 2 | 3>(3);
  const [kind, setKind] = useState<KindName>("SetMission");
  const [title, setTitle] = useState("");
  const [bodyUri, setBodyUri] = useState("");
  const [m, setM] = useState({ id: "", title: "", goal: "", asset: "near", budget: "", max: "", deadline: "", metric: "", chainSig: false });
  const [raw, setRaw] = useState("{}");
  const [msg, setMsg] = useState<string>();

  const buildKind = (): ProposalKind => {
    switch (kind) {
      case "SetMission":
        return {
          SetMission: {
            mission_json: JSON.stringify({
              id: m.id,
              title: m.title,
              goal: m.goal,
              asset: m.asset,
              budget: toRaw(m.budget),
              max_per_action: toRaw(m.max),
              deadline: (BigInt(new Date(m.deadline).getTime()) * 1_000_000n).toString(),
              metric: m.metric,
              allow_chain_sig: m.chainSig,
            }),
          },
        };
      case "CancelMission":
        return { CancelMission: { mission_id: m.id } };
      case "Amend":
        return { Amend: { article_json: JSON.stringify(JSON.parse(raw)) } };
      case "SetParams":
        return { SetParams: { params_json: JSON.stringify(JSON.parse(raw)) } };
      case "FunctionCall":
        return { FunctionCall: JSON.parse(raw) };
    }
  };

  const submit = async () => {
    setMsg(undefined);
    try {
      const cfg = await rpc.view<{ proposal_bond: string }>(CONTRACTS.governor, "get_config");
      await call({
        receiverId: CONTRACTS.governor,
        method: "propose",
        args: { tier: tier as Tier, kind: buildKind(), title, body_uri: bodyUri },
        deposit: cfg.proposal_bond,
        gasTgas: 50,
      });
      setMsg("Proposal sent. It appears in the list once indexed.");
    } catch (e) {
      setMsg((e as Error).message);
    }
  };

  const mi = (k: keyof typeof m, label: string, type = "text") => (
    <label className={field}>
      <span className="text-muted-foreground">{label}</span>
      <Input type={type} value={String(m[k])} onChange={(e) => setM({ ...m, [k]: e.target.value })} />
    </label>
  );

  return (
    <div className="space-y-6">
      <Link href="/proposals" className="text-sm text-muted-foreground hover:underline">
        ← All proposals
      </Link>
      <h1 className="text-2xl font-semibold">New proposal</h1>
      <Card className="space-y-4">
        <div className="grid gap-4 sm:grid-cols-2">
          <label className={field}>
            <span className="text-muted-foreground">Tier</span>
            <select
              className="h-9 w-full rounded-md border border-border bg-background px-2"
              value={tier}
              onChange={(e) => {
                const t = Number(e.target.value) as 1 | 2 | 3;
                setTier(t);
                setKind(KINDS_BY_TIER[t][0]!);
              }}
            >
              {[1, 2, 3].map((t) => (
                <option key={t} value={t}>
                  {TIER_NAMES[t]}
                </option>
              ))}
            </select>
          </label>
          <label className={field}>
            <span className="text-muted-foreground">Action</span>
            <select className="h-9 w-full rounded-md border border-border bg-background px-2" value={kind} onChange={(e) => setKind(e.target.value as KindName)}>
              {KINDS_BY_TIER[tier].map((k) => (
                <option key={k}>{k}</option>
              ))}
            </select>
          </label>
        </div>
        <label className={field}>
          <span className="text-muted-foreground">Title</span>
          <Input value={title} maxLength={200} onChange={(e) => setTitle(e.target.value)} />
        </label>
        <label className={field}>
          <span className="text-muted-foreground">Full text URI (ipfs://… with forum link)</span>
          <Input value={bodyUri} onChange={(e) => setBodyUri(e.target.value)} placeholder="ipfs://" />
        </label>

        {kind === "SetMission" && (
          <div className="grid gap-4 sm:grid-cols-2">
            {mi("id", "Mission id")}
            {mi("title", "Mission title")}
            {mi("goal", "Goal")}
            {mi("metric", "Metric")}
            {mi("asset", "Asset (near or token account)")}
            {mi("budget", "Budget (whole units)")}
            {mi("max", "Max per action (whole units)")}
            {mi("deadline", "Deadline", "datetime-local")}
            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={m.chainSig} onChange={(e) => setM({ ...m, chainSig: e.target.checked })} />
              Allow Chain Signatures (other chains)
            </label>
          </div>
        )}
        {kind === "CancelMission" && mi("id", "Mission id")}
        {(kind === "Amend" || kind === "SetParams" || kind === "FunctionCall") && (
          <label className={field}>
            <span className="text-muted-foreground">
              {kind === "Amend" && 'Article JSON: {"id","title","body"}'}
              {kind === "SetParams" && 'Params JSON: {"loop_interval_minutes", "daily_caps": [["near","…"]]}'}
              {kind === "FunctionCall" && 'FunctionCall JSON: {"receiver_id", "actions": [{"method_name","args"(base64),"deposit","gas"}]}'}
            </span>
            <textarea className="h-40 w-full rounded-md border border-border bg-background p-2 font-mono text-xs" value={raw} onChange={(e) => setRaw(e.target.value)} />
          </label>
        )}

        <CardTitle className="mb-0 mt-2">Before you submit</CardTitle>
        <p className="text-xs text-muted-foreground">
          Attach the proposal bond (refunded on finalize). Tier 1 needs a 66.67% supermajority and a timelock of at least 7 days.
        </p>
        <Button disabled={!accountId || !title} onClick={submit}>
          {accountId ? "Submit proposal" : "Connect a wallet first"}
        </Button>
        {msg && <p className="text-sm">{msg}</p>}
      </Card>
    </div>
  );
}
