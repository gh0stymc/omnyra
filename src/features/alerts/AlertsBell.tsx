import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Bell } from "lucide-react";
import { commands, type Alert } from "../../lib/bindings";
import { queryKeys } from "../../lib/queryKeys";

function useAlertText() {
  const { t, i18n } = useTranslation();
  return (a: Alert) => {
    const value = a.payload.value ?? 0;
    const title = a.payload.contentTitle ?? "";
    switch (a.kind) {
      case "milestone":
        return t("alerts.milestone", { value: value.toLocaleString(i18n.language) });
      case "overperforming":
        return t("alerts.overperforming", { title });
      case "underperforming":
        return t("alerts.underperforming", { title });
      default:
        return t("alerts.generic");
    }
  };
}

const dot: Record<string, string> = {
  positive: "bg-emerald-500",
  warning: "bg-amber-500",
  info: "bg-sky-500",
};

export function AlertsBell() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const text = useAlertText();

  const unread = useQuery({
    queryKey: queryKeys.alertsUnread,
    queryFn: () => commands.unreadAlertsCount(),
  });
  const alerts = useQuery({
    queryKey: queryKeys.alerts,
    queryFn: () => commands.listAlerts(30),
    enabled: open,
  });

  const invalidate = () => {
    void qc.invalidateQueries({ queryKey: queryKeys.alerts });
    void qc.invalidateQueries({ queryKey: queryKeys.alertsUnread });
  };
  const markRead = useMutation({
    mutationFn: (id: string) => commands.markAlertRead(id),
    onSuccess: invalidate,
  });
  const markAll = useMutation({
    mutationFn: () => commands.markAllAlertsRead(),
    onSuccess: invalidate,
  });

  const count = unread.data ?? 0;

  return (
    <div className="relative">
      <button
        type="button"
        aria-label={t("alerts.title")}
        className="relative rounded-md p-2 text-[var(--muted)] hover:bg-[var(--border)] hover:text-[var(--text)]"
        onClick={() => setOpen((v) => !v)}
      >
        <Bell size={18} />
        {count > 0 && (
          <span className="absolute -right-0.5 -top-0.5 flex min-w-4 items-center justify-center rounded-full bg-red-600 px-1 text-[10px] font-semibold text-white">
            {count > 99 ? "99+" : count}
          </span>
        )}
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} />
          <div className="absolute right-0 z-20 mt-2 w-80 rounded-lg border border-[var(--border)] bg-[var(--panel)] shadow-lg">
            <div className="flex items-center justify-between border-b border-[var(--border)] px-3 py-2">
              <span className="text-sm font-semibold">{t("alerts.title")}</span>
              {count > 0 && (
                <button
                  type="button"
                  className="text-xs text-[var(--accent)] hover:underline"
                  onClick={() => markAll.mutate()}
                >
                  {t("alerts.markAllRead")}
                </button>
              )}
            </div>
            <ul className="max-h-96 overflow-y-auto">
              {(alerts.data?.length ?? 0) === 0 && (
                <li className="px-3 py-6 text-center text-sm text-[var(--muted)]">
                  {t("alerts.empty")}
                </li>
              )}
              {alerts.data?.map((a) => (
                <li
                  key={a.id}
                  className={`flex cursor-pointer items-start gap-2 px-3 py-2 text-sm hover:bg-[var(--border)] ${
                    a.read ? "opacity-60" : ""
                  }`}
                  onClick={() => !a.read && markRead.mutate(a.id)}
                >
                  <span
                    className={`mt-1.5 size-2 shrink-0 rounded-full ${dot[a.severity] ?? dot.info}`}
                  />
                  <span>{text(a)}</span>
                </li>
              ))}
            </ul>
          </div>
        </>
      )}
    </div>
  );
}
