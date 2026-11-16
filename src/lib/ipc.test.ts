import { describe, expect, it } from "vitest";
import { errorMessage, isAppError } from "./ipc";

const t = ((k: string, o?: Record<string, unknown>) =>
  `${k}${o?.seconds ?? o?.field ?? ""}`) as never;

describe("errorMessage", () => {
  it("maps known codes", () => {
    expect(errorMessage({ code: "NETWORK" }, t)).toBe("errors.NETWORK");
  });
  it("interpolates details", () => {
    expect(errorMessage({ code: "RATE_LIMITED", detail: { retry_after_s: 30 } }, t)).toBe(
      "errors.RATE_LIMITED30",
    );
  });
  it("falls back for unknown shapes", () => {
    expect(isAppError("boom")).toBe(false);
    expect(errorMessage("boom", t)).toBe("errors.unknown");
  });
});
