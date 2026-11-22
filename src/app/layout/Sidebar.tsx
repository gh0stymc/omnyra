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
  Orbit,
} from "lucide-react";
import { commands } from "../../lib/bindings";
import { queryKeys } from "../../lib/queryKeys";

const groups = [
  [
    { to: "/", key: "dashboard", icon: LayoutDashboard },
    { to: "/uploads", key: "uploads", icon: Upload },
    { to: "/publish", key: "publish", icon: Send },
  ],
  [
    { to: "/calendar", key: "calendar", icon: Calendar },
    { to: "/ideas", key: "ideas", icon: Lightbulb },
    { to: "/community", key: "community", icon: MessageSquare },
    { to: "/creation", key: "creation", icon: Palette },
  ],
  [
    { to: "/sponsors", key: "sponsors", icon: Handshake },
    { to: "/competitors", key: "competitors", icon: Radar },
    { to: "/security", key: "security", icon: Shield },
  ],
] as const;

const link =
  "group relative flex items-center gap-2.5 rounded-xl px-3 py-2 text-[13px] font-medium text-[var(--muted)] hover:bg-[var(--accent-soft)] hover:text-[var(--text)]";
const active =
  "!bg-[var(--accent-soft)] !text-[var(--text)] before:absolute before:left-0 before:top-1/2 before:h-5 before:w-[3px] before:-translate-y-1/2 before:rounded-full before:bg-[linear-gradient(var(--accent),var(--accent-2))]";

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
  const visible = accounts.data?.filter((a) => a.status !== "disconnected") ?? [];

  return (
    <aside className="m-3 mr-0 flex w-60 shrink-0 flex-col gap-0.5 overflow-y-auto rounded-2xl border border-[var(--border)] bg-[var(--panel)] p-3">
      <div className="mb-3 flex items-center gap-2.5 px-2 pt-1">
        <span className="grid size-8 place-items-center rounded-xl bg-[linear-gradient(135deg,var(--accent),var(--accent-2))] text-white shadow-[var(--glow)]">
          <Orbit size={18} strokeWidth={2.25} />
        </span>
        <span className="text-[17px] font-bold tracking-tight">{t("app.name")}</span>
      </div>
      {groups.map((items, i) => (
        <div key={i} className={i ? "mt-2 border-t border-[var(--border)] pt-2" : ""}>
          {items.map(({ to, key, icon: Icon }) => (
            <NavLink
              key={to}
              to={to}
              end={to === "/"}
              className={({ isActive }) => `${link} ${isActive ? active : ""}`}
            >
              <Icon size={16} strokeWidth={2} className="opacity-80 group-hover:opacity-100" />
              {t(`nav.${key}`)}
            </NavLink>
          ))}
        </div>
      ))}
      {visible.length > 0 && (
        <>
          <div className="mt-4 px-3 pb-1 text-[10px] font-semibold uppercase tracking-[0.12em] text-[var(--muted)]">
            {t("nav.accounts")}
          </div>
          {visible.map((a) => (
            <NavLink
              key={a.id}
              to={`/platform/${a.platform}/${a.id}`}
              className={({ isActive }) => `${link} ${isActive ? active : ""}`}
            >
              <span
                className="size-2.5 rounded-full ring-2 ring-[var(--panel-solid)]"
                style={{
                  background: color(a.platform),
                  boxShadow: `0 0 10px ${color(a.platform)}`,
                }}
              />
              <span className="truncate">{a.displayName}</span>
            </NavLink>
          ))}
        </>
      )}
      <div className="mt-auto pt-3" />
      <NavLink to="/settings" className={({ isActive }) => `${link} ${isActive ? active : ""}`}>
        <Settings size={16} /> {t("nav.settings")}
      </NavLink>
    </aside>
  );
}
