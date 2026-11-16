import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useTranslation } from "react-i18next";
import { commands } from "../lib/bindings";
import { errorMessage } from "../lib/ipc";
import { queryKeys } from "../lib/queryKeys";

export function MetricChart({
  accountId,
  metric = "followers",
}: {
  accountId: string;
  metric?: string;
}) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const series = useQuery({
    queryKey: queryKeys.series(accountId, metric),
    queryFn: () => commands.getAccountSeries(accountId, metric),
  });
  const sync = useMutation({
    mutationFn: () => commands.syncAccount(accountId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["series", accountId] }),
  });

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-sm text-[var(--muted)]">
          {t(`dashboard.${metric}`, { defaultValue: metric })}
        </span>
        <button
          className="rounded-md border border-[var(--border)] px-2 py-1 text-xs"
          onClick={() => sync.mutate()}
          disabled={sync.isPending}
        >
          {t("dashboard.sync")}
        </button>
      </div>
      {sync.isError && <p className="text-sm text-red-500">{errorMessage(sync.error, t)}</p>}
      {series.data && series.data.length === 0 ? (
        <p className="text-sm text-[var(--muted)]">{t("dashboard.noData")}</p>
      ) : (
        <div className="h-56">
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={series.data}>
              <XAxis dataKey="day" tick={{ fontSize: 11 }} />
              <YAxis tick={{ fontSize: 11 }} domain={["auto", "auto"]} />
              <Tooltip />
              <Line type="monotone" dataKey="value" stroke="var(--accent)" dot={false} />
            </LineChart>
          </ResponsiveContainer>
        </div>
      )}
    </div>
  );
}
