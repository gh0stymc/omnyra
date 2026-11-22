import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { commands } from "../../lib/bindings";
import { errorMessage } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";
import { LOCK_KEY } from "./LockGate";

const STATES = ["unknown", "ok", "warning", "not_applicable"] as const;
const field = "rounded-md border border-[var(--border)] bg-[var(--bg)] px-2 py-1 text-sm";
const btn = "rounded-md border border-[var(--border)] px-2 py-1 text-xs disabled:opacity-50";
const card = "space-y-2 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-4";
const TONE: Record<string, string> = {
  ok: "text-green-600",
  warning: "text-amber-500",
  unknown: "text-[var(--muted)]",
  not_applicable: "text-[var(--muted)]",
};

function Checklist() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const data = useQuery({ queryKey: ["checklist"], queryFn: () => commands.securityChecklist() });
  const set = useMutation({
    mutationFn: (v: { a: string; i: string; s: string }) =>
      commands.setChecklistItem(v.a, v.i, v.s),
    onSettled: () => void qc.invalidateQueries({ queryKey: ["checklist"] }),
  });
  if (!data.data?.length)
    return <p className="text-sm text-[var(--muted)]">{t("security.noAccounts")}</p>;
  return (
    <div className="grid gap-3 lg:grid-cols-2">
      {data.data.map((a) => (
        <section key={a.accountId} className={card}>
          <h3 className="flex justify-between text-sm font-medium">
            {a.displayName} · {a.platform}
            <span className="text-xs text-[var(--muted)]">
              {Math.round((a.score ?? 0) * 100)} %
            </span>
          </h3>
          <ul className="space-y-1">
            {a.items.map((i) => (
              <li key={i.item} className="flex items-center justify-between gap-2 text-sm">
                <span className={TONE[i.state] ?? ""}>
                  {t(`security.item.${i.item}`, { defaultValue: i.item })}
                </span>
                <span className="flex gap-1">
                  {i.url && (
                    <button className={btn} onClick={() => void openUrl(i.url)}>
                      {t("security.openSettings")}
                    </button>
                  )}
                  <select
                    className={btn}
                    value={i.state}
                    onChange={(e) => set.mutate({ a: a.accountId, i: i.item, s: e.target.value })}
                  >
                    {STATES.map((s) => (
                      <option key={s} value={s}>
                        {t(`security.state.${s}`)}
                      </option>
                    ))}
                  </select>
                </span>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}

function Suspects() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const data = useQuery({ queryKey: ["suspects"], queryFn: () => commands.listSuspects() });
  const inv = () => void qc.invalidateQueries({ queryKey: ["suspects"] });
  const scan = useMutation({ mutationFn: () => commands.scanImpersonation(), onSettled: inv });
  const search = useMutation({
    mutationFn: () => commands.searchImpersonators(),
    onSettled: () => {
      inv();
      void qc.invalidateQueries({ queryKey: queryKeys.quota });
    },
  });
  const status = useMutation({
    mutationFn: (v: { id: string; s: string }) => commands.setSuspectStatus(v.id, v.s),
    onSettled: inv,
  });
  return (
    <section className={card}>
      <h2 className="font-medium">{t("security.impersonation")}</h2>
      <p className="text-xs text-[var(--muted)]">{t("security.impersonationHint")}</p>
      <div className="flex gap-2">
        <button className={btn} disabled={scan.isPending} onClick={() => scan.mutate()}>
          {t("security.scanComments")}
        </button>
        <button className={btn} disabled={search.isPending} onClick={() => search.mutate()}>
          {t("security.searchYoutube")}
        </button>
        {scan.data != null && (
          <span className="text-xs">{t("security.found", { count: scan.data })}</span>
        )}
      </div>
      {(scan.isError || search.isError) && (
        <p className="text-xs text-red-500">{errorMessage(scan.error ?? search.error, t)}</p>
      )}
      <ul className="space-y-1 text-sm">
        {data.data?.map((s) => (
          <li
            key={s.id}
            className={`flex items-center justify-between gap-2 ${s.status !== "new" ? "opacity-60" : ""}`}
          >
            <span>
              <b>{s.suspectName}</b>{" "}
              <span className="text-xs text-[var(--muted)]">
                {s.platform} · {Math.round((s.nameScore ?? 0) * 100)} % ·{" "}
                {t(`security.foundIn.${s.foundIn}`)}
              </span>
            </span>
            <span className="flex gap-1">
              {s.platform === "youtube" && s.suspectExternalId.startsWith("UC") && (
                <button
                  className={btn}
                  onClick={() =>
                    void openUrl(`https://www.youtube.com/channel/${s.suspectExternalId}`)
                  }
                >
                  {t("security.view")}
                </button>
              )}
              <button className={btn} onClick={() => status.mutate({ id: s.id, s: "reported" })}>
                {t("security.reported")}
              </button>
              <button className={btn} onClick={() => status.mutate({ id: s.id, s: "dismissed" })}>
                {t("security.dismiss")}
              </button>
            </span>
          </li>
        ))}
        {!data.data?.length && (
          <li className="text-xs text-[var(--muted)]">{t("security.noSuspects")}</li>
        )}
      </ul>
    </section>
  );
}

function LockSettings() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const status = useQuery({ queryKey: LOCK_KEY, queryFn: () => commands.lockStatus() });
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [repeat, setRepeat] = useState("");
  const inv = () => {
    setCurrent("");
    setNext("");
    setRepeat("");
    void qc.invalidateQueries({ queryKey: LOCK_KEY });
  };
  const setPin = useMutation({
    mutationFn: () => commands.setPin(current || null, next),
    onSuccess: inv,
  });
  const remove = useMutation({ mutationFn: () => commands.removePin(current), onSuccess: inv });
  const idle = useMutation({
    mutationFn: (m: number) => commands.setLockIdle(m),
    onSettled: () => void qc.invalidateQueries({ queryKey: LOCK_KEY }),
  });
  const enabled = !!status.data?.enabled;
  return (
    <section className={card}>
      <h2 className="font-medium">{t("security.lock.title")}</h2>
      <p className="text-xs text-amber-600">{t("security.lock.uiOnly")}</p>
      <div className="flex flex-wrap items-center gap-2">
        {enabled && (
          <input
            type="password"
            className={field}
            placeholder={t("security.lock.current")}
            value={current}
            onChange={(e) => setCurrent(e.target.value)}
          />
        )}
        <input
          type="password"
          className={field}
          placeholder={t("security.lock.new")}
          value={next}
          onChange={(e) => setNext(e.target.value)}
        />
        <input
          type="password"
          className={field}
          placeholder={t("security.lock.repeat")}
          value={repeat}
          onChange={(e) => setRepeat(e.target.value)}
        />
        <button
          className={btn}
          disabled={next.length < 4 || next !== repeat || (enabled && !current) || setPin.isPending}
          onClick={() => setPin.mutate()}
        >
          {enabled ? t("security.lock.change") : t("security.lock.enable")}
        </button>
        {enabled && (
          <button className={btn} disabled={!current} onClick={() => remove.mutate()}>
            {t("security.lock.disable")}
          </button>
        )}
      </div>
      {next && repeat && next !== repeat && (
        <p className="text-xs text-red-500">{t("security.lock.mismatch")}</p>
      )}
      {(setPin.isError || remove.isError) && (
        <p className="text-xs text-red-500">{errorMessage(setPin.error ?? remove.error, t)}</p>
      )}
      {enabled && (
        <div className="flex items-center gap-2 text-sm">
          <span>{t("security.lock.idle")}</span>
          <select
            className={field}
            value={status.data?.idleMinutes ?? 10}
            onChange={(e) => idle.mutate(Number(e.target.value))}
          >
            {[0, 5, 10, 15, 30, 60].map((m) => (
              <option key={m} value={m}>
                {m ? t("security.lock.minutes", { m }) : t("security.lock.never")}
              </option>
            ))}
          </select>
          <button className={btn} onClick={() => void commands.lockNow().then(inv)}>
            {t("security.lock.lockNow")}
          </button>
        </div>
      )}
    </section>
  );
}

function AuditLog() {
  const { t, i18n } = useTranslation();
  const data = useQuery({ queryKey: ["audit"], queryFn: () => commands.auditLogView(100) });
  return (
    <section className={card}>
      <h2 className="flex items-center justify-between font-medium">
        {t("security.audit")}
        {data.data && (
          <span className={`text-xs ${data.data.chainOk ? "text-green-600" : "text-red-500"}`}>
            {data.data.chainOk ? t("security.chainOk") : t("security.chainBroken")}
          </span>
        )}
      </h2>
      <ul className="max-h-72 space-y-0.5 overflow-y-auto text-xs">
        {data.data?.entries.map((e) => (
          <li key={e.id} className="flex gap-2">
            <span className="text-[var(--muted)]">
              {new Date(e.ts).toLocaleString(i18n.language)}
            </span>
            <span className="font-medium">
              {t(`security.action.${e.action}`, { defaultValue: e.action })}
            </span>
            <span className="truncate text-[var(--muted)]">
              {e.detail !== "{}" ? e.detail : ""}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** Account audit, scam detection, impersonation, PIN lock and the audit log. */
export function SecurityPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const counts = useQuery({
    queryKey: queryKeys.inboxCounts,
    queryFn: () => commands.inboxCounts(),
  });
  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold">{t("nav.security")}</h1>
      <section className={card}>
        <h2 className="font-medium">{t("security.phishing")}</h2>
        <p className="text-sm">
          {t("security.phishingCount", { count: counts.data?.suspicious ?? 0 })}{" "}
          <button className="underline" onClick={() => navigate("/community")}>
            {t("security.review")}
          </button>
        </p>
        <p className="text-xs text-[var(--muted)]">{t("security.phishingHint")}</p>
      </section>
      <div>
        <h2 className="mb-2 font-medium">{t("security.checklist")}</h2>
        <Checklist />
      </div>
      <Suspects />
      <LockSettings />
      <AuditLog />
    </div>
  );
}
