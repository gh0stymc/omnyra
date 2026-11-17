import { useMutation } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { commands } from "../lib/bindings";
import { errorMessage } from "../lib/ipc";

/** Syncs the given accounts one after another. Results arrive through `sync-status`. */
export function SyncButton({ accountIds, label }: { accountIds: string[]; label?: string }) {
  const { t } = useTranslation();
  const sync = useMutation({
    mutationFn: async () => {
      let quotaLimited = false;
      for (const id of accountIds) {
        const r = await commands.syncAccount(id);
        quotaLimited ||= r.quotaLimited;
      }
      return quotaLimited;
    },
  });
  return (
    <span className="inline-flex flex-col items-end gap-1">
      <button
        className="rounded-md border border-[var(--border)] px-2 py-1 text-xs disabled:opacity-50"
        onClick={() => sync.mutate()}
        disabled={sync.isPending || accountIds.length === 0}
      >
        {sync.isPending ? t("dashboard.syncing") : (label ?? t("dashboard.sync"))}
      </button>
      {sync.isError && <span className="text-xs text-red-500">{errorMessage(sync.error, t)}</span>}
      {sync.data && <span className="text-xs text-amber-500">{t("dashboard.quotaLimited")}</span>}
    </span>
  );
}
