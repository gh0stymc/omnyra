/** A headline number with an optional secondary line (change, context). */
export function StatCard({
  label,
  value,
  sub,
  subTone = "muted",
}: {
  label: string;
  value: string;
  sub?: string;
  subTone?: "muted" | "up" | "down";
}) {
  const tone =
    subTone === "up"
      ? "text-green-600 dark:text-green-400"
      : subTone === "down"
        ? "text-red-500"
        : "text-[var(--muted)]";
  return (
    <div className="rounded-lg border border-[var(--border)] bg-[var(--panel)] p-3">
      <p className="text-xs text-[var(--muted)]">{label}</p>
      <p className="text-2xl font-semibold tabular-nums">{value}</p>
      {sub && <p className={`text-xs ${tone}`}>{sub}</p>}
    </div>
  );
}

export function changeTone(v: number | null | undefined): "muted" | "up" | "down" {
  if (!v) return "muted";
  return v > 0 ? "up" : "down";
}
