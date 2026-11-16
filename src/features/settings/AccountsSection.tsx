import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { commands, type Account, type PlatformInfo } from "../../lib/bindings";
import { errorMessage, isAppError } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";
import { scopeLabels } from "../../lib/scopes";
import { platformUI } from "../../platforms/registry";

const btn = "rounded-md px-3 py-1.5 text-sm font-medium disabled:opacity-50";
const primary = `${btn} bg-[var(--accent)] text-white hover:opacity-90`;
const secondary = `${btn} border border-[var(--border)] hover:bg-[var(--border)]`;
const danger = `${btn} bg-red-600 text-white hover:opacity-90`;

const statusStyle: Record<Account["status"], string> = {
  active: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400",
  needs_reauth: "bg-amber-500/15 text-amber-600 dark:text-amber-400",
  disconnected: "bg-zinc-500/15 text-[var(--muted)]",
};

/** Settings → "Connected accounts": connect, reconnect and disconnect, with granted scopes. */
export function AccountsSection() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const platforms = useQuery({
    queryKey: queryKeys.platforms,
    queryFn: () => commands.listPlatforms(),
  });
  const accounts = useQuery({
    queryKey: queryKeys.accounts,
    queryFn: () => commands.listAccounts(),
  });
  const [connecting, setConnecting] = useState<string | null>(null);
  const [notice, setNotice] = useState<{ platform: string } | null>(null);

  const connect = useMutation({
    mutationFn: (platform: string) => {
      setConnecting(platform);
      setNotice(null);
      return commands.connectAccount(platform);
    },
    onSettled: () => {
      setConnecting(null);
      void qc.invalidateQueries({ queryKey: queryKeys.accounts });
    },
  });

  const oauthPlatforms = platforms.data?.filter((p) => p.auth === "oauth") ?? [];
  const byId = (id: string) => platforms.data?.find((p) => p.id === id);
  const connectCancelled = isAppError(connect.error) && connect.error.code === "AUTH_CANCELLED";

  return (
    <section className="space-y-4" aria-labelledby="accounts-title">
      <h2 id="accounts-title" className="text-lg font-semibold">
        {t("accounts.title")}
      </h2>

      <div className="flex flex-wrap items-center gap-2">
        {oauthPlatforms.map((p) => (
          <button
            key={p.id}
            className={primary}
            disabled={connect.isPending || !p.authConfigured}
            title={p.authConfigured ? undefined : t("accounts.notConfigured")}
            onClick={() => connect.mutate(p.id)}
          >
            {t("accounts.connect", { name: p.displayName })}
          </button>
        ))}
      </div>
      {oauthPlatforms.some((p) => !p.authConfigured) && (
        <p className="text-sm text-[var(--muted)]">{t("accounts.notConfigured")}</p>
      )}

      {connect.isPending && connecting && (
        <div className="flex items-center gap-3 rounded-md border border-[var(--border)] p-3 text-sm">
          <span>{t("accounts.waitingBrowser", { name: byId(connecting)?.displayName })}</span>
          <button className={secondary} onClick={() => void commands.cancelConnect()}>
            {t("accounts.cancel")}
          </button>
        </div>
      )}
      {connect.isError && !connectCancelled && (
        <p role="alert" className="text-sm text-red-500">
          {errorMessage(connect.error, t)}
        </p>
      )}
      {notice && <NotRevokedNotice platform={notice.platform} onClose={() => setNotice(null)} />}

      {accounts.data?.length === 0 && (
        <p className="text-sm text-[var(--muted)]">{t("accounts.empty")}</p>
      )}
      <ul className="space-y-3">
        {accounts.data?.map((a) => (
          <AccountRow
            key={a.id}
            account={a}
            platform={byId(a.platform)}
            busy={connect.isPending}
            onReconnect={() => connect.mutate(a.platform)}
            onNotRevoked={() => setNotice({ platform: a.platform })}
          />
        ))}
      </ul>
    </section>
  );
}

function AccountRow({
  account: a,
  platform,
  busy,
  onReconnect,
  onNotRevoked,
}: {
  account: Account;
  platform?: PlatformInfo;
  busy: boolean;
  onReconnect: () => void;
  onNotRevoked: () => void;
}) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const [confirming, setConfirming] = useState(false);
  const disconnect = useMutation({
    mutationFn: () => commands.disconnectAccount(a.id),
    onSuccess: (out) => {
      setConfirming(false);
      if (!out.revoked) onNotRevoked();
    },
    onSettled: () => void qc.invalidateQueries({ queryKey: queryKeys.accounts }),
  });
  const canReconnect = platform?.auth === "oauth" && platform.authConfigured;
  const scopes = scopeLabels(a.grantedScopes, t);
  const since = new Date(a.connectedAt);

  return (
    <li className="rounded-lg border border-[var(--border)] bg-[var(--panel)] p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 space-y-1">
          <div className="flex items-center gap-2">
            <span
              className="size-2.5 shrink-0 rounded-full"
              style={{ background: platform?.brandColor ?? "#888" }}
            />
            <span className="font-medium">{a.displayName}</span>
            {a.handle && <span className="text-sm text-[var(--muted)]">{a.handle}</span>}
            <span className={`rounded-full px-2 py-0.5 text-xs ${statusStyle[a.status]}`}>
              {t(`accounts.status.${a.status}`)}
            </span>
          </div>
          <p className="text-xs text-[var(--muted)]">
            {platform?.displayName ?? a.platform} ·{" "}
            {t("accounts.connectedAt", {
              date: isNaN(since.getTime()) ? a.connectedAt : since.toLocaleString(i18n.language),
            })}
          </p>
          {a.status === "needs_reauth" && (
            <p className="text-sm text-amber-600 dark:text-amber-400">
              {t("accounts.needsReauthHint")}
            </p>
          )}
          {scopes.length > 0 && a.status !== "disconnected" && (
            <div className="pt-1 text-xs">
              <span className="text-[var(--muted)]">{t("accounts.permissions")}: </span>
              {scopes.join(" · ")}
            </div>
          )}
        </div>

        <div className="flex shrink-0 flex-wrap gap-2">
          {a.status !== "active" && canReconnect && (
            <button className={primary} disabled={busy} onClick={onReconnect}>
              {t("accounts.reconnect")}
            </button>
          )}
          {a.status !== "disconnected" && !confirming && (
            <button className={secondary} onClick={() => setConfirming(true)}>
              {t("accounts.disconnect")}
            </button>
          )}
        </div>
      </div>

      {confirming && (
        <div className="mt-3 space-y-2 rounded-md border border-red-500/40 p-3 text-sm">
          <p>{t("accounts.confirmDisconnect", { name: a.displayName })}</p>
          <div className="flex gap-2">
            <button
              className={danger}
              disabled={disconnect.isPending}
              onClick={() => disconnect.mutate()}
            >
              {t("accounts.confirm")}
            </button>
            <button className={secondary} onClick={() => setConfirming(false)}>
              {t("accounts.cancel")}
            </button>
          </div>
        </div>
      )}
      {disconnect.isError && (
        <p role="alert" className="mt-2 text-sm text-red-500">
          {errorMessage(disconnect.error, t)}
        </p>
      )}
    </li>
  );
}

function NotRevokedNotice({ platform, onClose }: { platform: string; onClose: () => void }) {
  const { t } = useTranslation();
  const url = platformUI(platform)?.permissionsUrl;
  return (
    <div
      role="status"
      className="space-y-2 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm"
    >
      <p>{t("accounts.notRevoked")}</p>
      <div className="flex gap-2">
        {url && (
          <button className={secondary} onClick={() => void openUrl(url)}>
            {t("accounts.openPermissions")}
          </button>
        )}
        <button className={secondary} onClick={onClose}>
          {t("accounts.dismiss")}
        </button>
      </div>
    </div>
  );
}
