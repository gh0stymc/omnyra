import { useTranslation } from "react-i18next";
import { useRouteError } from "react-router";

/** Friendly screen when a page fails to load (e.g. a stale chunk after an update). */
export function RouteError() {
  const { t } = useTranslation();
  const error = useRouteError();
  const detail = error instanceof Error ? error.message : String(error ?? "");
  return (
    <div className="mx-auto mt-24 max-w-md space-y-3 rounded-2xl border border-[var(--border)] bg-[var(--panel)] p-8 text-center">
      <h1 className="text-xl font-semibold">{t("errors.pageTitle")}</h1>
      <p className="text-sm text-[var(--muted)]">{t("errors.pageHint")}</p>
      <button
        className="rounded-xl bg-[var(--accent)] px-4 py-2 text-sm font-medium text-white"
        onClick={() => window.location.reload()}
      >
        {t("errors.reload")}
      </button>
      {detail && <p className="break-all text-xs text-[var(--muted)] opacity-70">{detail}</p>}
    </div>
  );
}
