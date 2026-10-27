"use client";

import { useState } from "react";
import { NearRpc } from "@clawdao/near";
import { api, useQuery } from "@/lib/api";
import { CONTRACTS, NETWORK } from "@/lib/config";
import { formatAmount, toRaw, when } from "@/lib/format";
import { useWallet } from "@/lib/wallet";
import { Button, Card, CardTitle, Input, Stat, State } from "@/components/ui";

const rpc = new NearRpc(`https://rpc.${NETWORK}.near.org`);
const MONTH_MS = 30 * 86_400_000;
const MAX_MONTHS = 24;

/** Power = amount × remaining / 24 months (same formula as the ve contract). */
function powerAt(amount: number, unlockMs: number, atMs: number) {
  const remaining = Math.max(0, Math.min(unlockMs - atMs, MAX_MONTHS * MONTH_MS));
  return (amount * remaining) / (MAX_MONTHS * MONTH_MS);
}

export default function LockPage() {
  const { accountId, call } = useWallet();
  const lock = useQuery(() => (accountId ? api.lock.query({ account: accountId }) : Promise.resolve(null)), [accountId]);
  const balance = useQuery(
    () => (accountId ? rpc.view<string>(CONTRACTS.token, "ft_balance_of", { account_id: accountId }) : Promise.resolve("0")),
    [accountId],
  );
  const rewards = useQuery(
    () => (accountId ? api.fees.rewards.query({ account: accountId }) : Promise.resolve(null)),
    [accountId],
    60_000,
  );
  const [amount, setAmount] = useState("");
  const [months, setMonths] = useState(12);
  const [msg, setMsg] = useState<string>();

  const run = async (receiverId: string, method: string, args: Record<string, unknown>, deposit = "1") => {
    setMsg(undefined);
    try {
      await call({ receiverId, method, args, deposit, gasTgas: 150 });
      setMsg("Sent.");
      lock.reload();
      balance.reload();
    } catch (e) {
      setMsg((e as Error).message);
    }
  };

  const l = lock.data;
  const unlockMs = l ? Number(BigInt(l.unlock_at) / 1_000_000n) : 0;
  const amountNum = l ? Number(formatAmount(l.amount, 24, 4).replace(/,/g, "")) : 0;
  const preview = amount ? (Number(amount) * months) / MAX_MONTHS : 0;
  const decay = l ? [0, 3, 6, 12].map((m) => ({ m, p: powerAt(amountNum, unlockMs, Date.now() + m * MONTH_MS) })) : [];

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold">Lock CLAW</h1>
      <p className="max-w-2xl text-sm text-muted-foreground">
        Voting power comes only from locked CLAW. Lock for 1 to 24 months; power grows with lock length and decays as the
        unlock date gets closer. Locked tokens cannot be withdrawn early.
      </p>
      {!accountId && <p className="text-sm">Connect a wallet to see and manage your lock.</p>}

      {accountId && (
        <div className="grid gap-4 sm:grid-cols-3">
          <Stat label="Wallet balance" value={balance.data ? formatAmount(balance.data) : "…"} hint="CLAW" />
          <Stat label="Locked" value={l ? formatAmount(l.amount) : "0"} hint={l ? `unlocks ${when(l.unlock_at)}` : "no lock"} />
          <Stat label="Voting power now" value={l ? formatAmount(l.power) : "0"} />
        </div>
      )}
      <State loading={!!accountId && lock.loading} error={lock.error} />

      {accountId && rewards.data && (
        <Card className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <CardTitle className="mb-1">Holder rewards</CardTitle>
            <p className="text-sm">
              <span className="text-xl font-semibold tabular-nums">{formatAmount(rewards.data.pending, 24, 4)}</span> NEAR to claim
            </p>
            <p className="text-xs text-muted-foreground">
              Paid to locked CLAW, pro-rata by amount, when holders vote fees or treasury NEAR to holder rewards.
            </p>
          </div>
          <Button disabled={BigInt(rewards.data.pending) === 0n} onClick={() => run(CONTRACTS.ve, "claim_rewards", {}, "0").then(() => rewards.reload())}>
            Claim
          </Button>
        </Card>
      )}

      {accountId && (
        <div className="grid gap-6 md:grid-cols-2">
          <Card className="space-y-3">
            <CardTitle>{l ? "Add to lock" : "New lock"}</CardTitle>
            <label className="block space-y-1 text-sm">
              <span className="text-muted-foreground">Amount (CLAW)</span>
              <Input value={amount} onChange={(e) => setAmount(e.target.value)} placeholder="1000" inputMode="decimal" />
            </label>
            <label className="block space-y-1 text-sm">
              <span className="text-muted-foreground">Lock length: {months} months</span>
              <input type="range" min={1} max={24} value={months} onChange={(e) => setMonths(Number(e.target.value))} className="w-full accent-[hsl(var(--primary))]" />
            </label>
            <p className="text-xs text-muted-foreground">
              Starting power ≈ {preview.toLocaleString()} {l && "(added to your existing lock; the later unlock date wins)"}
            </p>
            <Button
              disabled={!amount}
              onClick={() =>
                run(CONTRACTS.token, "ft_transfer_call", {
                  receiver_id: CONTRACTS.ve,
                  amount: toRaw(amount),
                  msg: JSON.stringify({ months }),
                })
              }
            >
              Lock
            </Button>
          </Card>

          <Card className="space-y-3">
            <CardTitle>Your lock</CardTitle>
            {l ? (
              <>
                <table className="w-full text-sm">
                  <thead className="text-xs text-muted-foreground">
                    <tr>
                      <th className="text-left">In</th>
                      <th className="text-right">Power</th>
                    </tr>
                  </thead>
                  <tbody>
                    {decay.map(({ m, p }) => (
                      <tr key={m}>
                        <td>{m === 0 ? "now" : `${m} months`}</td>
                        <td className="text-right tabular-nums">{p.toLocaleString(undefined, { maximumFractionDigits: 0 })}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <div className="flex flex-wrap gap-2">
                  <Button variant="outline" onClick={() => run(CONTRACTS.ve, "extend_lock", { months: 24 }, "0")}>
                    Extend to 24 months
                  </Button>
                  <Button variant="outline" disabled={Date.now() < unlockMs} onClick={() => run(CONTRACTS.ve, "withdraw", {}, "0")}>
                    Withdraw
                  </Button>
                </div>
              </>
            ) : (
              <p className="text-sm text-muted-foreground">No lock yet.</p>
            )}
          </Card>
        </div>
      )}
      {msg && <p className="text-sm">{msg}</p>}
    </div>
  );
}
