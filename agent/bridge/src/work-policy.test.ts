import { describe, expect, it } from "vitest";
import type { MissionView } from "@clawdao/types";
import { toCents, type MarketJob } from "./market.js";
import { checkBid, missionForJob, workMissions } from "./work-policy.js";

const mission = (id: string, tags: string[], extra: Partial<MissionView> = {}): MissionView => ({
  id,
  title: id,
  goal: "",
  asset: "near",
  budget: "0",
  max_per_action: "0",
  deadline: "9000000000000000000",
  metric: "",
  allow_chain_sig: false,
  spent: "0",
  cancelled: false,
  status: "Active",
  work: { tags, min_price_cents: 500, max_open: 2, currencies: ["USDC"] },
  ...extra,
});

const job = (over: Partial<MarketJob> = {}): MarketJob => ({
  jobId: "11111111-1111-1111-1111-111111111111",
  title: "t",
  description: "d",
  tags: ["Research"],
  budgetAmount: "10",
  budgetToken: "USDC",
  jobKind: "standard",
  status: "open",
  ...over,
});

const ok = { openAssignments: [], pendingBids: [] };

describe("work policy", () => {
  const [m] = workMissions([mission("w1", ["research"]), mission("spend", [], { work: undefined })]);

  it("only active work missions count", () => {
    expect(workMissions([mission("w1", ["a"]), mission("w2", ["a"], { status: "Expired" })]).map((x) => x.id)).toEqual(["w1"]);
  });

  it("allows a matching bid at or above the minimum", () => {
    expect(checkBid({ mission: m!, job: job(), amount: "5.00", ...ok })).toBeNull();
  });

  it("rejects low bids, wrong tags, wrong currency", () => {
    expect(checkBid({ mission: m!, job: job(), amount: "4.99", ...ok })).toMatch(/minimum/);
    expect(checkBid({ mission: m!, job: job({ tags: ["design"] }), amount: "9", ...ok })).toMatch(/do not match/);
    expect(checkBid({ mission: m!, job: job({ budgetToken: "USD" }), amount: "9", ...ok })).toMatch(/Currency/);
  });

  it("enforces max_open across bids and assignments", () => {
    const pendingBids = [{ bidId: "b", jobId: "x", amount: "5", status: "pending" as const }];
    const openAssignments = [{ assignment: { status: "in_progress" } } as never];
    expect(checkBid({ mission: m!, job: job(), amount: "6", pendingBids, openAssignments })).toMatch(/max_open/);
  });

  it("attributes jobs deterministically", () => {
    const ms = workMissions([mission("b", ["research"]), mission("a", ["research"])]);
    expect(missionForJob(ms, { tags: ["research"] })?.id).toBe("a");
  });

  it("converts marketplace amounts to cents", () => {
    expect(toCents("12.500000000000")).toBe(1250);
    expect(toCents("7")).toBe(700);
  });
});
