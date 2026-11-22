import { ArrowDownRight, ArrowUpRight } from "lucide-react";

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
      ? "bg-emerald-500/12 text-emerald-600 dark:text-emerald-400"
      : subTone === "down"
        ? "bg-rose-500/12 text-rose-600 dark:text-rose-400"
        : "text-[var(--muted)]";
  return (
    <div className="group relative overflow-hidden rounded-2xl border border-[var(--border)] bg-[var(--panel)] p-4 hover:border-[var(--border-strong)]">
      <div className="pointer-events-none absolute -right-10 -top-10 size-28 rounded-full bg-[var(--accent-soft)] blur-2xl transition-opacity group-hover:opacity-80" />
      <p className="text-[11px] font-semibold uppercase tracking-[0.08em] text-[var(--muted)]">
        {label}
      </p>
      <p className="mt-1.5 text-[28px] font-bold leading-none tracking-tight tabular-nums">
        {value}
      </p>
      {sub && (
        <p
          className={`mt-2 inline-flex items-center gap-0.5 rounded-full text-xs font-medium ${subTone !== "muted" ? "px-2 py-0.5" : ""} ${tone}`}
        >
          {subTone === "up" && <ArrowUpRight size={12} />}
          {subTone === "down" && <ArrowDownRight size={12} />}
          {sub}
        </p>
      )}
    </div>
  );
}

export function changeTone(v: number | null | undefined): "muted" | "up" | "down" {
  if (!v) return "muted";
  return v > 0 ? "up" : "down";
}
