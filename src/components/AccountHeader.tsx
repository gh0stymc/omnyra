import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import type { AccountSummary, PlatformInfo } from "../lib/bindings";
import { RangePicker } from "./RangePicker";
import { SyncButton } from "./SyncButton";

/** Name, platform, last sync and the account actions (period, upload, sync). */
export function AccountHeader({
  a,
  info,
  badge,
}: {
  a: AccountSummary;
  info: PlatformInfo;
  badge?: React.ReactNode;
}) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const accountId = a.accountId;
  const canUpload = info.capabilities.upload !== "none";
  return (
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div>
        <h1 className="flex items-center gap-2 text-2xl font-semibold">
          {a.displayName}
          {badge}
        </h1>
        <p className="text-xs text-[var(--muted)]">
          {info.displayName}
          {a.handle && ` · ${a.handle}`} ·{" "}
          {t("dashboard.lastSync", {
            when: a.lastSyncAt ? new Date(a.lastSyncAt).toLocaleString(lang) : t("dashboard.never"),
          })}
        </p>
        {a.status === "needs_reauth" && (
          <p className="text-sm text-amber-500">
            {t("platform.needsReauth")}{" "}
            <Link className="underline" to="/settings">
              {t("nav.settings")}
            </Link>
          </p>
        )}
      </div>
      <div className="flex items-center gap-2">
        <RangePicker />
        {canUpload && (
          <Link
            to={`/uploads?account=${accountId}`}
            className="rounded-md border border-[var(--border)] px-2 py-1 text-xs"
          >
            {t("dashboard.uploadVideo")}
          </Link>
        )}
        <SyncButton accountIds={a.status === "active" ? [accountId] : []} />
      </div>
    </div>
  );
}
