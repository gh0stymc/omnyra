import { Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useTranslation } from "react-i18next";
import type { MetricPoint } from "../lib/bindings";
import { fmtDate } from "../lib/format";

/** Line chart of a daily series. Watch time (seconds) is shown in hours. */
export function SeriesChart({
  data,
  metric,
  loading,
}: {
  data: MetricPoint[] | undefined;
  metric: string;
  loading?: boolean;
}) {
  const { t, i18n } = useTranslation();
  const scale = metric === "watch_time_s" ? 1 / 3600 : 1;
  const points = data?.map((p) => ({ day: p.day, value: (p.value ?? 0) * scale }));
  const fmt = (v: number) => v.toLocaleString(i18n.language, { maximumFractionDigits: 1 });

  if (loading) return <p className="h-56 text-sm text-[var(--muted)]">{t("common.loading")}</p>;
  if (!points?.length) {
    return <p className="text-sm text-[var(--muted)]">{t("dashboard.noData")}</p>;
  }
  return (
    <div className="h-56">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={points}>
          <XAxis
            dataKey="day"
            tick={{ fontSize: 11 }}
            minTickGap={32}
            tickFormatter={(d: string) => fmtDate(d, i18n.language)}
          />
          <YAxis tick={{ fontSize: 11 }} domain={["auto", "auto"]} tickFormatter={fmt} width={56} />
          <Tooltip
            labelFormatter={(d) => fmtDate(String(d), i18n.language)}
            formatter={(v) => [fmt(Number(v)), t(`dashboard.metric.${metric}`)]}
            contentStyle={{ background: "var(--panel)", border: "1px solid var(--border)" }}
          />
          <Line
            type="monotone"
            dataKey="value"
            stroke="var(--accent)"
            strokeWidth={2}
            dot={false}
          />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
