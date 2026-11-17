import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { AccountHeader } from "../../components/AccountHeader";
import { ContentTable } from "../../components/ContentTable";
import { MetricChart } from "../../components/MetricChart";
import { StatCard, changeTone } from "../../components/StatCard";
import { commands } from "../../lib/bindings";
import { fmtDate, fmtDuration, fmtNumber, fmtSigned } from "../../lib/format";
import { errorMessage } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";
import { rangeStart } from "../../lib/range";
import { useUi } from "../../stores/ui";

/** Twitch account: followers, streams (VODs + what Omnyra saw live), VODs and clips. */
export function TwitchView({ accountId }: { accountId: string }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const range = useUi((s) => s.range);
  const from = rangeStart(range);
  const platforms = useQuery({
    queryKey: queryKeys.platforms,
    queryFn: () => commands.listPlatforms(),
  });
  const summary = useQuery({
    queryKey: queryKeys.summary(from),
    queryFn: () => commands.getSummary(from),
  });
  const live = useQuery({
    queryKey: queryKeys.live(accountId, from),
    queryFn: () => commands.getLiveOverview(accountId, from),
    // Samples arrive every 2 minutes while the app runs.
    refetchInterval: 60_000,
  });
  const contents = useQuery({
    queryKey: queryKeys.contents(accountId),
    queryFn: () => commands.listContents(accountId),
  });
  const a = summary.data?.accounts.find((x) => x.accountId === accountId);
  const info = platforms.data?.find((p) => p.id === "twitch");

  if (summary.isPending || platforms.isPending)
    return <p className="text-[var(--muted)]">{t("common.loading")}</p>;
  if (summary.isError) return <p className="text-red-500">{errorMessage(summary.error, t)}</p>;
  if (!a || !info) return <p className="text-[var(--muted)]">{t("platform.notFound")}</p>;

  const st = live.data?.stats;
  const sessions = live.data?.sessions ?? [];
  const badge = st?.liveNow ? (
    <span className="rounded-full bg-red-600 px-2 py-0.5 text-xs font-medium text-white">
      {t("twitch.liveNow")}
    </span>
  ) : null;

  return (
    <div className="space-y-6">
      <AccountHeader a={a} info={info} badge={badge} />

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <StatCard
          label={t("twitch.followers")}
          value={fmtNumber(a.followers, lang, true)}
          sub={t("dashboard.card.change", { value: fmtSigned(a.followersChange, lang) })}
          subTone={changeTone(a.followersChange)}
        />
        <StatCard label={t("twitch.streams")} value={fmtNumber(st?.streams, lang)} />
        <StatCard
          label={t("twitch.hoursStreamed")}
          value={(st?.hoursStreamed ?? 0).toLocaleString(lang, { maximumFractionDigits: 1 })}
        />
        <StatCard
          label={t("twitch.viewers")}
          value={st?.avgViewers != null ? fmtNumber(st.avgViewers, lang) : "–"}
          sub={
            st?.peakViewers != null
              ? t("twitch.peak", { value: fmtNumber(st.peakViewers, lang) })
              : undefined
          }
        />
      </div>
      <p className="text-xs text-[var(--muted)]">
        {t("twitch.observedNote", { observed: st?.observed ?? 0, streams: st?.streams ?? 0 })}
      </p>

      <section className="space-y-2 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-4">
        <h2 className="text-sm font-medium">{t("twitch.followers")}</h2>
        <MetricChart accountId={accountId} metric="followers" range={range} />
        <p className="text-xs text-[var(--muted)]">{t("twitch.followersNote")}</p>
      </section>

      <section className="space-y-2">
        <h2 className="font-medium">{t("twitch.sessionsTitle")}</h2>
        {sessions.length === 0 ? (
          <p className="text-sm text-[var(--muted)]">{t("twitch.noSessions")}</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="text-left text-xs text-[var(--muted)]">
                <tr>
                  <th className="py-1 pr-2 font-normal">{t("twitch.col.date")}</th>
                  <th className="py-1 pr-2 font-normal">{t("twitch.col.title")}</th>
                  <th className="py-1 pr-2 text-right font-normal">{t("twitch.col.duration")}</th>
                  <th className="py-1 pr-2 text-right font-normal">{t("twitch.col.avg")}</th>
                  <th className="py-1 text-right font-normal">{t("twitch.col.peak")}</th>
                </tr>
              </thead>
              <tbody>
                {sessions.map((s) => (
                  <tr key={s.id} className="border-t border-[var(--border)]">
                    <td className="py-1 pr-2 text-xs">{fmtDate(s.startedAt, lang)}</td>
                    <td className="max-w-xs truncate py-1 pr-2">
                      {s.title ?? "–"}
                      {s.category && (
                        <span className="text-xs text-[var(--muted)]"> · {s.category}</span>
                      )}
                      {!s.endedAt && (
                        <span className="text-xs text-red-500"> · {t("twitch.liveNow")}</span>
                      )}
                    </td>
                    <td className="py-1 pr-2 text-right tabular-nums">
                      {fmtDuration(s.durationS)}
                    </td>
                    <td className="py-1 pr-2 text-right tabular-nums">
                      {fmtNumber(s.avgViewers, lang)}
                    </td>
                    <td className="py-1 text-right tabular-nums">
                      {fmtNumber(s.peakViewers, lang)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="space-y-2">
        <h2 className="font-medium">{t("twitch.contentsTitle")}</h2>
        {contents.isPending ? (
          <p className="text-sm text-[var(--muted)]">{t("common.loading")}</p>
        ) : contents.isError ? (
          <p className="text-sm text-red-500">{errorMessage(contents.error, t)}</p>
        ) : (
          <ContentTable rows={contents.data} from={from} kinds={["all", "vod", "clip", "video"]} />
        )}
      </section>
    </div>
  );
}
