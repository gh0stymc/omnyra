import { useQuery } from "@tanstack/react-query";
import { Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useTranslation } from "react-i18next";
import { commands } from "../lib/bindings";
import { queryKeys } from "../lib/queryKeys";

export const RANGES = ["30d", "1y", "5y", "all"] as const;
export type Range = (typeof RANGES)[number];

/** First day (`YYYY-MM-DD`, UTC) shown for `range`, or null for everything. */
export function rangeStart(range: Range, now = new Date()): string | null {
  const d = new Date(now);
  if (range === "30d") d.setUTCDate(d.getUTCDate() - 29);
  else if (range === "1y") d.setUTCFullYear(d.getUTCFullYear() - 1);
  else if (range === "5y") d.setUTCFullYear(d.getUTCFullYear() - 5);
  else return null;
  return d.toISOString().slice(0, 10);
}

/** Daily series of one account metric (followers, views, watch_time_s...). */
export function MetricChart({
  accountId,
  metric,
  range = "all",
}: {
  accountId: string;
  metric: string;
  range?: Range;
}) {
  const { t, i18n } = useTranslation();
  const series = useQuery({
    queryKey: queryKeys.series(accountId, metric),
    queryFn: () => commands.getAccountSeries(accountId, metric),
  });
  // Watch time is stored in seconds; hours read better.
  const scale = metric === "watch_time_s" ? 1 / 3600 : 1;
  const from = rangeStart(range);
  const data = series.data
    ?.filter((p) => from === null || p.day >= from)
    .map((p) => ({ day: p.day, value: (p.value ?? 0) * scale }));
  const fmt = (v: number) => v.toLocaleString(i18n.language, { maximumFractionDigits: 1 });

  if (data && data.length === 0) {
    return <p className="text-sm text-[var(--muted)]">{t("dashboard.noData")}</p>;
  }
  return (
    <div className="h-56">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={data}>
          <XAxis dataKey="day" tick={{ fontSize: 11 }} minTickGap={24} />
          <YAxis tick={{ fontSize: 11 }} domain={["auto", "auto"]} tickFormatter={fmt} width={56} />
          <Tooltip
            formatter={(v) => [fmt(Number(v)), t(`dashboard.metric.${metric}`)]}
            contentStyle={{ background: "var(--panel)", border: "1px solid var(--border)" }}
          />
          <Line type="monotone" dataKey="value" stroke="var(--accent)" dot={false} />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
