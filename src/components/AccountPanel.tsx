import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Account } from "../lib/bindings";
import { errorMessage } from "../lib/ipc";
import { queryKeys } from "../lib/queryKeys";
import { MetricChart, RANGES, type Range } from "./MetricChart";

export const CHART_METRICS = ["followers", "views", "watch_time_s"] as const;

/** One account on the dashboard: sync controls, metric picker, chart and content counts. */
export function AccountPanel({ account }: { account: Account }) {
  const { t, i18n } = useTranslation();
  const [metric, setMetric] = useState<string>(CHART_METRICS[0]);
  const [range, setRange] = useState<Range>("30d");
  const contents = useQuery({
    queryKey: queryKeys.contents(account.id),
    queryFn: () => commands.listContents(account.id),
  });
  // Results arrive through the SyncStatus event (see app/providers.tsx).
  const sync = useMutation({ mutationFn: () => commands.syncAccount(account.id) });

  const counts = { video: 0, short: 0, stream: 0 };
  for (const c of contents.data ?? []) {
    if (c.kind in counts) counts[c.kind as keyof typeof counts]++;
  }
  const lastSync = account.lastSyncAt
    ? new Date(account.lastSyncAt).toLocaleString(i18n.language)
    : t("dashboard.never");
  const tab = (active: boolean) =>
    `rounded-md px-2 py-1 text-xs ${active ? "bg-[var(--accent)] text-white" : "border border-[var(--border)]"}`;

  return (
    <section className="space-y-3 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h2 className="font-medium">{account.displayName}</h2>
          <p className="text-xs text-[var(--muted)]">
            {t("dashboard.lastSync", { when: lastSync })}
            {contents.data && contents.data.length > 0 && (
              <> · {t("dashboard.contentCounts", counts)}</>
            )}
          </p>
        </div>
        <button
          className="rounded-md border border-[var(--border)] px-2 py-1 text-xs disabled:opacity-50"
          onClick={() => sync.mutate()}
          disabled={sync.isPending}
        >
          {sync.isPending ? t("dashboard.syncing") : t("dashboard.sync")}
        </button>
      </div>
      {sync.isError && <p className="text-sm text-red-500">{errorMessage(sync.error, t)}</p>}
      {sync.data?.quotaLimited && (
        <p className="text-sm text-amber-500">{t("dashboard.quotaLimited")}</p>
      )}
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex gap-1" role="tablist" aria-label={t("dashboard.metricLabel")}>
          {CHART_METRICS.map((m) => (
            <button
              key={m}
              role="tab"
              aria-selected={m === metric}
              className={tab(m === metric)}
              onClick={() => setMetric(m)}
            >
              {t(`dashboard.metric.${m}`)}
            </button>
          ))}
        </div>
        <div className="flex gap-1" role="group" aria-label={t("dashboard.rangeLabel")}>
          {RANGES.map((r) => (
            <button
              key={r}
              aria-pressed={r === range}
              className={tab(r === range)}
              onClick={() => setRange(r)}
            >
              {t(`dashboard.range.${r}`)}
            </button>
          ))}
        </div>
      </div>
      <MetricChart accountId={account.id} metric={metric} range={range} />
    </section>
  );
}
