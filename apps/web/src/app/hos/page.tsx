"use client";

import Link from "next/link";
import { useState } from "react";
import { NearRpc } from "@clawdao/near";
import { api, useQuery } from "@/lib/api";
import { CONTRACTS, NETWORK } from "@/lib/config";
import { countdown, formatAmount, short, when } from "@/lib/format";
import { useWallet } from "@/lib/wallet";
import { Badge, Button, Card, CardTitle, Stat, State } from "@/components/ui";

const rpc = new NearRpc(`https://rpc.${NETWORK}.near.org`);
const HOS_WEB = "https://gov.houseofstake.org";

export default function HouseOfStakePage() {
  const position = useQuery(() => api.hos.position.query(), [], 60_000);
  const list = useQuery(() => api.hos.proposals.query({ limit: 30 }), [], 60_000);
  const { accountId, call } = useWallet();
  const [msg, setMsg] = useState<string>();

  // Anyone with CLAW can open the clawdao vote that decides how the treasury votes.
  const startVote = async (id: number, title: string | null) => {
    setMsg(undefined);
    try {
      const cfg = await rpc.view<{ proposal_bond: string }>(CONTRACTS.governor, "get_config");
      await call({
        receiverId: CONTRACTS.governor,
        method: "propose",
        args: {
          tier: 3,
          kind: { HosVote: { hos_proposal_id: id } },
          title: `House of Stake #${id}: ${title ?? "untitled"}`.slice(0, 200),
          body_uri: `${HOS_WEB}/proposals/${id}`,
        },
        deposit: cfg.proposal_bond,
        gasTgas: 50,
      });
      setMsg("clawdao vote created. It appears under Proposals once indexed.");
      list.reload();
    } catch (e) {
      setMsg((e as Error).message);
    }
  };

  const p = position.data;
  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold">House of Stake</h1>
      <p className="max-w-3xl text-sm text-muted-foreground">
        The treasury's NEAR is locked in a veNEAR lockup owned by the agent contract. CLAW holders vote on each House of
        Stake proposal; the winning option becomes a mandate on-chain, and a relayer casts it on vote.dao with a proof from
        the House of Stake snapshot. Nobody can cast a vote holders did not choose.
      </p>
      <State loading={position.loading} error={position.error} />
      {p && !p.config && <p className="text-sm">House of Stake is not configured yet (needs a Tier 1 proposal).</p>}
      {p?.config && (
        <div className="grid gap-4 sm:grid-cols-4">
          <Stat label="Treasury veNEAR" value={formatAmount(p.venear)} hint="voting power on House of Stake" />
          <Stat label="Locked NEAR" value={formatAmount(p.locked)} hint={`cap ${formatAmount(p.config.max_locked, 24, 0)}`} />
          <Stat label="Lockup balance" value={p.lockupBalance ? formatAmount(p.lockupBalance) : "—"} hint={p.lockup_account_id ? short(p.lockup_account_id, 8) : undefined} />
          <Stat label="Staking pool" value={p.stakingPool ? short(p.stakingPool, 10) : "none"} hint="rewards stay in the lockup" />
        </div>
      )}

      <Card>
        <CardTitle>Proposals</CardTitle>
        <State loading={list.loading} error={list.error} empty={list.data?.length === 0 ? "No House of Stake proposals synced yet." : undefined} />
        <ul className="divide-y divide-border">
          {list.data?.map((h) => {
            const open = h.clawdaoVotes.find((c) => c.status === "Active");
            return (
              <li key={h.id} className="flex flex-wrap items-center gap-3 py-3 text-sm">
                <span className="w-10 text-muted-foreground">#{h.id}</span>
                <div className="min-w-0 flex-1">
                  <a className="font-medium hover:underline" href={h.link ?? `${HOS_WEB}/proposals/${h.id}`} target="_blank" rel="noreferrer">
                    {h.title ?? "Untitled"}
                  </a>
                  <div className="text-xs text-muted-foreground">
                    {h.status === "Voting" ? `HoS voting ends in ${countdown(h.votingEnds)}` : `HoS ${h.status}`} · snapshot block {h.snapshotBlock ?? "—"}
                  </div>
                </div>
                <div className="text-right text-xs">
                  {h.mandate ? (
                    <>
                      <div>
                        CLAW decided <Badge tone={h.mandate.vote === "For" ? "Active" : h.mandate.vote === "Against" ? "Failed" : undefined}>{h.mandate.vote}</Badge>
                      </div>
                      <div className="text-muted-foreground">{h.mandate.castVote === h.mandate.vote ? "cast on vote.dao" : "waiting for relay"}</div>
                    </>
                  ) : open ? (
                    <Link className="text-primary hover:underline" href={`/proposals?id=${open.id}`}>
                      CLAW vote open · ends {when(open.endsAt)}
                    </Link>
                  ) : h.status === "Voting" ? (
                    <Button variant="outline" disabled={!accountId} onClick={() => startVote(h.id, h.title)}>
                      Start CLAW vote
                    </Button>
                  ) : (
                    <span className="text-muted-foreground">no CLAW vote</span>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
        {msg && <p className="mt-2 text-sm">{msg}</p>}
      </Card>
    </div>
  );
}
