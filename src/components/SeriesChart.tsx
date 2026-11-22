import { useId } from "react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
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
  const gid = useId().replace(/:/g, "");
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
        <AreaChart data={points} margin={{ top: 8, right: 4, left: 0, bottom: 0 }}>
          <defs>
            <linearGradient id={`fill-${gid}`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--accent)" stopOpacity={0.35} />
              <stop offset="100%" stopColor="var(--accent)" stopOpacity={0} />
            </linearGradient>
            <linearGradient id={`stroke-${gid}`} x1="0" y1="0" x2="1" y2="0">
              <stop offset="0%" stopColor="var(--accent)" />
              <stop offset="100%" stopColor="var(--accent-2)" />
            </linearGradient>
          </defs>
          <CartesianGrid vertical={false} stroke="var(--border)" strokeDasharray="3 6" />
          <XAxis
            dataKey="day"
            tick={{ fontSize: 11, fill: "var(--muted)" }}
            axisLine={false}
            tickLine={false}
            minTickGap={32}
            tickFormatter={(d: string) => fmtDate(d, i18n.language)}
          />
          <YAxis
            tick={{ fontSize: 11, fill: "var(--muted)" }}
            axisLine={false}
            tickLine={false}
            domain={["auto", "auto"]}
            tickFormatter={fmt}
            width={56}
          />
          <Tooltip
            labelFormatter={(d) => fmtDate(String(d), i18n.language)}
            formatter={(v) => [fmt(Number(v)), t(`dashboard.metric.${metric}`)]}
            cursor={{ stroke: "var(--border-strong)", strokeDasharray: "4 4" }}
            contentStyle={{
              background: "var(--panel-solid)",
              border: "1px solid var(--border)",
              borderRadius: 12,
              boxShadow: "var(--shadow)",
              fontSize: 12,
            }}
            labelStyle={{ color: "var(--muted)" }}
            itemStyle={{ color: "var(--text)" }}
          />
          <Area
            type="monotone"
            dataKey="value"
            stroke={`url(#stroke-${gid})`}
            strokeWidth={2.5}
            fill={`url(#fill-${gid})`}
            dot={false}
            activeDot={{
              r: 4,
              fill: "var(--accent)",
              stroke: "var(--panel-solid)",
              strokeWidth: 2,
            }}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
