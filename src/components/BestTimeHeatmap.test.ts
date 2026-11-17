import { describe, expect, it } from "vitest";
import { cellColor } from "./BestTimeHeatmap";

describe("cellColor", () => {
  it("leaves cells without enough data uncoloured", () => {
    expect(cellColor({ score: 1, enoughData: false })).toBe("transparent");
  });
  it("is the neutral midpoint at 'as usual'", () => {
    expect(cellColor({ score: 0, enoughData: true })).toContain("0%");
  });
  it("uses blue above and red below, saturating at ×2 / ×0.5", () => {
    expect(cellColor({ score: Math.LN2, enoughData: true })).toBe(
      "color-mix(in oklab, var(--div-pos) 100%, var(--div-mid))",
    );
    expect(cellColor({ score: -5, enoughData: true })).toBe(
      "color-mix(in oklab, var(--div-neg) 100%, var(--div-mid))",
    );
  });
});
