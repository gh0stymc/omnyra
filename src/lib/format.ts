/** Number formatting shared by the screens. `null`/`undefined` render as an en dash. */
export function fmtNumber(v: number | null | undefined, lang: string, compact = false): string {
  if (v == null || Number.isNaN(v)) return "–";
  return v.toLocaleString(lang, {
    maximumFractionDigits: compact ? 1 : 0,
    notation: compact && Math.abs(v) >= 10_000 ? "compact" : "standard",
  });
}

export function fmtSigned(v: number | null | undefined, lang: string): string {
  if (v == null || Number.isNaN(v)) return "–";
  return v.toLocaleString(lang, { maximumFractionDigits: 0, signDisplay: "exceptZero" });
}

/** Seconds as hours with one decimal. */
export function fmtHours(seconds: number, lang: string): string {
  return (seconds / 3600).toLocaleString(lang, { maximumFractionDigits: 1 });
}

/** Seconds as m:ss or h:mm:ss. */
export function fmtDuration(seconds: number | null | undefined): string {
  if (seconds == null) return "–";
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

export function fmtDate(iso: string | null | undefined, lang: string): string {
  if (!iso) return "–";
  // Bare days (YYYY-MM-DD) are UTC calendar days: format them without shifting.
  const d = iso.length === 10 ? new Date(`${iso}T00:00:00Z`) : new Date(iso);
  return d.toLocaleDateString(lang, iso.length === 10 ? { timeZone: "UTC" } : undefined);
}
