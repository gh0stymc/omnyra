import { useTranslation } from "react-i18next";
import { SUPPORTED_LANGS, setLang } from "../../i18n";
import { useUi } from "../../stores/ui";
import { AccountsSection } from "./AccountsSection";

export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useUi();
  const field = "rounded-md border border-[var(--border)] bg-[var(--panel)] px-3 py-1.5";
  return (
    <div className="max-w-3xl space-y-10">
      <h1 className="text-2xl font-semibold">{t("settings.title")}</h1>
      <AccountsSection />
      <section className="max-w-md space-y-6">
        <h2 className="text-lg font-semibold">{t("settings.general")}</h2>
        <label className="flex items-center justify-between gap-4">
          {t("settings.language")}
          <select
            className={field}
            value={i18n.language}
            onChange={(e) => setLang(e.target.value as (typeof SUPPORTED_LANGS)[number])}
          >
            {SUPPORTED_LANGS.map((l) => (
              <option key={l} value={l}>
                {l.toUpperCase()}
              </option>
            ))}
          </select>
        </label>
        <label className="flex items-center justify-between gap-4">
          {t("settings.theme")}
          <select
            className={field}
            value={theme}
            onChange={(e) => setTheme(e.target.value as "light" | "dark")}
          >
            <option value="dark">{t("settings.dark")}</option>
            <option value="light">{t("settings.light")}</option>
          </select>
        </label>
      </section>
    </div>
  );
}
