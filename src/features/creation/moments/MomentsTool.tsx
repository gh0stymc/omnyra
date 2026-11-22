import { useMutation, useQuery } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { commands, type Fit, type Moment } from "../../../lib/bindings";
import { fmtDuration } from "../../../lib/format";
import { errorMessage } from "../../../lib/ipc";
import { queryKeys } from "../../../lib/queryKeys";

const field = "rounded-md border border-[var(--border)] bg-[var(--bg)] px-2 py-1 text-sm";
const btn = "rounded-md border border-[var(--border)] px-2 py-1 text-xs disabled:opacity-50";

/** Mini line chart of the analysed series with the moments highlighted. */
export function MomentChart({
  series,
  moments,
  duration,
}: {
  series: [number | null, number | null][];
  moments: Moment[];
  duration: number;
}) {
  const pts = series.map(([a, b]) => [a ?? 0, b ?? 0] as const);
  if (pts.length < 2 || duration <= 0) return null;
  const W = 600;
  const H = 120;
  const maxY = Math.max(...pts.map((p) => p[1]), 1e-9);
  const x = (t: number) => (t / duration) * W;
  const y = (v: number) => H - (v / maxY) * (H - 8) - 4;
  const d = pts
    .map((p, i) => `${i ? "L" : "M"}${x(p[0]).toFixed(1)},${y(p[1]).toFixed(1)}`)
    .join(" ");
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="h-32 w-full rounded-md border border-[var(--border)]">
      {moments.map((m) => (
        <rect
          key={m.startS}
          x={x(m.startS ?? 0)}
          width={Math.max(2, x(m.endS ?? 0) - x(m.startS ?? 0))}
          y={0}
          height={H}
          fill="var(--accent)"
          opacity={0.2}
        />
      ))}
      <path d={d} fill="none" stroke="var(--accent)" strokeWidth={1.5} />
    </svg>
  );
}

/** Long video or stream → best moments → vertical clips → multi-network publishing. */
export function MomentsTool() {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const [accountId, setAccountId] = useState("");
  const [contentId, setContentId] = useState("");
  const [file, setFile] = useState("");
  const [fit, setFit] = useState<Fit>("blur");
  const [edits, setEdits] = useState<Record<number, { s: number; e: number }>>({});
  const accounts = useQuery({
    queryKey: queryKeys.accounts,
    queryFn: () => commands.listAccounts(),
  });
  const usable = (accounts.data ?? []).filter(
    (a) =>
      a.status === "active" &&
      (a.platform === "youtube" || a.platform === "twitch" || a.platform === "mock"),
  );
  const contents = useQuery({
    queryKey: queryKeys.contents(accountId),
    queryFn: () => commands.listContents(accountId),
    enabled: !!accountId,
  });
  const long = (contents.data ?? []).filter(
    (c) => ["video", "vod", "stream"].includes(c.kind) && (c.durationS ?? 0) >= 60,
  );
  const report = useQuery({
    queryKey: ["moments", contentId],
    queryFn: () => commands.getMoments(contentId),
    enabled: !!contentId,
  });
  const cut = useMutation({
    mutationFn: (m: { s: number; e: number }) => commands.cutMoment(file, m.s, m.e, fit),
    onSuccess: (path) => navigate(`/publish?file=${encodeURIComponent(path)}`),
  });
  const pick = async () => {
    const p = await open({
      multiple: false,
      filters: [{ name: t("upload.videoFiles"), extensions: ["mp4", "mov", "mkv", "webm", "flv"] }],
    });
    if (typeof p === "string") setFile(p);
  };
  const range = (i: number, m: Moment) => edits[i] ?? { s: m.startS ?? 0, e: m.endS ?? 0 };

  return (
    <div className="max-w-4xl space-y-4">
      <p className="text-xs text-[var(--muted)]">{t("moments.intro")}</p>
      <div className="flex flex-wrap gap-2">
        <select
          className={field}
          value={accountId}
          onChange={(e) => {
            setAccountId(e.target.value);
            setContentId("");
          }}
        >
          <option value="">{t("moments.pickAccount")}</option>
          {usable.map((a) => (
            <option key={a.id} value={a.id}>
              {a.displayName} ({a.platform})
            </option>
          ))}
        </select>
        <select
          className={field}
          value={contentId}
          onChange={(e) => {
            setContentId(e.target.value);
            setEdits({});
          }}
          disabled={!accountId}
        >
          <option value="">{t("moments.pickContent")}</option>
          {long.map((c) => (
            <option key={c.id} value={c.id}>
              {c.title ?? c.externalId} · {fmtDuration(c.durationS)}
            </option>
          ))}
        </select>
      </div>
      {report.isPending && contentId && (
        <p className="text-sm text-[var(--muted)]">{t("common.loading")}</p>
      )}
      {report.isError && <p className="text-sm text-red-500">{errorMessage(report.error, t)}</p>}
      {report.data && !report.data.source && (
        <p className="text-sm text-[var(--muted)]">{t("moments.noData")}</p>
      )}
      {report.data?.source && (
        <section className="space-y-3">
          <p className="text-xs text-[var(--muted)]">{t(`moments.source.${report.data.source}`)}</p>
          <MomentChart
            series={report.data.series}
            moments={report.data.moments}
            duration={report.data.durationS ?? 0}
          />
          <div className="flex flex-wrap items-center gap-2 text-sm">
            <button className={btn} onClick={() => void pick()}>
              {t("moments.pickFile")}
            </button>
            <span className="break-all text-xs">
              {file ? file.split(/[\\/]/).pop() : t("moments.fileHint")}
            </span>
            <select className={field} value={fit} onChange={(e) => setFit(e.target.value as Fit)}>
              {(["blur", "crop", "pad"] as const).map((f) => (
                <option key={f} value={f}>
                  {t(`publish.fitOption.${f}`)}
                </option>
              ))}
            </select>
          </div>
          {!report.data.moments.length && (
            <p className="text-sm text-[var(--muted)]">{t("moments.none")}</p>
          )}
          <ul className="space-y-2">
            {report.data.moments.map((m, i) => {
              const r = range(i, m);
              return (
                <li
                  key={m.startS}
                  className="flex flex-wrap items-center gap-2 rounded-md border border-[var(--border)] bg-[var(--panel)] p-2 text-sm"
                >
                  <span className="font-medium">#{i + 1}</span>
                  <input
                    type="number"
                    className={`${field} w-24`}
                    value={Math.round(r.s)}
                    onChange={(e) =>
                      setEdits((x) => ({ ...x, [i]: { ...r, s: Number(e.target.value) } }))
                    }
                  />
                  <span>→</span>
                  <input
                    type="number"
                    className={`${field} w-24`}
                    value={Math.round(r.e)}
                    onChange={(e) =>
                      setEdits((x) => ({ ...x, [i]: { ...r, e: Number(e.target.value) } }))
                    }
                  />
                  <span className="text-xs text-[var(--muted)]">
                    {fmtDuration(r.s)}–{fmtDuration(r.e)} ·{" "}
                    {t("moments.score", {
                      score: (m.score ?? 0).toLocaleString(i18n.language, {
                        maximumFractionDigits: 1,
                      }),
                    })}
                  </span>
                  <button
                    className={btn}
                    disabled={!file || cut.isPending || r.e <= r.s}
                    onClick={() => cut.mutate(r)}
                  >
                    {t("moments.cut")}
                  </button>
                </li>
              );
            })}
          </ul>
          {cut.isPending && <p className="text-xs text-[var(--muted)]">{t("moments.cutting")}</p>}
          {cut.isError && <p className="text-xs text-red-500">{errorMessage(cut.error, t)}</p>}
        </section>
      )}
    </div>
  );
}
