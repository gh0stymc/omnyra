import { describe, expect, it } from "vitest";
import { fmtDate, fmtDuration, fmtHours, fmtNumber, fmtSigned } from "./format";

describe("format", () => {
  it("formats numbers and missing values", () => {
    expect(fmtNumber(1234, "es")).toBe("1234");
    expect(fmtNumber(null, "es")).toBe("–");
    expect(fmtNumber(25_000, "en", true)).toBe("25K");
    expect(fmtSigned(5, "en")).toBe("+5");
    expect(fmtSigned(0, "en")).toBe("0");
  });
  it("formats durations", () => {
    expect(fmtDuration(65)).toBe("1:05");
    expect(fmtDuration(3725)).toBe("1:02:05");
    expect(fmtHours(5400, "en")).toBe("1.5");
  });
  it("keeps bare days on their calendar day", () => {
    expect(fmtDate("2026-11-01", "en-US")).toBe("11/1/2026");
  });
});
