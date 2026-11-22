import { useMutation, useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { commands } from "../../lib/bindings";
import { errorMessage } from "../../lib/ipc";

const VERSION = import.meta.env.VITE_APP_VERSION ?? "";
const btn = "rounded-md border border-[var(--border)] px-3 py-1.5 text-sm disabled:opacity-50";

/** Version and signed auto-updates (only in release builds that carry the key). */
export function UpdatesSection() {
  const { t } = useTranslation();
  const available = useQuery({
    queryKey: ["updaterAvailable"],
    queryFn: () => commands.updaterAvailable(),
  });
  const check = useMutation({ mutationFn: () => commands.checkUpdate() });
  const install = useMutation({ mutationFn: () => commands.installUpdate() });
  return (
    <section className="max-w-md space-y-3">
      <h2 className="text-lg font-semibold">{t("updates.title")}</h2>
      <p className="text-sm text-[var(--muted)]">{t("updates.version", { version: VERSION })}</p>
      {available.data ? (
        <div className="space-y-2">
          <button className={btn} disabled={check.isPending} onClick={() => check.mutate()}>
            {check.isPending ? t("updates.checking") : t("updates.check")}
          </button>
          {check.isSuccess && !check.data && <p className="text-sm">{t("updates.upToDate")}</p>}
          {check.data && (
            <div className="space-y-1 text-sm">
              <p>{t("updates.found", { version: check.data.version })}</p>
              {check.data.notes && (
                <p className="whitespace-pre-wrap text-xs text-[var(--muted)]">
                  {check.data.notes}
                </p>
              )}
              <button className={btn} disabled={install.isPending} onClick={() => install.mutate()}>
                {install.isPending ? t("updates.installing") : t("updates.install")}
              </button>
            </div>
          )}
          {(check.isError || install.isError) && (
            <p className="text-sm text-red-500">{errorMessage(check.error ?? install.error, t)}</p>
          )}
        </div>
      ) : (
        <p className="text-xs text-[var(--muted)]">{t("updates.devBuild")}</p>
      )}
    </section>
  );
}
