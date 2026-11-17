import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useParams } from "react-router";
import { AccountHeader } from "../../components/AccountHeader";
import { BestTimeHeatmap } from "../../components/BestTimeHeatmap";
import { ContentTable } from "../../components/ContentTable";
import { MetricChart } from "../../components/MetricChart";
import { StatCard, changeTone } from "../../components/StatCard";
import { commands } from "../../lib/bindings";
import { fmtDate, fmtDuration, fmtHours, fmtNumber, fmtSigned } from "../../lib/format";
import { errorMessage } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";
import { rangeStart } from "../../lib/range";
import { platformUI } from "../../platforms/registry";
import { useUi } from "../../stores/ui";

const CHARTS = ["followers", "views", "watch_time_s"] as const;

/** One account: figures for the period, charts, best time and its contents. */
export function PlatformViewPage() {
  const { platform = "", accountId = "" } = useParams();
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const range = useUi((s) => s.range);
  const from = rangeStart(range);
  const [metric, setMetric] = useState<string>(CHARTS[0]);

  const platforms = useQuery({
    queryKey: queryKeys.platforms,
    queryFn: () => commands.listPlatforms(),
  });
  const summary = useQuery({
    queryKey: queryKeys.summary(from),
    queryFn: () => commands.getSummary(from),
  });
  const contents = useQuery({
    queryKey: queryKeys.contents(accountId),
    queryFn: () => commands.listContents(accountId),
  });
  const info = platforms.data?.find((p) => p.id === platform);
  const a = summary.data?.accounts.find((x) => x.accountId === accountId);
  const Custom = platformUI(platform)?.AnalyticsView;
  const pill = (active: boolean) =>
    `rounded-md px-2 py-1 text-xs ${active ? "bg-[var(--accent)] text-white" : "border border-[var(--border)]"}`;

  if (summary.isPending || platforms.isPending)
    return <p className="text-[var(--muted)]">{t("common.loading")}</p>;
  if (summary.isError) return <p className="text-red-500">{errorMessage(summary.error, t)}</p>;
  if (!a || !info) return <p className="text-[var(--muted)]">{t("platform.notFound")}</p>;
  if (Custom) return <Custom accountId={accountId} />;

  const net = (a.subscribersGained ?? 0) - (a.subscribersLost ?? 0);

  return (
    <div className="space-y-6">
      <AccountHeader a={a} info={info} />

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <StatCard
          label={t("dashboard.card.followers")}
          value={fmtNumber(a.followers, lang, true)}
          sub={t("dashboard.card.change", { value: fmtSigned(a.followersChange, lang) })}
          subTone={changeTone(a.followersChange)}
        />
        <StatCard label={t("dashboard.card.views")} value={fmtNumber(a.views, lang, true)} />
        <StatCard
          label={t("dashboard.card.watchHours")}
          value={fmtHours(a.watchTimeS ?? 0, lang)}
          sub={t("platform.avgDuration", { value: fmtDuration(a.avgViewDurationS) })}
        />
        <StatCard
          label={t("platform.netSubscribers")}
          value={fmtSigned(net, lang)}
          sub={t("platform.gainedLost", {
            gained: fmtNumber(a.subscribersGained, lang),
            lost: fmtNumber(a.subscribersLost, lang),
          })}
          subTone={changeTone(net)}
        />
        <StatCard label={t("platform.likes")} value={fmtNumber(a.likes, lang, true)} />
        <StatCard label={t("platform.comments")} value={fmtNumber(a.comments, lang, true)} />
        <StatCard label={t("platform.shares")} value={fmtNumber(a.shares, lang, true)} />
        <StatCard
          label={t("dashboard.card.published")}
          value={fmtNumber(a.contentsPublished, lang)}
        />
      </div>
      {a.dataUntil && (
        <p className="text-xs text-[var(--muted)]">
          {t("dashboard.dataUntil", { day: fmtDate(a.dataUntil, lang) })}
        </p>
      )}

      <section className="space-y-3 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-4">
        <div className="flex gap-1" role="tablist">
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
        <MetricChart accountId={accountId} metric={metric} range={range} />
      </section>

      {info.capabilities.analytics && (
        <section className="rounded-lg border border-[var(--border)] bg-[var(--panel)] p-4">
          <BestTimeHeatmap accountId={accountId} />
        </section>
      )}

      <section className="space-y-2">
        <h2 className="font-medium">{t("contents.title")}</h2>
        {contents.isPending ? (
          <p className="text-sm text-[var(--muted)]">{t("common.loading")}</p>
        ) : contents.isError ? (
          <p className="text-sm text-red-500">{errorMessage(contents.error, t)}</p>
        ) : (
          <ContentTable rows={contents.data} from={from} />
        )}
      </section>
    </div>
  );
}
