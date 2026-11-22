import { describe, expect, it } from "vitest";
import { formatMoney, parseMoney } from "./money";

describe("money", () => {
  it("parses European and English formats into cents", () => {
    expect(parseMoney("500")).toBe(50000);
    expect(parseMoney("1.234,56")).toBe(123456);
    expect(parseMoney("1,234.56")).toBe(123456);
    expect(parseMoney("12,5")).toBe(1250);
    expect(parseMoney("1.500")).toBe(150000);
    expect(parseMoney("")).toBeNull();
    expect(parseMoney("abc")).toBeNull();
    expect(parseMoney("1,234")).toBe(123400);
  });
  it("formats with the currency", () => {
    expect(formatMoney(123456, "EUR", "en")).toContain("1,234.56");
    expect(formatMoney(null, "EUR", "es")).toBe("—");
  });
});
