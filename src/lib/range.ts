export const RANGES = ["30d", "1y", "5y", "all"] as const;
export type Range = (typeof RANGES)[number];

/** First day (`YYYY-MM-DD`, UTC) of `range`, or null for everything. */
export function rangeStart(range: Range, now = new Date()): string | null {
  const d = new Date(now);
  if (range === "30d") d.setUTCDate(d.getUTCDate() - 29);
  else if (range === "1y") d.setUTCFullYear(d.getUTCFullYear() - 1);
  else if (range === "5y") d.setUTCFullYear(d.getUTCFullYear() - 5);
  else return null;
  return d.toISOString().slice(0, 10);
}

export function isRange(v: unknown): v is Range {
  return typeof v === "string" && (RANGES as readonly string[]).includes(v);
}
