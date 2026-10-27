import { describe, expect, it } from "vitest";
import { formatEvent } from "./format.js";

describe("formatEvent", () => {
  it("formats receipts with amount and reasoning link", () => {
    const t = formatEvent("receipt", { mission_id: "bounties", amount: "1500000000000000000000000", reasoning_uri: "ipfs://Qm" }, "https://x");
    expect(t).toContain("1.50");
    expect(t).toContain("ipfs://Qm");
  });
  it("stays quiet on genesis and unknown events", () => {
    expect(formatEvent("constitution_updated", { reason: "genesis" }, "")).toBeNull();
    expect(formatEvent("spend_checked", {}, "")).toBeNull();
  });
});
