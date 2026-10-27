// Minimal, dependency-free NEAR JSON-RPC client plus typed views for each contract.
// Writes go through wallets (web) or the agent's own key (bridge), never through here.

import type { ConstitutionView, LockView, MissionView, Proposal, Receipt } from "@clawdao/types";

export interface Accounts {
  token: string;
  ve: string;
  governor: string;
  constitution: string;
  agent: string;
}

export function accountsFromEnv(env: Record<string, string | undefined> = process.env): Accounts {
  const need = (k: string) => {
    const v = env[k];
    if (!v) throw new Error(`Missing env ${k}`);
    return v;
  };
  return {
    token: need("TOKEN_CONTRACT"),
    ve: need("VE_CONTRACT"),
    governor: need("GOVERNOR_CONTRACT"),
    constitution: need("CONSTITUTION_CONTRACT"),
    agent: need("AGENT_CONTRACT"),
  };
}

/** UTF-8 to base64 in both Node and browsers (the web app uses this client too). */
export function toBase64(s: string): string {
  let bin = "";
  for (const b of new TextEncoder().encode(s)) bin += String.fromCharCode(b);
  return btoa(bin);
}

export class NearRpc {
  constructor(readonly url: string) {}

  async request<T>(method: string, params: unknown): Promise<T> {
    const res = await fetch(this.url, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: "clawdao", method, params }),
    });
    if (!res.ok) throw new Error(`RPC ${method} HTTP ${res.status}`);
    const body = (await res.json()) as { result?: T; error?: { message: string; data?: unknown } };
    if (body.error) throw new Error(`RPC ${method}: ${body.error.message} ${JSON.stringify(body.error.data ?? "")}`);
    return body.result as T;
  }

  async view<T>(contract: string, method: string, args: Record<string, unknown> = {}, blockId?: number): Promise<T> {
    const result = await this.request<{ result: number[]; error?: string }>("query", {
      request_type: "call_function",
      account_id: contract,
      method_name: method,
      args_base64: toBase64(JSON.stringify(args)),
      ...(blockId ? { block_id: blockId } : { finality: "final" }),
    });
    if (result.error) throw new Error(result.error);
    return JSON.parse(new TextDecoder().decode(new Uint8Array(result.result))) as T;
  }

  block(finality: "final" | "optimistic" = "final") {
    return this.request<{ header: { height: number; timestamp_nanosec: string } }>("block", { finality });
  }

  async accountBalance(accountId: string): Promise<bigint> {
    const r = await this.request<{ amount: string }>("query", {
      request_type: "view_account",
      account_id: accountId,
      finality: "final",
    });
    return BigInt(r.amount);
  }
}

export class ClawViews {
  constructor(readonly rpc: NearRpc, readonly accounts: Accounts) {}

  constitution() {
    return this.rpc.view<ConstitutionView>(this.accounts.constitution, "get_constitution");
  }
  hash() {
    return this.rpc.view<[number, string]>(this.accounts.constitution, "get_hash");
  }
  activeMissions() {
    return this.rpc.view<MissionView[]>(this.accounts.constitution, "get_active_missions");
  }
  missions() {
    return this.rpc.view<MissionView[]>(this.accounts.constitution, "get_missions");
  }
  receipts(from_index = 0, limit = 50) {
    return this.rpc.view<Receipt[]>(this.accounts.constitution, "get_receipts", { from_index, limit });
  }
  dailySpent(asset: string) {
    return this.rpc.view<[string, string]>(this.accounts.constitution, "get_daily_spent", { asset });
  }
  proposals(from_index = 0, limit = 50) {
    return this.rpc.view<Proposal[]>(this.accounts.governor, "get_proposals", { from_index, limit });
  }
  proposal(proposal_id: number) {
    return this.rpc.view<Proposal | null>(this.accounts.governor, "get_proposal", { proposal_id });
  }
  lock(account_id: string) {
    return this.rpc.view<LockView | null>(this.accounts.ve, "get_lock", { account_id });
  }
  power(account_id: string) {
    return this.rpc.view<string>(this.accounts.ve, "power_of", { account_id });
  }
  balance(account_id: string) {
    return this.rpc.view<string>(this.accounts.token, "ft_balance_of", { account_id });
  }
  agentInfo() {
    return this.rpc.view<Record<string, unknown>>(this.accounts.agent, "get_contract_info");
  }
  agents() {
    return this.rpc.view<Record<string, unknown>[]>(this.accounts.agent, "get_agents", {});
  }
  approvedMeasurements() {
    return this.rpc.view<unknown[]>(this.accounts.agent, "get_approved_measurements");
  }
}

/** Parse NEP-297 `EVENT_JSON:` log lines. Non-event logs return null. */
export function parseEventLog(log: string): { standard: string; version: string; event: string; data: unknown[] } | null {
  if (!log.startsWith("EVENT_JSON:")) return null;
  try {
    const v = JSON.parse(log.slice("EVENT_JSON:".length));
    return typeof v?.event === "string" ? v : null;
  } catch {
    return null;
  }
}

export const NEAR_DECIMALS = 24;

export function formatAmount(raw: string | bigint, decimals = NEAR_DECIMALS, digits = 2): string {
  const v = BigInt(raw);
  const base = 10n ** BigInt(decimals);
  const whole = (v / base).toLocaleString("en-US");
  if (digits === 0) return whole;
  const frac = (v % base).toString().padStart(decimals, "0").slice(0, digits);
  return `${whole}.${frac}`;
}
