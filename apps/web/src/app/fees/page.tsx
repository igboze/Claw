"use client";

import Link from "next/link";
import { api, useQuery } from "@/lib/api";
import { formatAmount, short, when } from "@/lib/format";
import { Badge, Button, Card, CardTitle, Progress, Stat, State } from "@/components/ui";

const MODE_LABEL: Record<string, string> = {
  StakeHoS: "Stake in House of Stake",
  HolderRewards: "Rewards to locked-CLAW holders",
  Buyback: "Buy back and burn CLAW",
};

export default function FeesPage() {
  const state = useQuery(() => api.fees.state.query(), [], 60_000);
  const feed = useQuery(() => api.fees.events.query({ limit: 30 }), [], 60_000);
  const s = state.data;
  const threshold = s ? Number(s.mcap_threshold_usd) : 100_000;
  const devPct = s ? s.dev_bps / 100 : 20;

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold">Trading fees</h1>
      <p className="max-w-3xl text-sm text-muted-foreground">
        Every CLAW buy or sell on a listed DEX pays a 2% fee. The fee router sells it for NEAR and splits it: {100 - devPct}%
        by the mode holders chose, {devPct}% to the dev wallet. Until CLAW holds a ${threshold.toLocaleString()} market cap for
        3 days in a row, the mode is fixed to staking in House of Stake.
      </p>
      <State loading={state.loading} error={state.error} />

      {s && (
        <>
          <div className="grid gap-4 sm:grid-cols-4">
            <Stat label="Current mode" value={MODE_LABEL[s.mode]?.split(" ").slice(0, 3).join(" ") ?? s.mode} hint={MODE_LABEL[s.mode]} />
            <Stat label="NEAR from fees" value={formatAmount(s.totals.near_from_fees)} hint="all time" />
            <Stat label="Locked in HoS" value={formatAmount(s.totals.to_hos)} hint="from fees" />
            <Stat label="CLAW burned" value={formatAmount(s.totals.claw_burned, 24, 0)} hint="by buybacks" />
          </div>

          <div className="grid gap-6 md:grid-cols-2">
            <Card>
              <CardTitle>Fee-mode vote</CardTitle>
              {s.mode_unlocked ? (
                <>
                  <p className="text-sm">
                    <Badge tone="Active">Unlocked</Badge> Holders can vote to send fees to House of Stake staking, holder
                    rewards, or buyback and burn.
                  </p>
                  <Link href="/proposals?new=1&kind=SetFeeMode">
                    <Button className="mt-3">Propose a fee mode</Button>
                  </Link>
                </>
              ) : (
                <>
                  <p className="text-sm text-muted-foreground">
                    Last market cap check: ${Number(s.last_mcap_usd).toLocaleString()} of ${threshold.toLocaleString()} needed.
                  </p>
                  <div className="mt-3">
                    <Progress value={(s.streak_days / 3) * 100} />
                  </div>
                  <p className="mt-2 text-xs text-muted-foreground">{s.streak_days} of 3 consecutive days at or above the threshold.</p>
                </>
              )}
            </Card>

            <Card>
              <CardTitle>Where fees went</CardTitle>
              <dl className="grid grid-cols-2 gap-1 text-sm tabular-nums">
                <dt className="text-muted-foreground">House of Stake</dt>
                <dd>{formatAmount(s.totals.to_hos)} NEAR</dd>
                <dt className="text-muted-foreground">Holder rewards</dt>
                <dd>{formatAmount(s.totals.to_holders)} NEAR</dd>
                <dt className="text-muted-foreground">Buyback</dt>
                <dd>{formatAmount(s.totals.to_buyback)} NEAR</dd>
                <dt className="text-muted-foreground">Dev wallet ({devPct}%)</dt>
                <dd>{formatAmount(s.totals.to_dev)} NEAR</dd>
                <dt className="text-muted-foreground">Waiting to route</dt>
                <dd>{formatAmount(s.pending_near)} NEAR</dd>
              </dl>
              <p className="mt-3 text-xs text-muted-foreground">
                Dev wallet <span className="font-mono">{short(s.dev_wallet, 10)}</span>. Only a Tier 1 vote can change it, and its
                share can never go above 20%.
              </p>
            </Card>
          </div>
        </>
      )}

      <Card>
        <CardTitle>Recent activity</CardTitle>
        <State loading={feed.loading} error={feed.error} empty={feed.data?.length === 0 ? "No fee activity yet." : undefined} />
        <ul className="divide-y divide-border text-sm">
          {feed.data?.map((e) => (
            <li key={e.id} className="flex flex-wrap justify-between gap-2 py-2">
              <span className="font-medium">{e.kind.replace(/_/g, " ")}</span>
              <span className="font-mono text-xs text-muted-foreground">{JSON.stringify(e.payload).slice(0, 120)}</span>
              <span className="text-xs text-muted-foreground">{when(e.blockTs)}</span>
            </li>
          ))}
        </ul>
      </Card>
    </div>
  );
}
