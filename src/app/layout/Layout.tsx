import { Suspense } from "react";
import { useTranslation } from "react-i18next";
import { Outlet, useLocation } from "react-router";
import { Moon, Sun } from "lucide-react";
import { Sidebar } from "./Sidebar";
import { AlertsBell } from "../../features/alerts/AlertsBell";
import { LockGate } from "../../features/security/LockGate";
import { useUi } from "../../stores/ui";

function Loading() {
  const { t } = useTranslation();
  return <p className="text-[var(--muted)]">{t("common.loading")}</p>;
}

function ThemeToggle() {
  const { t } = useTranslation();
  const { theme, toggleTheme } = useUi();
  return (
    <button
      type="button"
      aria-label={t("settings.theme")}
      className="rounded-xl p-2 text-[var(--muted)] hover:bg-[var(--accent-soft)] hover:text-[var(--text)]"
      onClick={toggleTheme}
    >
      {theme === "dark" ? <Sun size={18} /> : <Moon size={18} />}
    </button>
  );
}

export function Layout() {
  const { pathname } = useLocation();
  return (
    <LockGate>
      <div className="flex h-screen">
        <Sidebar />
        <div className="flex flex-1 flex-col overflow-hidden">
          <header className="flex items-center justify-end gap-1 px-6 pt-3">
            <ThemeToggle />
            <AlertsBell />
          </header>
          <main className="flex-1 overflow-y-auto px-8 pb-10 pt-2">
            <div key={pathname} className="page-enter mx-auto max-w-[1400px]">
              <Suspense fallback={<Loading />}>
                <Outlet />
              </Suspense>
            </div>
          </main>
        </div>
      </div>
    </LockGate>
  );
}
