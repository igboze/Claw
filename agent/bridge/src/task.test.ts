import { describe, expect, it } from "vitest";
import { taskId } from "./task.js";

describe("taskId", () => {
  it("is the same for every worker in the same loop", () => {
    expect(taskId("bounties", 42)).toBe(taskId("bounties", 42));
  });
  it("differs across loops, missions and seq", () => {
    const ids = new Set([taskId("a", 1), taskId("a", 2), taskId("b", 1), taskId("a", 1, 1)]);
    expect(ids.size).toBe(4);
  });
  it("matches sha256(mission:epoch)", () => {
    expect(taskId("m", 1)).toBe("52f0d6eedeb7a050032338a08840f7c69187d2816370c730a8da7331f555e0ea");
  });
});
