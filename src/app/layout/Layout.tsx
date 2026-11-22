import { Suspense } from "react";
import { useTranslation } from "react-i18next";
import { Outlet } from "react-router";
import { Sidebar } from "./Sidebar";
import { AlertsBell } from "../../features/alerts/AlertsBell";
import { LockGate } from "../../features/security/LockGate";

function Loading() {
  const { t } = useTranslation();
  return <p className="text-[var(--muted)]">{t("common.loading")}</p>;
}

export function Layout() {
  return (
    <LockGate>
      <div className="flex h-screen">
        <Sidebar />
        <div className="flex flex-1 flex-col overflow-hidden">
          <header className="flex items-center justify-end border-b border-[var(--border)] px-4 py-2">
            <AlertsBell />
          </header>
          <main className="flex-1 overflow-y-auto p-6">
            <Suspense fallback={<Loading />}>
              <Outlet />
            </Suspense>
          </main>
        </div>
      </div>
    </LockGate>
  );
}
