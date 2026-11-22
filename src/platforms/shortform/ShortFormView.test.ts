import { describe, expect, it } from "vitest";
import { contentTotals } from "./ShortFormView";

describe("contentTotals", () => {
  const rows = [
    { publishedAt: "2026-11-20T10:00:00Z", views: 100, likes: 5, comments: null },
    { publishedAt: "2026-10-01T10:00:00Z", views: 50, likes: 1, comments: 2 },
  ];
  it("sums only contents in the period and keeps null when nothing is known", () => {
    expect(contentTotals(rows, "2026-11-01")).toEqual({
      count: 1,
      views: 100,
      likes: 5,
      comments: null,
    });
    expect(contentTotals(rows, null)).toEqual({ count: 2, views: 150, likes: 6, comments: 2 });
  });
});
