import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { commands } from "../../lib/bindings";
import { errorMessage } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";
import { MetricChart } from "../../components/MetricChart";

export function DashboardPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const accounts = useQuery({
    queryKey: queryKeys.accounts,
    queryFn: () => commands.listAccounts(),
  });
  const addMock = useMutation({
    mutationFn: () => commands.addMockAccount(),
    onSuccess: () => qc.invalidateQueries({ queryKey: queryKeys.accounts }),
  });
  const visible = accounts.data?.filter((a) => a.status !== "disconnected");
  const btn =
    "rounded-md bg-[var(--accent)] px-3 py-1.5 text-sm font-medium text-white hover:opacity-90";

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold">{t("dashboard.title")}</h1>
      {visible?.length === 0 && (
        <div className="space-y-3">
          <p className="text-[var(--muted)]">{t("dashboard.noAccounts")}</p>
          <div className="flex gap-2">
            <Link to="/settings" className={btn}>
              {t("dashboard.connectNetwork")}
            </Link>
            <button
              className="rounded-md border border-[var(--border)] px-3 py-1.5 text-sm"
              onClick={() => addMock.mutate()}
            >
              {t("dashboard.addMock")}
            </button>
          </div>
        </div>
      )}
      {addMock.isError && <p className="text-red-500">{errorMessage(addMock.error, t)}</p>}
      {visible?.map((a) => (
        <section
          key={a.id}
          className="rounded-lg border border-[var(--border)] bg-[var(--panel)] p-4"
        >
          <h2 className="mb-3 font-medium">{a.displayName}</h2>
          <MetricChart accountId={a.id} />
        </section>
      ))}
    </div>
  );
}
