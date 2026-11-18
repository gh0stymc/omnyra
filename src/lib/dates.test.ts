import { describe, expect, it } from "vitest";
import {
  addDays,
  dayKey,
  fromLocalInput,
  monthGrid,
  moveTo,
  startOfWeek,
  toLocalInput,
} from "./dates";

describe("dates", () => {
  it("weeks start on Monday", () => {
    // Sunday 22 November 2026 → Monday 16.
    expect(dayKey(startOfWeek(new Date(2026, 10, 22, 15)))).toBe("2026-11-16");
    expect(dayKey(startOfWeek(new Date(2026, 10, 16)))).toBe("2026-11-16");
  });
  it("the month grid covers 6 weeks from the Monday before the 1st", () => {
    const g = monthGrid(new Date(2026, 10, 17));
    expect(g).toHaveLength(42);
    expect(dayKey(g[0])).toBe("2026-10-26");
    expect(g.some((d) => dayKey(d) === "2026-11-30")).toBe(true);
  });
  it("moving keeps the local time of day unless an hour is given", () => {
    const at = new Date(2026, 10, 10, 18, 30).toISOString();
    expect(toLocalInput(moveTo(at, new Date(2026, 10, 12)))).toBe("2026-11-12T18:30");
    expect(toLocalInput(moveTo(at, new Date(2026, 10, 12), 9))).toBe("2026-11-12T09:30");
  });
  it("round-trips datetime-local values", () => {
    expect(toLocalInput(fromLocalInput("2026-03-29T02:30"))).toMatch(/^2026-03-29T0[23]:30$/);
    expect(fromLocalInput("")).toBeNull();
    expect(dayKey(addDays(new Date(2026, 11, 31), 1))).toBe("2027-01-01");
  });
});
