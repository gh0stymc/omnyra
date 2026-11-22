import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { RangePicker } from "../../components/RangePicker";
import { SeriesChart } from "../../components/SeriesChart";
import { StatCard, changeTone } from "../../components/StatCard";
import { SyncButton } from "../../components/SyncButton";
import { commands } from "../../lib/bindings";
import { fmtDate, fmtHours, fmtNumber, fmtSigned } from "../../lib/format";
import { errorMessage } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";
import { rangeStart } from "../../lib/range";
import { useUi } from "../../stores/ui";

const CHARTS = ["followers", "views", "watch_time_s"] as const;

export function DashboardPage() {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const lang = i18n.language;
  const range = useUi((s) => s.range);
  const from = rangeStart(range);
  const [metric, setMetric] = useState<string>(CHARTS[0]);

  const summary = useQuery({
    queryKey: queryKeys.summary(from),
    queryFn: () => commands.getSummary(from),
  });
  const series = useQuery({
    queryKey: queryKeys.combined(metric, from),
    queryFn: () => commands.getCombinedSeries(metric, from),
  });
  const top = useQuery({
    queryKey: queryKeys.top(from),
    queryFn: () => commands.getTopContents(from, 5),
  });
  const addMock = useMutation({
    mutationFn: () => commands.addMockAccount(),
    onSuccess: () => void qc.invalidateQueries(),
  });

  const accounts = summary.data?.accounts ?? [];
  const totals = summary.data?.totals;
  const btn =
    "rounded-md bg-[var(--accent)] px-3 py-1.5 text-sm font-medium text-white hover:opacity-90";
  const pill = (active: boolean) =>
    `rounded-[10px] px-3 py-1 text-xs font-medium ${active ? "bg-[var(--accent)] text-white" : "text-[var(--muted)] hover:text-[var(--text)]"}`;
  const dataUntil = accounts
    .map((a) => a.dataUntil)
    .filter(Boolean)
    .sort()
    .at(-1);

  if (summary.isPending) return <p className="text-[var(--muted)]">{t("common.loading")}</p>;
  if (summary.isError) return <p className="text-red-500">{errorMessage(summary.error, t)}</p>;

  if (accounts.length === 0) {
    return (
      <div className="space-y-3">
        <h1 className="text-2xl font-semibold">{t("dashboard.title")}</h1>
        <p className="text-[var(--muted)]">{t("dashboard.noAccounts")}</p>
        <div className="flex gap-2">
          <Link to="/settings" className={btn}>
            {t("dashboard.connectNetwork")}
          </Link>
          <button
            className="rounded-md border border-[var(--border)] px-3 py-1.5 text-sm"
            onClick={() => addMock.mutate()}
          >
            {t("dashboard.addMock")}
          </button>
        </div>
        {addMock.isError && <p className="text-red-500">{errorMessage(addMock.error, t)}</p>}
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-3xl font-bold">{t("dashboard.title")}</h1>
          <p className="mt-1 text-sm text-[var(--muted)]">{t("dashboard.subtitle")}</p>
        </div>
        <div className="flex items-center gap-3">
          <RangePicker />
          <SyncButton
            accountIds={accounts.filter((a) => a.status === "active").map((a) => a.accountId)}
            label={t("dashboard.syncAll")}
          />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <StatCard
          label={t("dashboard.card.followers")}
          value={fmtNumber(totals?.followers, lang, true)}
          sub={t("dashboard.card.change", { value: fmtSigned(totals?.followersChange, lang) })}
          subTone={changeTone(totals?.followersChange)}
        />
        <StatCard label={t("dashboard.card.views")} value={fmtNumber(totals?.views, lang, true)} />
        <StatCard
          label={t("dashboard.card.watchHours")}
          value={fmtHours(totals?.watchTimeS ?? 0, lang)}
        />
        <StatCard
          label={t("dashboard.card.published")}
          value={fmtNumber(totals?.contentsPublished, lang)}
        />
      </div>
      {dataUntil && (
        <p className="text-xs text-[var(--muted)]">
          {t("dashboard.dataUntil", { day: fmtDate(dataUntil, lang) })}
        </p>
      )}

      <section className="space-y-4 rounded-2xl border border-[var(--border)] bg-[var(--panel)] p-5">
        <div
          className="inline-flex gap-0.5 rounded-xl border border-[var(--border)] p-0.5"
          role="tablist"
        >
          {CHARTS.map((m) => (
            <button
              key={m}
              role="tab"
              aria-selected={m === metric}
              className={pill(m === metric)}
              onClick={() => setMetric(m)}
            >
              {t(`dashboard.metric.${m}`)}
            </button>
          ))}
        </div>
        <SeriesChart data={series.data} metric={metric} loading={series.isPending} />
        {accounts.length > 1 && (
          <p className="text-xs text-[var(--muted)]">{t("dashboard.combinedNote")}</p>
        )}
      </section>

      <div className="grid gap-6 lg:grid-cols-2">
        <section className="space-y-3 rounded-2xl border border-[var(--border)] bg-[var(--panel)] p-5">
          <h2 className="font-semibold">{t("dashboard.topContents")}</h2>
          {top.data?.length ? (
            <ol className="space-y-1 text-sm">
              {top.data.map((c, i) => (
                <li
                  key={c.content.id}
                  className="flex items-center justify-between gap-3 rounded-xl px-2 py-2 hover:bg-[var(--accent-soft)]"
                >
                  <span className="flex min-w-0 items-center gap-3">
                    <span
                      className={`grid size-7 shrink-0 place-items-center rounded-lg text-xs font-bold ${i === 0 ? "bg-[var(--accent)] text-white" : "bg-[var(--accent-soft)] text-[var(--accent)]"}`}
                    >
                      {i + 1}
                    </span>
                    <span className="truncate">{c.content.title || t("upload.untitled")}</span>
                    {accounts.length > 1 && (
                      <span className="shrink-0 text-xs text-[var(--muted)]">{c.accountName}</span>
                    )}
                  </span>
                  <span className="shrink-0 font-semibold tabular-nums">
                    {fmtNumber(c.content.views, lang, true)}
                  </span>
                </li>
              ))}
            </ol>
          ) : (
            <p className="text-sm text-[var(--muted)]">{t("dashboard.noTopContents")}</p>
          )}
        </section>

        <section className="space-y-3 rounded-2xl border border-[var(--border)] bg-[var(--panel)] p-5">
          <h2 className="font-semibold">{t("nav.accounts")}</h2>
          <ul className="space-y-1 text-sm">
            {accounts.map((a) => (
              <li
                key={a.accountId}
                className="flex items-center justify-between gap-2 rounded-xl px-2 py-2 hover:bg-[var(--accent-soft)]"
              >
                <Link
                  to={`/platform/${a.platform}/${a.accountId}`}
                  className="flex min-w-0 items-center gap-2.5 font-medium"
                >
                  <span className="grid size-7 shrink-0 place-items-center rounded-lg bg-[var(--accent-soft)] text-xs font-bold uppercase text-[var(--accent)]">
                    {a.platform.slice(0, 2)}
                  </span>
                  <span className="truncate">{a.displayName}</span>
                </Link>
                <span className="shrink-0 text-xs text-[var(--muted)]">
                  {a.status === "needs_reauth"
                    ? t("accounts.status.needs_reauth", { defaultValue: "!" })
                    : t("dashboard.lastSync", {
                        when: a.lastSyncAt
                          ? new Date(a.lastSyncAt).toLocaleString(lang)
                          : t("dashboard.never"),
                      })}
                </span>
              </li>
            ))}
          </ul>
        </section>
      </div>
    </div>
  );
}
