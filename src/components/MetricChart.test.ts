import { describe, expect, it } from "vitest";
import { rangeStart } from "./MetricChart";

describe("rangeStart", () => {
  const now = new Date("2026-11-17T12:00:00Z");
  it("includes today in the last 30 days", () => {
    expect(rangeStart("30d", now)).toBe("2026-10-19");
  });
  it("goes back whole years", () => {
    expect(rangeStart("1y", now)).toBe("2025-11-17");
    expect(rangeStart("5y", now)).toBe("2021-11-17");
  });
  it("has no lower bound for everything", () => {
    expect(rangeStart("all", now)).toBeNull();
  });
});
