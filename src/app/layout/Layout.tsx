import { Outlet } from "react-router";
import { Sidebar } from "./Sidebar";
import { AlertsBell } from "../../features/alerts/AlertsBell";

export function Layout() {
  return (
    <div className="flex h-screen">
      <Sidebar />
      <div className="flex flex-1 flex-col overflow-hidden">
        <header className="flex items-center justify-end border-b border-[var(--border)] px-4 py-2">
          <AlertsBell />
        </header>
        <main className="flex-1 overflow-y-auto p-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
