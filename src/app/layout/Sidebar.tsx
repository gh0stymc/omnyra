import { NavLink } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  Calendar,
  LayoutDashboard,
  MessageSquare,
  Palette,
  Settings,
  Shield,
  Handshake,
  Radar,
  Upload,
  Lightbulb,
  Send,
} from "lucide-react";
import { commands } from "../../lib/bindings";
import { queryKeys } from "../../lib/queryKeys";

const sections = [
  { to: "/", key: "dashboard", icon: LayoutDashboard },
  { to: "/uploads", key: "uploads", icon: Upload },
  { to: "/publish", key: "publish", icon: Send },
  { to: "/calendar", key: "calendar", icon: Calendar },
  { to: "/ideas", key: "ideas", icon: Lightbulb },
  { to: "/community", key: "community", icon: MessageSquare },
  { to: "/creation", key: "creation", icon: Palette },
  { to: "/sponsors", key: "sponsors", icon: Handshake },
  { to: "/competitors", key: "competitors", icon: Radar },
  { to: "/security", key: "security", icon: Shield },
] as const;

const link =
  "flex items-center gap-2 rounded-md px-3 py-2 text-sm text-[var(--muted)] hover:bg-[var(--border)] hover:text-[var(--text)]";
const active = "bg-[var(--border)] !text-[var(--text)]";

export function Sidebar() {
  const { t } = useTranslation();
  const accounts = useQuery({
    queryKey: queryKeys.accounts,
    queryFn: () => commands.listAccounts(),
  });
  const platforms = useQuery({
    queryKey: queryKeys.platforms,
    queryFn: () => commands.listPlatforms(),
  });
  const color = (id: string) => platforms.data?.find((p) => p.id === id)?.brandColor ?? "#888";

  return (
    <aside className="flex w-60 shrink-0 flex-col gap-1 border-r border-[var(--border)] bg-[var(--panel)] p-3">
      <div className="px-3 py-2 text-lg font-semibold">{t("app.name")}</div>
      {sections.map(({ to, key, icon: Icon }) => (
        <NavLink
          key={to}
          to={to}
          end={to === "/"}
          className={({ isActive }) => `${link} ${isActive ? active : ""}`}
        >
          <Icon size={16} /> {t(`nav.${key}`)}
        </NavLink>
      ))}
      <div className="mt-4 px-3 text-xs uppercase tracking-wide text-[var(--muted)]">
        {t("nav.accounts")}
      </div>
      {accounts.data
        ?.filter((a) => a.status !== "disconnected")
        .map((a) => (
          <NavLink
            key={a.id}
            to={`/platform/${a.platform}/${a.id}`}
            className={({ isActive }) => `${link} ${isActive ? active : ""}`}
          >
            <span className="size-2 rounded-full" style={{ background: color(a.platform) }} />
            {a.displayName}
          </NavLink>
        ))}
      <div className="mt-auto" />
      <NavLink to="/settings" className={({ isActive }) => `${link} ${isActive ? active : ""}`}>
        <Settings size={16} /> {t("nav.settings")}
      </NavLink>
    </aside>
  );
}
