import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Fragment, useState } from "react";
import { useTranslation } from "react-i18next";
import { SeriesChart } from "../../components/SeriesChart";
import { commands, type PublicVideo, type TrackedSummary } from "../../lib/bindings";
import { fmtDate, fmtNumber, fmtSigned } from "../../lib/format";
import { errorMessage } from "../../lib/ipc";

const field = "rounded-md border border-[var(--border)] bg-[var(--panel)] px-2 py-1 text-sm";
const btn = "rounded-md border border-[var(--border)] px-2 py-1 text-xs disabled:opacity-50";
const primary = "rounded-md bg-[var(--accent)] px-3 py-1 text-sm text-white disabled:opacity-50";

function VideoList({ videos }: { videos: PublicVideo[] }) {
  const { i18n } = useTranslation();
  return (
    <ul className="space-y-1 text-sm">
      {videos.map((v) => (
        <li key={v.externalId} className="flex justify-between gap-2">
          <button
            className="truncate text-left underline"
            onClick={() => v.url && void openUrl(v.url)}
          >
            {v.title ?? v.externalId}
            {v.channelTitle && (
              <span className="text-xs text-[var(--muted)]"> · {v.channelTitle}</span>
            )}
          </button>
          <span className="shrink-0 text-xs text-[var(--muted)]">
            {fmtNumber(v.views, i18n.language, true)} ·{" "}
            {v.publishedAt ? fmtDate(v.publishedAt.slice(0, 10), i18n.language) : ""}
          </span>
        </li>
      ))}
    </ul>
  );
}

function Detail({ t: row }: { t: TrackedSummary }) {
  const series = useQuery({
    queryKey: ["trackedFollowers", row.id],
    queryFn: () => commands.trackedFollowers(row.id),
  });
  const top = useQuery({
    queryKey: ["trackedTop", row.id],
    queryFn: () => commands.trackedTop(row.id),
  });
  return (
    <div className="grid gap-3 lg:grid-cols-2">
      <SeriesChart
        data={series.data?.map((p) => ({ day: p.day, metric: "followers", value: p.value }))}
        metric="followers"
        loading={series.isPending}
      />
      <VideoList videos={top.data ?? []} />
    </div>
  );
}

/** Followed public channels (YouTube), comparisons and keyword trends. */
export function CompetitorsPage() {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const lang = i18n.language;
  const [query, setQuery] = useState("");
  const [keyword, setKeyword] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const tracked = useQuery({ queryKey: ["tracked"], queryFn: () => commands.listTracked() });
  const trends = useQuery({ queryKey: ["trends"], queryFn: () => commands.listTrends() });
  const inv = () => {
    void qc.invalidateQueries({ queryKey: ["tracked"] });
    void qc.invalidateQueries({ queryKey: ["trackedFollowers"] });
    void qc.invalidateQueries({ queryKey: ["trackedTop"] });
    void qc.invalidateQueries({ queryKey: ["quota"] });
  };
  const add = useMutation({
    mutationFn: () => commands.addTracked(query),
    onSuccess: () => {
      setQuery("");
      inv();
    },
  });
  const refresh = useMutation({ mutationFn: () => commands.refreshTracked(), onSettled: inv });
  const remove = useMutation({
    mutationFn: (id: string) => commands.removeTracked(id),
    onSettled: inv,
  });
  const invT = () => {
    void qc.invalidateQueries({ queryKey: ["trends"] });
    void qc.invalidateQueries({ queryKey: ["quota"] });
  };
  const addK = useMutation({
    mutationFn: () => commands.addTrendKeyword(keyword),
    onSuccess: () => {
      setKeyword("");
      invT();
    },
  });
  const delK = useMutation({
    mutationFn: (id: string) => commands.removeTrendKeyword(id),
    onSettled: invT,
  });
  const refreshK = useMutation({ mutationFn: () => commands.refreshTrends(), onSettled: invT });
  const rows = [...(tracked.data ?? [])].sort((a, b) => (b.followers ?? 0) - (a.followers ?? 0));

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold">{t("nav.competitors")}</h1>
      <p className="text-xs text-[var(--muted)]">{t("competitors.intro")}</p>

      <section className="space-y-2">
        <div className="flex flex-wrap items-center gap-2">
          <input
            className={`${field} w-80`}
            placeholder={t("competitors.placeholder")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && query.trim() && add.mutate()}
          />
          <button
            className={primary}
            disabled={!query.trim() || add.isPending}
            onClick={() => add.mutate()}
          >
            {t("competitors.add")}
          </button>
          <button className={btn} disabled={refresh.isPending} onClick={() => refresh.mutate()}>
            {refresh.isPending ? t("community.syncing") : t("competitors.refresh")}
          </button>
        </div>
        {(add.isError || refresh.isError) && (
          <p className="text-sm text-red-500">{errorMessage(add.error ?? refresh.error, t)}</p>
        )}
        {!rows.length ? (
          <p className="text-sm text-[var(--muted)]">{t("competitors.empty")}</p>
        ) : (
          <table className="w-full text-sm">
            <thead className="text-left text-xs text-[var(--muted)]">
              <tr>
                <th className="py-1">{t("competitors.col.channel")}</th>
                <th>{t("competitors.col.followers")}</th>
                <th>{t("competitors.col.change")}</th>
                <th>{t("competitors.col.rhythm")}</th>
                <th>{t("competitors.col.median")}</th>
                <th>{t("competitors.col.videos")}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <Fragment key={r.id}>
                  <tr className={`border-t border-[var(--border)] ${r.own ? "font-medium" : ""}`}>
                    <td className="py-1.5">
                      {r.displayName ?? r.handle}
                      {r.own && (
                        <span className="ml-1 text-xs text-[var(--accent)]">
                          ({t("competitors.you")})
                        </span>
                      )}
                      {r.handle && (
                        <span className="ml-1 text-xs text-[var(--muted)]">{r.handle}</span>
                      )}
                    </td>
                    <td>{fmtNumber(r.followers, lang, true)}</td>
                    <td>
                      {r.followersChange30d == null ? "—" : fmtSigned(r.followersChange30d, lang)}
                    </td>
                    <td>
                      {t("competitors.perWeek", {
                        n: (r.uploadsPerWeek ?? 0).toLocaleString(lang, {
                          maximumFractionDigits: 1,
                        }),
                      })}
                    </td>
                    <td>{fmtNumber(r.medianViews, lang, true)}</td>
                    <td>{fmtNumber(r.contentsTotal, lang)}</td>
                    <td className="text-right">
                      {!r.own && (
                        <div className="flex justify-end gap-1">
                          <button
                            className={btn}
                            onClick={() => setOpen(open === r.id ? null : r.id)}
                          >
                            {t("competitors.details")}
                          </button>
                          <button className={btn} onClick={() => remove.mutate(r.id)}>
                            ✕
                          </button>
                        </div>
                      )}
                    </td>
                  </tr>
                  {open === r.id && (
                    <tr>
                      <td colSpan={7} className="py-2">
                        <Detail t={r} />
                      </td>
                    </tr>
                  )}
                </Fragment>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="space-y-2">
        <h2 className="font-medium">{t("competitors.trends")}</h2>
        <p className="text-xs text-[var(--muted)]">{t("competitors.trendsHint")}</p>
        <div className="flex flex-wrap items-center gap-2">
          <input
            className={`${field} w-64`}
            placeholder={t("competitors.keyword")}
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && keyword.trim() && addK.mutate()}
          />
          <button className={btn} disabled={!keyword.trim()} onClick={() => addK.mutate()}>
            {t("competitors.addKeyword")}
          </button>
          <button
            className={btn}
            disabled={refreshK.isPending || !trends.data?.length}
            onClick={() => refreshK.mutate()}
          >
            {refreshK.isPending ? t("community.syncing") : t("competitors.searchNow")}
          </button>
        </div>
        {(addK.isError || refreshK.isError) && (
          <p className="text-sm text-red-500">{errorMessage(addK.error ?? refreshK.error, t)}</p>
        )}
        <div className="grid gap-3 lg:grid-cols-2">
          {trends.data?.map((k) => (
            <article
              key={k.id}
              className="space-y-2 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-3"
            >
              <header className="flex items-center justify-between">
                <h3 className="font-medium">{k.keyword}</h3>
                <div className="flex items-center gap-2 text-xs text-[var(--muted)]">
                  {k.lastDay ? fmtDate(k.lastDay, lang) : t("competitors.notSearched")}
                  <button className={btn} onClick={() => delK.mutate(k.id)}>
                    ✕
                  </button>
                </div>
              </header>
              {k.history.length > 1 && (
                <SeriesChart
                  data={k.history.map((h) => ({ day: h.day, metric: "views", value: h.value }))}
                  metric="views"
                />
              )}
              <VideoList videos={k.videos.slice(0, 8)} />
            </article>
          ))}
        </div>
      </section>
    </div>
  );
}
