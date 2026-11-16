import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
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
  const btn =
    "rounded-md bg-[var(--accent)] px-3 py-1.5 text-sm font-medium text-white hover:opacity-90";

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold">{t("dashboard.title")}</h1>
      {accounts.data?.length === 0 && (
        <div className="space-y-3">
          <p className="text-[var(--muted)]">{t("dashboard.noAccounts")}</p>
          <button className={btn} onClick={() => addMock.mutate()}>
            {t("dashboard.addMock")}
          </button>
        </div>
      )}
      {addMock.isError && <p className="text-red-500">{errorMessage(addMock.error, t)}</p>}
      {accounts.data?.map((a) => (
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
