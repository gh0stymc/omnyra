import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { commands, type AppSettings } from "../../lib/bindings";
import { queryKeys } from "../../lib/queryKeys";

type ToggleKey = Exclude<keyof AppSettings, "locale">;

/** Settings → "Background & notifications" (step 2c): tray, autostart, periodic sync, alerts. */
export function PreferencesSection() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const settings = useQuery({
    queryKey: queryKeys.settings,
    queryFn: () => commands.getSettings(),
  });

  const update = useMutation({
    mutationFn: (next: AppSettings) => commands.updateSettings(next),
    onSuccess: (saved) => qc.setQueryData(queryKeys.settings, saved),
  });

  const s = settings.data;
  const toggle = (key: ToggleKey) => {
    if (!s) return;
    update.mutate({ ...s, [key]: !s[key] });
  };

  const rows: { key: ToggleKey; title: string; hint: string }[] = [
    { key: "closeToTray", title: t("prefs.tray"), hint: t("prefs.trayHint") },
    { key: "autostart", title: t("prefs.autostart"), hint: t("prefs.autostartHint") },
    {
      key: "backgroundSync",
      title: t("prefs.backgroundSync"),
      hint: t("prefs.backgroundSyncHint"),
    },
    { key: "notifications", title: t("prefs.notifications"), hint: t("prefs.notificationsHint") },
  ];

  return (
    <section className="max-w-md space-y-6">
      <h2 className="text-lg font-semibold">{t("prefs.title")}</h2>
      {settings.isError && <p className="text-sm text-red-500">{t("common.error")}</p>}
      <div className="space-y-4">
        {rows.map(({ key, title, hint }) => (
          <label key={key} className="flex items-start justify-between gap-4">
            <span>
              <span className="block text-sm">{title}</span>
              <span className="block text-xs text-[var(--muted)]">{hint}</span>
            </span>
            <input
              type="checkbox"
              className="mt-1 size-4 shrink-0 accent-[var(--accent)]"
              checked={s?.[key] ?? false}
              disabled={!s || update.isPending}
              onChange={() => toggle(key)}
            />
          </label>
        ))}
      </div>
    </section>
  );
}
