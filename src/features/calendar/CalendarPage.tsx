import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useState, type DragEvent } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { commands, type CalendarEvent, type EventKind } from "../../lib/bindings";
import {
  addDays,
  dayKey,
  fromLocalInput,
  monthGrid,
  moveTo,
  sameDay,
  startOfWeek,
  toLocalInput,
} from "../../lib/dates";
import { errorMessage } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";

const DRAG_TYPE = "application/x-omnyra-event";
const HOURS = Array.from({ length: 24 }, (_, h) => h);
type View = "month" | "week";

function eventColor(e: CalendarEvent, brand: (p: string) => string): string {
  if (e.kind === "reminder") return "#d97706";
  if (e.kind === "idea") return "var(--accent)";
  return e.platform ? brand(e.platform) : "#888";
}

export function CalendarPage() {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const qc = useQueryClient();
  const [view, setView] = useState<View>("month");
  const [cursor, setCursor] = useState(() => new Date());
  const [platform, setPlatform] = useState<string>("all");
  const [selected, setSelected] = useState<CalendarEvent | null>(null);
  const [newTitle, setNewTitle] = useState("");
  const [newAt, setNewAt] = useState("");

  const days =
    view === "month"
      ? monthGrid(cursor)
      : Array.from({ length: 7 }, (_, i) => addDays(startOfWeek(cursor), i));
  const from = days[0].toISOString();
  const to = addDays(days[days.length - 1], 1).toISOString();

  const platforms = useQuery({
    queryKey: queryKeys.platforms,
    queryFn: () => commands.listPlatforms(),
  });
  const brand = (p: string) => platforms.data?.find((x) => x.id === p)?.brandColor ?? "#888";
  const events = useQuery({
    queryKey: queryKeys.calendar(from, to),
    queryFn: () => commands.getCalendar(from, to),
  });
  const visible = (events.data ?? []).filter(
    (e) => platform === "all" || e.platform === platform || (!e.platform && platform === "none"),
  );
  const byDay = new Map<string, CalendarEvent[]>();
  for (const e of visible) {
    const k = dayKey(new Date(e.at));
    byDay.set(k, [...(byDay.get(k) ?? []), e]);
  }

  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["calendar"] });
    void qc.invalidateQueries({ queryKey: queryKeys.ideas });
  };
  const move = useMutation({
    mutationFn: (m: { kind: EventKind; id: string; at: string }) =>
      commands.moveCalendarEvent(m.kind, m.id, m.at),
    onSettled: refresh,
  });
  const addReminder = useMutation({
    mutationFn: () =>
      commands.createReminder({ title: newTitle, at: fromLocalInput(newAt) ?? "", platform: null }),
    onSuccess: () => {
      setNewTitle("");
      refresh();
    },
  });
  const delReminder = useMutation({
    mutationFn: (id: string) => commands.deleteReminder(id),
    onSuccess: () => {
      setSelected(null);
      refresh();
    },
  });

  const onDragStart = (ev: DragEvent, e: CalendarEvent) => {
    ev.dataTransfer.setData(DRAG_TYPE, JSON.stringify({ kind: e.kind, id: e.id, at: e.at }));
    ev.dataTransfer.effectAllowed = "move";
  };
  const allowDrop = (ev: DragEvent) => {
    if (ev.dataTransfer.types.includes(DRAG_TYPE)) ev.preventDefault();
  };
  const onDrop = (ev: DragEvent, day: Date, hour?: number) => {
    const raw = ev.dataTransfer.getData(DRAG_TYPE);
    if (!raw) return;
    ev.preventDefault();
    const d = JSON.parse(raw) as { kind: EventKind; id: string; at: string };
    const at = moveTo(d.at, day, hour);
    if (at !== new Date(d.at).toISOString()) move.mutate({ kind: d.kind, id: d.id, at });
  };
  const pickDay = (day: Date, hour = 10) => {
    const d = new Date(day.getFullYear(), day.getMonth(), day.getDate(), hour);
    setNewAt(toLocalInput(d.toISOString()));
  };

  const today = new Date();
  const title =
    view === "month"
      ? cursor.toLocaleDateString(lang, { month: "long", year: "numeric" })
      : `${days[0].toLocaleDateString(lang, { day: "numeric", month: "short" })} – ${days[6].toLocaleDateString(lang, { day: "numeric", month: "short", year: "numeric" })}`;
  const step = (n: number) =>
    setCursor((c) =>
      view === "month" ? new Date(c.getFullYear(), c.getMonth() + n, 1) : addDays(c, 7 * n),
    );
  const btn = "rounded-md border border-[var(--border)] px-2 py-1 text-xs";
  const pill = (a: boolean) =>
    `rounded-md px-2 py-1 text-xs ${a ? "bg-[var(--accent)] text-white" : "border border-[var(--border)]"}`;
  const time = (iso: string) =>
    new Date(iso).toLocaleTimeString(lang, { hour: "2-digit", minute: "2-digit" });
  const weekdays = Array.from({ length: 7 }, (_, i) =>
    addDays(startOfWeek(today), i).toLocaleDateString(lang, { weekday: "short" }),
  );

  const chip = (e: CalendarEvent) => (
    <button
      key={`${e.kind}-${e.id}`}
      draggable={e.movable}
      onDragStart={(ev) => onDragStart(ev, e)}
      onClick={(ev) => {
        ev.stopPropagation();
        setSelected(e);
      }}
      title={`${time(e.at)} · ${e.title}`}
      className={`flex w-full items-center gap-1 truncate rounded px-1 text-left text-[11px] hover:bg-[var(--border)] ${e.movable ? "cursor-grab" : ""}`}
    >
      <span className="size-2 shrink-0 rounded-full" style={{ background: eventColor(e, brand) }} />
      <span className="truncate">
        {view === "month" && <span className="text-[var(--muted)]">{time(e.at)} </span>}
        {e.title || t("upload.untitled")}
      </span>
    </button>
  );

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-2xl font-semibold capitalize">{title}</h1>
        <div className="flex flex-wrap items-center gap-2">
          <button className={btn} onClick={() => step(-1)} aria-label={t("calendar.prev")}>
            ‹
          </button>
          <button className={btn} onClick={() => setCursor(new Date())}>
            {t("calendar.today")}
          </button>
          <button className={btn} onClick={() => step(1)} aria-label={t("calendar.next")}>
            ›
          </button>
          <div className="flex gap-1" role="group">
            {(["month", "week"] as const).map((v) => (
              <button
                key={v}
                aria-pressed={v === view}
                className={pill(v === view)}
                onClick={() => setView(v)}
              >
                {t(`calendar.view.${v}`)}
              </button>
            ))}
          </div>
          <select
            className="rounded-md border border-[var(--border)] px-2 py-1 text-xs"
            value={platform}
            onChange={(e) => setPlatform(e.target.value)}
            aria-label={t("calendar.filter")}
          >
            <option value="all">{t("calendar.allNetworks")}</option>
            {platforms.data
              ?.filter((p) => p.id !== "mock")
              .map((p) => (
                <option key={p.id} value={p.id}>
                  {p.displayName}
                </option>
              ))}
            <option value="none">{t("calendar.noNetwork")}</option>
          </select>
        </div>
      </div>

      <form
        className="flex flex-wrap items-center gap-2 text-sm"
        onSubmit={(ev) => {
          ev.preventDefault();
          addReminder.mutate();
        }}
      >
        <input
          className="min-w-48 flex-1 rounded-md border border-[var(--border)] bg-[var(--panel)] px-2 py-1"
          placeholder={t("calendar.reminderTitle")}
          value={newTitle}
          onChange={(e) => setNewTitle(e.target.value)}
        />
        <input
          type="datetime-local"
          className="rounded-md border border-[var(--border)] bg-[var(--panel)] px-2 py-1"
          value={newAt}
          onChange={(e) => setNewAt(e.target.value)}
        />
        <button
          className="rounded-md bg-[var(--accent)] px-3 py-1 text-white disabled:opacity-50"
          disabled={!newTitle.trim() || !newAt || addReminder.isPending}
        >
          {t("calendar.addReminder")}
        </button>
        {addReminder.isError && (
          <span className="text-red-500">{errorMessage(addReminder.error, t)}</span>
        )}
      </form>
      <p className="text-xs text-[var(--muted)]">{t("calendar.hint")}</p>
      {move.isError && <p className="text-sm text-red-500">{errorMessage(move.error, t)}</p>}
      {events.isError && <p className="text-sm text-red-500">{errorMessage(events.error, t)}</p>}

      {selected && (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-3 text-sm">
          <div>
            <p className="font-medium">{selected.title || t("upload.untitled")}</p>
            <p className="text-xs text-[var(--muted)]">
              {new Date(selected.at).toLocaleString(lang)} · {t(`calendar.kind.${selected.kind}`)}
              {selected.platform &&
                ` · ${platforms.data?.find((p) => p.id === selected.platform)?.displayName ?? selected.platform}`}
              {selected.detail &&
                ` · ${t(`calendar.detail.${selected.detail}`, { defaultValue: selected.detail })}`}
            </p>
          </div>
          <div className="flex gap-1">
            {selected.url && (
              <button className={btn} onClick={() => void openUrl(selected.url!)}>
                {t("calendar.open")}
              </button>
            )}
            {selected.kind === "idea" && (
              <Link className={btn} to={`/ideas?open=${selected.id}`}>
                {t("calendar.openIdea")}
              </Link>
            )}
            {selected.kind === "reminder" && (
              <button className={btn} onClick={() => delReminder.mutate(selected.id)}>
                {t("calendar.deleteReminder")}
              </button>
            )}
            <button className={btn} onClick={() => setSelected(null)}>
              {t("accounts.dismiss")}
            </button>
          </div>
        </div>
      )}

      {view === "month" ? (
        <div className="grid grid-cols-7 gap-px overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--border)]">
          {weekdays.map((w) => (
            <div
              key={w}
              className="bg-[var(--panel)] px-2 py-1 text-xs capitalize text-[var(--muted)]"
            >
              {w}
            </div>
          ))}
          {days.map((d) => {
            const list = byDay.get(dayKey(d)) ?? [];
            const out = d.getMonth() !== cursor.getMonth();
            return (
              <div
                key={dayKey(d)}
                onDragOver={allowDrop}
                onDrop={(ev) => onDrop(ev, d)}
                onClick={() => pickDay(d)}
                className={`min-h-24 space-y-0.5 bg-[var(--bg)] p-1 ${out ? "opacity-50" : ""}`}
              >
                <div
                  className={`text-xs ${sameDay(d, today) ? "inline-block rounded-full bg-[var(--accent)] px-1.5 text-white" : "text-[var(--muted)]"}`}
                >
                  {d.getDate()}
                </div>
                {list.slice(0, 4).map(chip)}
                {list.length > 4 && (
                  <p className="px-1 text-[11px] text-[var(--muted)]">
                    {t("calendar.more", { count: list.length - 4 })}
                  </p>
                )}
              </div>
            );
          })}
        </div>
      ) : (
        <div className="max-h-[70vh] overflow-auto rounded-lg border border-[var(--border)]">
          <div className="grid" style={{ gridTemplateColumns: "3rem repeat(7, minmax(0, 1fr))" }}>
            <div className="sticky top-0 z-10 bg-[var(--panel)]" />
            {days.map((d) => (
              <div
                key={dayKey(d)}
                className={`sticky top-0 z-10 border-l border-[var(--border)] bg-[var(--panel)] px-1 py-1 text-xs capitalize ${sameDay(d, today) ? "font-semibold text-[var(--accent)]" : "text-[var(--muted)]"}`}
              >
                {d.toLocaleDateString(lang, { weekday: "short", day: "numeric" })}
              </div>
            ))}
            {HOURS.map((h) => (
              <div key={h} className="contents">
                <div className="border-t border-[var(--border)] pr-1 text-right text-[10px] text-[var(--muted)]">{`${String(h).padStart(2, "0")}:00`}</div>
                {days.map((d) => {
                  const list = (byDay.get(dayKey(d)) ?? []).filter(
                    (e) => new Date(e.at).getHours() === h,
                  );
                  return (
                    <div
                      key={dayKey(d)}
                      onDragOver={allowDrop}
                      onDrop={(ev) => onDrop(ev, d, h)}
                      onClick={() => pickDay(d, h)}
                      className="min-h-8 border-l border-t border-[var(--border)] p-0.5"
                    >
                      {list.map(chip)}
                    </div>
                  );
                })}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
