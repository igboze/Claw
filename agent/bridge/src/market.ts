// NEAR AI Agent Market client for a self-hosted seller (https://market.near.ai, OpenAPI at
// /openapi.json). Authenticates with the per-agent `aat_` token, which can only act as
// this one agent: it can find, bid on, start and deliver jobs, never touch the owner's
// account, listing or balance.

export interface MarketJob {
  jobId: string;
  title: string;
  description: string;
  tags: string[];
  budgetAmount: string | null;
  budgetToken: string;
  jobKind: string;
  status: string;
}

export interface MarketAssignment {
  assignmentId: string;
  jobId: string;
  status: "in_progress" | "submitted" | "accepted" | "disputed" | "dispute_resolved" | "cancelled";
  escrowAmount: string;
  escrowToken: string;
  slaDeadlineAt: string;
  startedAt: string | null;
  submittedAt: string | null;
  deliverableHash: string | null;
  bidId: string | null;
}

export interface MarketAssignmentRow {
  assignment: MarketAssignment;
  job: Pick<MarketJob, "jobId" | "title" | "description" | "tags" | "status">;
  latestMessage: { body: string; truncated?: boolean } | null;
}

export interface MarketBid {
  bidId: string;
  jobId: string;
  amount: string;
  status: "pending" | "accepted" | "rejected" | "withdrawn";
}

export class MarketClient {
  constructor(
    private readonly token: string,
    private readonly baseUrl = "https://market.near.ai",
  ) {}

  private async req<T>(method: string, path: string, body?: unknown): Promise<T> {
    const res = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers: { authorization: `Bearer ${this.token}`, "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await res.text();
    if (!res.ok) throw new Error(`Agent Market ${method} ${path}: HTTP ${res.status} ${text.slice(0, 300)}`);
    return (text ? JSON.parse(text) : {}) as T;
  }

  /** Public board of standard jobs still taking bids. */
  async board(q?: string, limit = 25): Promise<MarketJob[]> {
    const params = new URLSearchParams({ kind: "standard", limit: String(limit) });
    if (q) params.set("q", q);
    return (await this.req<{ jobs: MarketJob[] }>("GET", `/v1/jobs/board?${params}`)).jobs;
  }

  async job(jobId: string): Promise<MarketJob> {
    return this.req("GET", `/v1/jobs/${encodeURIComponent(jobId)}`);
  }

  async assignments(status?: MarketAssignment["status"]): Promise<MarketAssignmentRow[]> {
    const params = new URLSearchParams({ limit: "100" });
    if (status) params.set("status", status);
    return (await this.req<{ assignments: MarketAssignmentRow[] }>("GET", `/v1/agents/me/assignments?${params}`)).assignments;
  }

  async myBids(): Promise<MarketBid[]> {
    const r = await this.req<{ bids?: MarketBid[] } | MarketBid[]>("GET", "/v1/agents/me/bids?limit=100");
    return Array.isArray(r) ? r : (r.bids ?? []);
  }

  bid(jobId: string, amount: string, proposal: string, slaSeconds?: number): Promise<MarketBid> {
    return this.req("POST", `/v1/jobs/${encodeURIComponent(jobId)}/bids`, { amount, proposal, slaSeconds });
  }

  start(assignmentId: string): Promise<{ startedAt: string }> {
    return this.req("POST", `/v1/assignments/${encodeURIComponent(assignmentId)}/start`);
  }

  submit(assignmentId: string, deliverableMarkdown: string, files: string[] = []): Promise<MarketAssignment> {
    return this.req("POST", `/v1/assignments/${encodeURIComponent(assignmentId)}/submit`, { deliverableMarkdown, files });
  }

  message(assignmentId: string, body: string): Promise<unknown> {
    return this.req("POST", `/v1/assignments/${encodeURIComponent(assignmentId)}/messages`, { body });
  }
}

/** "12.5", "12.500000000000" → 1250 cents (rounded down). */
export function toCents(amount: string): number {
  const [whole = "0", frac = ""] = amount.trim().split(".");
  if (!/^\d+$/.test(whole) || !/^\d*$/.test(frac)) throw new Error(`Bad amount: ${amount}`);
  return Number(whole) * 100 + Number((frac + "00").slice(0, 2));
}
