import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { ContentRow } from "../lib/bindings";
import { fmtDate, fmtDuration, fmtNumber } from "../lib/format";

const KINDS = ["all", "video", "short", "stream"] as const;
type Sort = "date" | "views";

/** Contents of an account, with kind filter and sorting. */
export function ContentTable({ rows, from }: { rows: ContentRow[]; from: string | null }) {
  const { t, i18n } = useTranslation();
  const [kind, setKind] = useState<(typeof KINDS)[number]>("all");
  const [sort, setSort] = useState<Sort>("date");
  const lang = i18n.language;
  const visible = rows
    .filter((r) => kind === "all" || r.kind === kind)
    .filter((r) => from === null || (r.publishedAt ?? "").slice(0, 10) >= from)
    .sort((a, b) =>
      sort === "views"
        ? (b.views ?? 0) - (a.views ?? 0)
        : (b.publishedAt ?? "").localeCompare(a.publishedAt ?? ""),
    );
  const pill = (active: boolean) =>
    `rounded-md px-2 py-1 text-xs ${active ? "bg-[var(--accent)] text-white" : "border border-[var(--border)]"}`;

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex gap-1">
          {KINDS.map((k) => (
            <button
              key={k}
              aria-pressed={k === kind}
              className={pill(k === kind)}
              onClick={() => setKind(k)}
            >
              {t(`contents.kind.${k}`)}
            </button>
          ))}
        </div>
        <label className="flex items-center gap-2 text-xs">
          {t("contents.sortBy")}
          <select
            className="rounded-md border border-[var(--border)] px-2 py-1"
            value={sort}
            onChange={(e) => setSort(e.target.value as Sort)}
          >
            <option value="date">{t("contents.sort.date")}</option>
            <option value="views">{t("contents.sort.views")}</option>
          </select>
        </label>
      </div>
      {visible.length === 0 ? (
        <p className="text-sm text-[var(--muted)]">{t("contents.empty")}</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="text-left text-xs text-[var(--muted)]">
              <tr>
                <th className="py-1 pr-2 font-normal">{t("contents.col.title")}</th>
                <th className="py-1 pr-2 font-normal">{t("contents.col.kind")}</th>
                <th className="py-1 pr-2 font-normal">{t("contents.col.published")}</th>
                <th className="py-1 pr-2 text-right font-normal">{t("contents.col.duration")}</th>
                <th className="py-1 pr-2 text-right font-normal">{t("contents.col.views")}</th>
                <th className="py-1 pr-2 text-right font-normal">{t("contents.col.likes")}</th>
                <th className="py-1 text-right font-normal">{t("contents.col.comments")}</th>
              </tr>
            </thead>
            <tbody>
              {visible.map((r) => (
                <tr key={r.id} className="border-t border-[var(--border)]">
                  <td className="max-w-xs py-1 pr-2">
                    {r.url ? (
                      <button
                        className="truncate text-left hover:underline"
                        onClick={() => void openUrl(r.url!)}
                      >
                        {r.title || t("upload.untitled")}
                      </button>
                    ) : (
                      <span className="truncate">{r.title || t("upload.untitled")}</span>
                    )}
                    {r.visibility && r.visibility !== "public" && (
                      <span className="ml-1 text-xs text-[var(--muted)]">
                        ·{" "}
                        {t(`upload.visibilityOption.${r.visibility}`, {
                          defaultValue: r.visibility,
                        })}
                      </span>
                    )}
                  </td>
                  <td className="py-1 pr-2 text-xs">
                    {t(`contents.kind.${r.kind}`, { defaultValue: r.kind })}
                  </td>
                  <td className="py-1 pr-2 text-xs">{fmtDate(r.publishedAt, lang)}</td>
                  <td className="py-1 pr-2 text-right tabular-nums">{fmtDuration(r.durationS)}</td>
                  <td className="py-1 pr-2 text-right tabular-nums">{fmtNumber(r.views, lang)}</td>
                  <td className="py-1 pr-2 text-right tabular-nums">{fmtNumber(r.likes, lang)}</td>
                  <td className="py-1 text-right tabular-nums">{fmtNumber(r.comments, lang)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
