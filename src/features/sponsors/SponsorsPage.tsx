import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type Deliverable,
  type MediaKitConfig,
  type Sponsorship,
  type SponsorshipInput,
} from "../../lib/bindings";
import { errorMessage } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";
import { formatMoney, parseMoney } from "./money";

const STATUSES = [
  "prospect",
  "negotiating",
  "signed",
  "in_progress",
  "delivered",
  "cancelled",
] as const;
const PAYMENT = ["pending", "invoiced", "paid"] as const;
const CURRENCIES = ["EUR", "USD", "GBP", "MXN", "ARS", "COP", "CLP"];
const KIT_METRICS = ["followers", "views30", "avgViews", "engagement", "topContents"] as const;
const field = "w-full rounded-md border border-[var(--border)] bg-[var(--bg)] px-2 py-1 text-sm";
const btn = "rounded-md border border-[var(--border)] px-2 py-1 text-xs disabled:opacity-50";
const primary = "rounded-md bg-[var(--accent)] px-3 py-1 text-sm text-white disabled:opacity-50";

const blankInput = (): SponsorshipInput => ({
  brand: "",
  contactName: null,
  contactEmail: null,
  status: "prospect",
  amountCents: null,
  currency: "EUR",
  paymentStatus: "pending",
  paymentDue: null,
  startDate: null,
  endDate: null,
  notes: null,
  deliverables: [],
});

const toInput = (s: Sponsorship): SponsorshipInput => ({
  brand: s.brand,
  contactName: s.contactName,
  contactEmail: s.contactEmail,
  status: s.status,
  amountCents: s.amountCents,
  currency: s.currency,
  paymentStatus: s.paymentStatus,
  paymentDue: s.paymentDue,
  startDate: s.startDate,
  endDate: s.endDate,
  notes: s.notes,
  deliverables: s.deliverables,
});

function Editor({ initial, onClose }: { initial: Sponsorship | null; onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [f, setF] = useState<SponsorshipInput>(initial ? toInput(initial) : blankInput());
  const [amount, setAmount] = useState(
    initial?.amountCents != null ? ((initial.amountCents ?? 0) / 100).toString() : "",
  );
  const set = <K extends keyof SponsorshipInput>(k: K, v: SponsorshipInput[K]) =>
    setF((x) => ({ ...x, [k]: v }));
  const str = (v: string) => (v.trim() ? v : null);
  const refresh = () => void qc.invalidateQueries({ queryKey: ["sponsors"] });
  const saveM = useMutation({
    mutationFn: () => {
      const input = { ...f, amountCents: parseMoney(amount) };
      return initial
        ? commands.updateSponsorship(initial.id, input)
        : commands.createSponsorship(input);
    },
    onSuccess: () => {
      refresh();
      onClose();
    },
  });
  const del = useMutation({
    mutationFn: () => commands.deleteSponsorship(initial!.id),
    onSuccess: () => {
      refresh();
      onClose();
    },
  });
  const attach = useMutation({
    mutationFn: async () => {
      const p = await open({ multiple: false });
      if (typeof p === "string") await commands.attachSponsorshipFile(initial!.id, p);
    },
    onSettled: refresh,
  });
  const detach = useMutation({
    mutationFn: (fileId: string) => commands.detachSponsorshipFile(initial!.id, fileId),
    onSettled: refresh,
  });
  const setDel = (i: number, patch: Partial<Deliverable>) =>
    set(
      "deliverables",
      f.deliverables.map((d, j) => (j === i ? { ...d, ...patch } : d)),
    );
  const live = useQuery({
    queryKey: ["sponsors"],
    queryFn: () => commands.listSponsorships(),
    enabled: !!initial,
  });
  const files = live.data?.items.find((s) => s.id === initial?.id)?.files ?? initial?.files ?? [];
  const amountOk = !amount.trim() || parseMoney(amount) !== null;

  return (
    <div className="fixed inset-0 z-20 flex justify-end bg-black/40" onClick={onClose}>
      <div
        className="h-full w-[32rem] space-y-3 overflow-y-auto bg-[var(--panel)] p-4"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="text-lg font-medium">{initial ? initial.brand : t("sponsors.new")}</h2>
        <label className="block space-y-1 text-xs">
          <span>{t("sponsors.brand")}</span>
          <input className={field} value={f.brand} onChange={(e) => set("brand", e.target.value)} />
        </label>
        <div className="grid grid-cols-2 gap-2">
          <label className="space-y-1 text-xs">
            <span>{t("sponsors.contactName")}</span>
            <input
              className={field}
              value={f.contactName ?? ""}
              onChange={(e) => set("contactName", str(e.target.value))}
            />
          </label>
          <label className="space-y-1 text-xs">
            <span>{t("sponsors.contactEmail")}</span>
            <input
              className={field}
              value={f.contactEmail ?? ""}
              onChange={(e) => set("contactEmail", str(e.target.value))}
            />
          </label>
          <label className="space-y-1 text-xs">
            <span>{t("sponsors.statusLabel")}</span>
            <select
              className={field}
              value={f.status}
              onChange={(e) => set("status", e.target.value)}
            >
              {STATUSES.map((s) => (
                <option key={s} value={s}>
                  {t(`sponsors.status.${s}`)}
                </option>
              ))}
            </select>
          </label>
          <div className="flex gap-1">
            <label className="flex-1 space-y-1 text-xs">
              <span>{t("sponsors.amount")}</span>
              <input
                className={`${field} ${amountOk ? "" : "border-red-500"}`}
                value={amount}
                onChange={(e) => setAmount(e.target.value)}
              />
            </label>
            <label className="w-20 space-y-1 text-xs">
              <span>&nbsp;</span>
              <select
                className={field}
                value={f.currency ?? "EUR"}
                onChange={(e) => set("currency", e.target.value)}
              >
                {CURRENCIES.map((c) => (
                  <option key={c}>{c}</option>
                ))}
              </select>
            </label>
          </div>
          <label className="space-y-1 text-xs">
            <span>{t("sponsors.paymentLabel")}</span>
            <select
              className={field}
              value={f.paymentStatus}
              onChange={(e) => set("paymentStatus", e.target.value)}
            >
              {PAYMENT.map((s) => (
                <option key={s} value={s}>
                  {t(`sponsors.payment.${s}`)}
                </option>
              ))}
            </select>
          </label>
          <label className="space-y-1 text-xs">
            <span>{t("sponsors.paymentDue")}</span>
            <input
              type="date"
              className={field}
              value={f.paymentDue ?? ""}
              onChange={(e) => set("paymentDue", str(e.target.value))}
            />
          </label>
          <label className="space-y-1 text-xs">
            <span>{t("sponsors.start")}</span>
            <input
              type="date"
              className={field}
              value={f.startDate ?? ""}
              onChange={(e) => set("startDate", str(e.target.value))}
            />
          </label>
          <label className="space-y-1 text-xs">
            <span>{t("sponsors.end")}</span>
            <input
              type="date"
              className={field}
              value={f.endDate ?? ""}
              onChange={(e) => set("endDate", str(e.target.value))}
            />
          </label>
        </div>
        <section className="space-y-1">
          <h3 className="text-sm font-medium">{t("sponsors.deliverables")}</h3>
          {f.deliverables.map((d, i) => (
            <div key={i} className="flex items-center gap-1">
              <input
                type="checkbox"
                checked={d.done}
                onChange={(e) => setDel(i, { done: e.target.checked })}
              />
              <input
                className={field}
                value={d.description}
                onChange={(e) => setDel(i, { description: e.target.value })}
              />
              <input
                type="date"
                className={`${field} w-36`}
                value={d.dueAt ?? ""}
                onChange={(e) => setDel(i, { dueAt: str(e.target.value) })}
              />
              <button
                className={btn}
                onClick={() =>
                  set(
                    "deliverables",
                    f.deliverables.filter((_, j) => j !== i),
                  )
                }
              >
                ✕
              </button>
            </div>
          ))}
          <button
            className={btn}
            onClick={() =>
              set("deliverables", [
                ...f.deliverables,
                { id: null, description: "", platform: null, dueAt: null, done: false },
              ])
            }
          >
            + {t("sponsors.addDeliverable")}
          </button>
        </section>
        <label className="block space-y-1 text-xs">
          <span>{t("ideas.notes")}</span>
          <textarea
            className={`${field} min-h-20`}
            value={f.notes ?? ""}
            onChange={(e) => set("notes", str(e.target.value))}
          />
        </label>
        {initial && (
          <section className="space-y-1">
            <h3 className="text-sm font-medium">{t("ideas.files")}</h3>
            {files.map((file) => (
              <div key={file.id} className="flex items-center gap-2 text-xs">
                <button
                  className="truncate underline"
                  onClick={() => void commands.openSponsorshipFile(file.id)}
                >
                  {file.name}
                </button>
                <button className={btn} onClick={() => detach.mutate(file.id)}>
                  {t("ideas.detach")}
                </button>
              </div>
            ))}
            <button className={btn} onClick={() => attach.mutate()}>
              {t("ideas.attach")}
            </button>
            <p className="text-xs text-[var(--muted)]">{t("ideas.filesNote")}</p>
          </section>
        )}
        <div className="flex items-center gap-2">
          <button
            className={primary}
            disabled={!f.brand.trim() || !amountOk || saveM.isPending}
            onClick={() => saveM.mutate()}
          >
            {t("ideas.save")}
          </button>
          <button className={btn} onClick={onClose}>
            {t("accounts.cancel")}
          </button>
          {initial && (
            <button className={`${btn} ml-auto text-red-500`} onClick={() => del.mutate()}>
              {t("ideas.delete")}
            </button>
          )}
        </div>
        {saveM.isError && <p className="text-xs text-red-500">{errorMessage(saveM.error, t)}</p>}
      </div>
    </div>
  );
}

function MediaKit() {
  const { t, i18n } = useTranslation();
  const frame = useRef<HTMLIFrameElement>(null);
  const accounts = useQuery({
    queryKey: queryKeys.accounts,
    queryFn: () => commands.listAccounts(),
  });
  const active = (accounts.data ?? []).filter((a) => a.status !== "disconnected");
  const [cfg, setCfg] = useState<MediaKitConfig>({
    title: "Media kit",
    bio: "",
    contactEmail: "",
    accountIds: [],
    metrics: ["followers", "views30", "avgViews", "engagement", "topContents"],
    accent: "#7c3aed",
    language: i18n.language === "en" ? "en" : "es",
  });
  const ids = cfg.accountIds.length ? cfg.accountIds : active.map((a) => a.id);
  const effective = { ...cfg, accountIds: ids };
  const html = useQuery({
    queryKey: ["mediaKit", effective],
    queryFn: () => commands.mediaKitHtml(effective),
    enabled: ids.length > 0,
  });
  const saveHtml = useMutation({
    mutationFn: async () => {
      const dest = await save({
        defaultPath: "media-kit.html",
        filters: [{ name: "HTML", extensions: ["html"] }],
      });
      if (dest) await commands.saveMediaKit(effective, dest);
      return !!dest;
    },
  });
  const toggle = <T,>(list: T[], v: T) =>
    list.includes(v) ? list.filter((x) => x !== v) : [...list, v];

  return (
    <section className="grid gap-4 lg:grid-cols-[20rem_1fr]">
      <div className="space-y-2">
        <input
          className={field}
          value={cfg.title}
          onChange={(e) => setCfg({ ...cfg, title: e.target.value })}
        />
        <textarea
          className={`${field} min-h-20`}
          placeholder={t("sponsors.kit.bio")}
          value={cfg.bio}
          onChange={(e) => setCfg({ ...cfg, bio: e.target.value })}
        />
        <input
          className={field}
          placeholder={t("sponsors.contactEmail")}
          value={cfg.contactEmail}
          onChange={(e) => setCfg({ ...cfg, contactEmail: e.target.value })}
        />
        <div className="space-y-1 text-xs">
          <p className="font-medium">{t("sponsors.kit.accounts")}</p>
          {active.map((a) => (
            <label key={a.id} className="flex items-center gap-1">
              <input
                type="checkbox"
                checked={ids.includes(a.id)}
                onChange={() => setCfg({ ...cfg, accountIds: toggle(ids, a.id) })}
              />
              {a.displayName} ({a.platform})
            </label>
          ))}
        </div>
        <div className="space-y-1 text-xs">
          <p className="font-medium">{t("sponsors.kit.metrics")}</p>
          {KIT_METRICS.map((m) => (
            <label key={m} className="flex items-center gap-1">
              <input
                type="checkbox"
                checked={cfg.metrics.includes(m)}
                onChange={() => setCfg({ ...cfg, metrics: toggle(cfg.metrics, m) })}
              />
              {t(`sponsors.kit.metric.${m}`)}
            </label>
          ))}
        </div>
        <div className="flex items-center gap-2 text-xs">
          <span>{t("sponsors.kit.accent")}</span>
          <input
            type="color"
            value={cfg.accent}
            onChange={(e) => setCfg({ ...cfg, accent: e.target.value })}
          />
          <select
            className={btn}
            value={cfg.language}
            onChange={(e) => setCfg({ ...cfg, language: e.target.value })}
          >
            <option value="es">Español</option>
            <option value="en">English</option>
          </select>
        </div>
        <div className="flex flex-wrap gap-2">
          <button className={btn} disabled={!html.data} onClick={() => saveHtml.mutate()}>
            {t("sponsors.kit.saveHtml")}
          </button>
          <button
            className={btn}
            disabled={!html.data}
            onClick={() => frame.current?.contentWindow?.print()}
          >
            {t("sponsors.kit.print")}
          </button>
        </div>
        <p className="text-xs text-[var(--muted)]">{t("sponsors.kit.printHint")}</p>
        {html.isError && <p className="text-xs text-red-500">{errorMessage(html.error, t)}</p>}
      </div>
      <iframe
        ref={frame}
        title="media kit"
        sandbox="allow-same-origin allow-modals"
        srcDoc={html.data ?? ""}
        className="h-[36rem] w-full rounded-md border border-[var(--border)] bg-white"
      />
    </section>
  );
}

/** Sponsorship manager and media kit. */
export function SponsorsPage() {
  const { t, i18n } = useTranslation();
  const [tab, setTab] = useState<"deals" | "kit">("deals");
  const [editing, setEditing] = useState<Sponsorship | null | "new">(null);
  const data = useQuery({ queryKey: ["sponsors"], queryFn: () => commands.listSponsorships() });
  const lang = i18n.language;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">{t("nav.sponsors")}</h1>
        <div className="flex gap-1">
          {(["deals", "kit"] as const).map((k) => (
            <button
              key={k}
              className={`rounded-md px-3 py-1 text-sm ${tab === k ? "bg-[var(--accent)] text-white" : "border border-[var(--border)]"}`}
              onClick={() => setTab(k)}
            >
              {t(`sponsors.tab.${k}`)}
            </button>
          ))}
        </div>
      </div>
      {tab === "kit" ? (
        <MediaKit />
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-3">
            <button className={primary} onClick={() => setEditing("new")}>
              + {t("sponsors.new")}
            </button>
            {data.data?.totals.map((m) => (
              <span key={m.currency} className="text-sm text-[var(--muted)]">
                {t("sponsors.totals", {
                  signed: formatMoney(m.signedCents, m.currency, lang),
                  paid: formatMoney(m.paidCents, m.currency, lang),
                  pending: formatMoney(m.pendingCents, m.currency, lang),
                })}
                {(m.overdueCents ?? 0) > 0 && (
                  <span className="ml-2 text-red-500">
                    {t("sponsors.overdueTotal", {
                      amount: formatMoney(m.overdueCents, m.currency, lang),
                    })}
                  </span>
                )}
              </span>
            ))}
          </div>
          {data.isError && <p className="text-sm text-red-500">{errorMessage(data.error, t)}</p>}
          {!data.data?.items.length ? (
            <p className="text-sm text-[var(--muted)]">{t("sponsors.empty")}</p>
          ) : (
            <table className="w-full text-sm">
              <thead className="text-left text-xs text-[var(--muted)]">
                <tr>
                  <th className="py-1">{t("sponsors.brand")}</th>
                  <th>{t("sponsors.statusLabel")}</th>
                  <th>{t("sponsors.amount")}</th>
                  <th>{t("sponsors.paymentLabel")}</th>
                  <th>{t("sponsors.deliverables")}</th>
                  <th>{t("sponsors.dates")}</th>
                </tr>
              </thead>
              <tbody>
                {data.data.items.map((s) => (
                  <tr
                    key={s.id}
                    className="cursor-pointer border-t border-[var(--border)] hover:bg-[var(--border)]"
                    onClick={() => setEditing(s)}
                  >
                    <td className="py-1.5 font-medium">{s.brand}</td>
                    <td>{t(`sponsors.status.${s.status}`)}</td>
                    <td>{formatMoney(s.amountCents, s.currency, lang)}</td>
                    <td className={s.overdue ? "text-red-500" : ""}>
                      {t(`sponsors.payment.${s.paymentStatus}`)}
                      {s.overdue && ` · ${t("sponsors.overdue")}`}
                    </td>
                    <td>
                      {s.deliverables.filter((d) => d.done).length}/{s.deliverables.length}
                    </td>
                    <td className="text-xs text-[var(--muted)]">
                      {[s.startDate, s.endDate].filter(Boolean).join(" → ")}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {editing && (
            <Editor initial={editing === "new" ? null : editing} onClose={() => setEditing(null)} />
          )}
        </>
      )}
    </div>
  );
}
