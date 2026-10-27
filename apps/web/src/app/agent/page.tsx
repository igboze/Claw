"use client";

import { api, useQuery } from "@/lib/api";
import { CONTRACTS, EXPLORER, REPO_URL } from "@/lib/config";
import { formatAmount, short, when } from "@/lib/format";
import { Badge, Card, CardTitle, State } from "@/components/ui";

interface Info {
  requires_tee: boolean;
  paused: boolean;
  guardian_id: string | null;
  guardian_expiry_block: string;
  attestation_expiration_time_ms: string;
  mpc_contract_id: string;
  treasury_balance: string;
}

export default function AgentPage() {
  const q = useQuery(() => api.agent.query(), [], 30_000);
  const info = q.data?.info as Info | undefined;

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold">Agent</h1>
      <p className="max-w-2xl text-sm text-muted-foreground">
        Workers run one Docker image inside a TEE. They register with a hardware attestation; the agent contract only
        accepts images whose measurements (code hash) the DAO approved in a Tier 1 vote. Release notes list each image's
        SHA256 next to the matching Git tag.
      </p>
      <State loading={q.loading} error={q.error} />

      {info && (
        <div className="grid gap-4 md:grid-cols-3">
          <Card>
            <CardTitle>Contract</CardTitle>
            <a className="font-mono text-sm text-primary hover:underline" href={`${EXPLORER}/address/${CONTRACTS.agent}`}>
              {CONTRACTS.agent}
            </a>
            <p className="mt-2 text-sm">
              Status: <Badge tone={info.paused ? "Failed" : "Active"}>{info.paused ? "paused" : "running"}</Badge>
            </p>
            <p className="text-sm">TEE required: {info.requires_tee ? "yes" : "no (local/test only)"}</p>
            <p className="text-sm">Treasury: {formatAmount(info.treasury_balance)} NEAR</p>
          </Card>
          <Card>
            <CardTitle>Guardian (phase 4)</CardTitle>
            {info.guardian_id ? (
              <>
                <p className="font-mono text-sm">{info.guardian_id}</p>
                <p className="text-sm text-muted-foreground">Can pause only, never move funds. Expires at block {info.guardian_expiry_block}.</p>
              </>
            ) : (
              <p className="text-sm">No guardian.</p>
            )}
          </Card>
          <Card>
            <CardTitle>Signing</CardTitle>
            <p className="text-sm">MPC: {info.mpc_contract_id}</p>
            <p className="text-sm">Attestation valid for {Math.round(Number(info.attestation_expiration_time_ms) / 86_400_000)} days</p>
          </Card>
        </div>
      )}

      <Card>
        <CardTitle>Approved code hashes</CardTitle>
        <State empty={q.data?.measurements.length ? undefined : "None approved yet."} />
        {q.data?.measurements.map((m, i) => {
          const mm = m as { app_compose_hash_payload?: string; rtmrs?: Record<string, string> };
          return (
            <div key={i} className="mb-3 rounded-md border border-border p-3 text-xs">
              <div>
                compose hash <span className="font-mono">{mm.app_compose_hash_payload}</span>
              </div>
              {mm.rtmrs &&
                Object.entries(mm.rtmrs).map(([k, v]) => (
                  <div key={k}>
                    {k} <span className="font-mono">{short(v, 16)}</span>
                  </div>
                ))}
              <a className="mt-1 inline-block text-primary hover:underline" href={`${REPO_URL}/releases`}>
                Match against release notes →
              </a>
            </div>
          );
        })}
      </Card>

      <Card>
        <CardTitle>Workers</CardTitle>
        <State empty={q.data?.workers.length ? undefined : "No registered workers."} />
        <table className="w-full text-left text-sm">
          <thead className="text-xs text-muted-foreground">
            <tr>
              <th className="py-2">Account</th>
              <th>Attestation valid until</th>
              <th>Last heartbeat</th>
              <th>Loop</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border">
            {q.data?.workers.map((w) => (
              <tr key={w.accountId}>
                <td className="py-2 font-mono">{short(w.accountId, 8)}</td>
                <td>{when(w.validUntil)}</td>
                <td>{when(w.lastHeartbeat)}</td>
                <td>{w.lastEpoch ?? "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Card>
    </div>
  );
}
