import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { useParams } from "react-router";
import { AccountHeader } from "../../components/AccountHeader";
import { ContentTable } from "../../components/ContentTable";
import { MetricChart } from "../../components/MetricChart";
import { StatCard, changeTone } from "../../components/StatCard";
import { commands } from "../../lib/bindings";
import { fmtNumber, fmtSigned } from "../../lib/format";
import { errorMessage } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";
import { rangeStart } from "../../lib/range";
import { useUi } from "../../stores/ui";

/** Totals of the contents published since `from` (null = all time). */
export function contentTotals(
  rows: {
    publishedAt?: string | null;
    views?: number | null;
    likes?: number | null;
    comments?: number | null;
  }[],
  from: string | null,
) {
  const inRange = rows.filter((r) => from === null || (r.publishedAt ?? "") >= from);
  const sum = (k: "views" | "likes" | "comments") =>
    inRange.some((r) => r[k] != null) ? inRange.reduce((s, r) => s + (r[k] ?? 0), 0) : null;
  return {
    count: inRange.length,
    views: sum("views"),
    likes: sum("likes"),
    comments: sum("comments"),
  };
}

/**
 * Instagram and TikTok: neither API gives a daily history like YouTube Analytics, so the
 * figures come from the contents' own stats and the daily follower snapshots Omnyra takes.
 */
export function ShortFormView({ accountId }: { accountId: string }) {
  const { platform = "" } = useParams();
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
  const contents = useQuery({
    queryKey: queryKeys.contents(accountId),
    queryFn: () => commands.listContents(accountId),
  });
  const a = summary.data?.accounts.find((x) => x.accountId === accountId);
  const info = platforms.data?.find((p) => p.id === platform);

  if (summary.isPending || platforms.isPending)
    return <p className="text-[var(--muted)]">{t("common.loading")}</p>;
  if (summary.isError) return <p className="text-red-500">{errorMessage(summary.error, t)}</p>;
  if (!a || !info) return <p className="text-[var(--muted)]">{t("platform.notFound")}</p>;

  const totals = contentTotals(contents.data ?? [], from);
  const isInstagram = platform === "instagram";

  return (
    <div className="space-y-6">
      <AccountHeader a={a} info={info} />
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <StatCard
          label={t("shortform.followers")}
          value={fmtNumber(a.followers, lang, true)}
          sub={t("dashboard.card.change", { value: fmtSigned(a.followersChange, lang) })}
          subTone={changeTone(a.followersChange)}
        />
        <StatCard label={t("dashboard.card.published")} value={fmtNumber(totals.count, lang)} />
        <StatCard label={t("shortform.views")} value={fmtNumber(totals.views, lang, true)} />
        <StatCard
          label={t("shortform.likesComments")}
          value={`${fmtNumber(totals.likes, lang, true)} / ${fmtNumber(totals.comments, lang, true)}`}
        />
      </div>
      <p className="text-xs text-[var(--muted)]">
        {t(isInstagram ? "shortform.noteInstagram" : "shortform.noteTiktok")}
      </p>

      <section className="space-y-2 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-4">
        <h2 className="text-sm font-medium">{t("shortform.followers")}</h2>
        <MetricChart accountId={accountId} metric="followers" range={range} />
      </section>
      {isInstagram && (
        <section className="space-y-2 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-4">
          <h2 className="text-sm font-medium">{t("shortform.reach")}</h2>
          <MetricChart accountId={accountId} metric="reach" range={range} />
        </section>
      )}

      <section className="space-y-2">
        <h2 className="font-medium">{t("contents.title")}</h2>
        {contents.isPending ? (
          <p className="text-sm text-[var(--muted)]">{t("common.loading")}</p>
        ) : contents.isError ? (
          <p className="text-sm text-red-500">{errorMessage(contents.error, t)}</p>
        ) : (
          <ContentTable
            rows={contents.data}
            from={from}
            kinds={isInstagram ? ["all", "reel", "post"] : ["all"]}
          />
        )}
      </section>
    </div>
  );
}
