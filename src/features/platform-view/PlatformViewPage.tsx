import { useParams } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { commands } from "../../lib/bindings";
import { queryKeys } from "../../lib/queryKeys";
import { platformUI } from "../../platforms/registry";
import { MetricChart } from "../../components/MetricChart";

export function PlatformViewPage() {
  const { platform = "", accountId = "" } = useParams();
  const { t } = useTranslation();
  const platforms = useQuery({
    queryKey: queryKeys.platforms,
    queryFn: () => commands.listPlatforms(),
  });
  const info = platforms.data?.find((p) => p.id === platform);
  const Custom = platformUI(platform)?.AnalyticsView;

  if (!info) return null;
  const caps = info.capabilities;
  const flags = [
    caps.analytics && "analytics",
    caps.upload !== "none" && `upload: ${caps.upload}`,
    caps.schedule !== "none" && `schedule: ${caps.schedule}`,
    caps.comments !== "none" && `comments: ${caps.comments}`,
  ].filter(Boolean);

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">{t("platform.title", { name: info.displayName })}</h1>
      <p className="text-sm text-[var(--muted)]">
        {t("platform.capabilities")}: {flags.join(" · ")}
      </p>
      {Custom ? <Custom accountId={accountId} /> : <MetricChart accountId={accountId} />}
      {/* Without upload capability the UI shows "Prepare and download" instead of "Publish" (step 1d/3c). */}
    </div>
  );
}
