import { useQuery } from "@tanstack/react-query";
import { commands } from "../lib/bindings";
import { queryKeys } from "../lib/queryKeys";
import { rangeStart, type Range } from "../lib/range";
import { SeriesChart } from "./SeriesChart";

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
  const series = useQuery({
    queryKey: queryKeys.series(accountId, metric),
    queryFn: () => commands.getAccountSeries(accountId, metric),
  });
  const from = rangeStart(range);
  const data = series.data?.filter((p) => from === null || p.day >= from);
  return <SeriesChart data={data} metric={metric} loading={series.isPending} />;
}
