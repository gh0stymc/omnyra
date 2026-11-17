import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Cell, type Format } from "../lib/bindings";
import { errorMessage } from "../lib/ipc";
import { queryKeys } from "../lib/queryKeys";

/** |score| at which the colour saturates: ln 2, i.e. ×2 or ×0.5 views. */
const SATURATION = Math.LN2;
const HOUR_LABELS = [0, 3, 6, 9, 12, 15, 18, 21];

/** Background for a cell: neutral gray at "as usual", blue above, red below. */
export function cellColor(cell: Pick<Cell, "score" | "enoughData">): string {
  if (!cell.enoughData) return "transparent";
  const score = cell.score ?? 0; // f64 arrives as number | null
  const t = Math.min(Math.abs(score) / SATURATION, 1);
  const pole = score >= 0 ? "var(--div-pos)" : "var(--div-neg)";
  return `color-mix(in oklab, ${pole} ${Math.round(t * 100)}%, var(--div-mid))`;
}

function weekdayName(wd: number, lang: string): string {
  // 1 January 2024 was a Monday.
  return new Date(Date.UTC(2024, 0, 1 + wd)).toLocaleDateString(lang, {
    weekday: "long",
    timeZone: "UTC",
  });
}

const hh = (h: number) => `${String(h).padStart(2, "0")}:00`;

/** Weekday × hour map of how first-week views compare with the channel's usual. */
export function BestTimeHeatmap({ accountId }: { accountId: string }) {
  const { t, i18n } = useTranslation();
  const [format, setFormat] = useState<Format>("video");
  const [includeUnlisted, setIncludeUnlisted] = useState(false);
  const [hover, setHover] = useState<Cell | null>(null);
  const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const q = useQuery({
    queryKey: queryKeys.bestTime(accountId, format, includeUnlisted, timezone),
    queryFn: () => commands.getBestTime(accountId, { format, includeUnlisted, timezone }),
  });
  const lang = i18n.language;
  const pct = (factor: number | null) =>
    ((factor ?? 1) - 1).toLocaleString(lang, { style: "percent", signDisplay: "always" });
  const fmtFormat = t(`bestTime.format.${format}`);
  const describe = (c: Cell) =>
    `${weekdayName(c.weekday, lang)} ${hh(c.hour)} · ` +
    (c.enoughData
      ? t("bestTime.cellValue", { pct: pct(c.factor), count: c.contents })
      : t("bestTime.cellNoData", { count: c.contents }));
  const tab = (active: boolean) =>
    `rounded-md px-2 py-1 text-xs ${active ? "bg-[var(--accent)] text-white" : "border border-[var(--border)]"}`;
  const data = q.data;

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-sm font-medium">{t("bestTime.title")}</h3>
        <div className="flex flex-wrap items-center gap-3">
          <div className="flex gap-1" role="group" aria-label={t("bestTime.formatLabel")}>
            {(["video", "short"] as const).map((f) => (
              <button
                key={f}
                aria-pressed={f === format}
                className={tab(f === format)}
                onClick={() => setFormat(f)}
              >
                {t(`bestTime.format.${f}`)}
              </button>
            ))}
          </div>
          <label className="flex items-center gap-1 text-xs">
            <input
              type="checkbox"
              checked={includeUnlisted}
              onChange={(e) => setIncludeUnlisted(e.target.checked)}
            />
            {t("bestTime.includeUnlisted")}
          </label>
        </div>
      </div>

      {q.isError && <p className="text-sm text-red-500">{errorMessage(q.error, t)}</p>}
      {data && includeUnlisted && data.sufficient && (
        <p className="text-xs text-amber-500">{t("bestTime.indicative")}</p>
      )}

      {data && !data.sufficient && (
        <p className="text-sm text-[var(--muted)]">
          {t("bestTime.insufficient", {
            sample: data.sample,
            needed: data.needed,
            format: fmtFormat,
          })}{" "}
          {!includeUnlisted && t("bestTime.tryUnlisted")}
        </p>
      )}

      {data?.sufficient && (
        <>
          <p className="text-sm">
            {data.best.length > 0
              ? t("bestTime.best", {
                  list: data.best
                    .map((c) => `${weekdayName(c.weekday, lang)} ${hh(c.hour)} (${pct(c.factor)})`)
                    .join(", "),
                })
              : t("bestTime.noStandout")}
          </p>
          <div
            className="grid gap-[2px] text-[10px] text-[var(--muted)]"
            style={{ gridTemplateColumns: "auto repeat(24, minmax(0, 1fr))" }}
            onMouseLeave={() => setHover(null)}
          >
            <span />
            {Array.from({ length: 24 }, (_, h) => (
              <span key={h} className="text-center">
                {HOUR_LABELS.includes(h) ? h : ""}
              </span>
            ))}
            {Array.from({ length: 7 }, (_, wd) => (
              <div key={wd} className="contents">
                <span className="pr-1 capitalize">{weekdayName(wd, lang).slice(0, 3)}</span>
                {data.cells
                  .filter((c) => c.weekday === wd)
                  .map((c) => (
                    <div
                      key={c.hour}
                      role="img"
                      aria-label={describe(c)}
                      title={describe(c)}
                      onMouseEnter={() => setHover(c)}
                      className={`h-5 rounded-[3px] ${c.enoughData ? "" : "border border-dashed border-[var(--border)]"} ${
                        hover === c ? "outline-2 outline-[var(--text)]" : ""
                      }`}
                      style={{ background: cellColor(c) }}
                    />
                  ))}
              </div>
            ))}
          </div>
          <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-[var(--muted)]">
            <span aria-live="polite">{hover ? describe(hover) : t("bestTime.hoverHint")}</span>
            <span className="flex items-center gap-2">
              {pct(0.5)}
              <span
                className="inline-block h-2 w-24 rounded-sm"
                style={{
                  background:
                    "linear-gradient(to right, var(--div-neg), var(--div-mid), var(--div-pos))",
                }}
              />
              {pct(2)}
              <span className="ml-2 inline-block h-2 w-3 rounded-sm border border-dashed border-[var(--border)]" />
              {t("bestTime.legendNoData")}
            </span>
          </div>
          <p className="text-xs text-[var(--muted)]">
            {t("bestTime.footnote", { sample: data.sample, format: fmtFormat, tz: data.timezone })}
          </p>
        </>
      )}
    </div>
  );
}
