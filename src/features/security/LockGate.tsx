import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "../../lib/bindings";
import { errorMessage } from "../../lib/ipc";

export const LOCK_KEY = ["lockStatus"] as const;

/** Covers the app while it is locked; locks again after the configured idle time. */
export function LockGate({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [pin, setPin] = useState("");
  const [wrong, setWrong] = useState(false);
  const status = useQuery({
    queryKey: LOCK_KEY,
    queryFn: () => commands.lockStatus(),
    retry: false,
  });
  const unlock = useMutation({
    mutationFn: () => commands.unlock(pin),
    onSuccess: (ok) => {
      setWrong(!ok);
      setPin("");
      void qc.invalidateQueries({ queryKey: LOCK_KEY });
    },
  });
  const last = useRef(0);
  useEffect(() => {
    last.current = Date.now();
  }, []);

  // Idle auto-lock.
  const idle = status.data?.enabled ? (status.data.idleMinutes ?? 0) : 0;
  useEffect(() => {
    if (!idle) return;
    const touch = () => (last.current = Date.now());
    const events = ["pointerdown", "keydown", "wheel"] as const;
    events.forEach((e) => window.addEventListener(e, touch, { passive: true }));
    const timer = setInterval(() => {
      if (Date.now() - last.current > idle * 60_000) {
        void commands.lockNow().then(() => qc.invalidateQueries({ queryKey: LOCK_KEY }));
        last.current = Date.now();
      }
    }, 15_000);
    return () => {
      events.forEach((e) => window.removeEventListener(e, touch));
      clearInterval(timer);
    };
  }, [idle, qc]);

  if (!status.data?.locked) return <>{children}</>;
  const wait = status.data.retryAfterS ?? 0;
  return (
    <div className="flex h-screen items-center justify-center bg-[var(--bg)]">
      <form
        className="w-72 space-y-3 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-6 text-center"
        onSubmit={(e) => {
          e.preventDefault();
          if (pin) unlock.mutate();
        }}
      >
        <h1 className="text-lg font-semibold">{t("app.name")}</h1>
        <p className="text-sm text-[var(--muted)]">{t("security.lock.locked")}</p>
        <input
          autoFocus
          type="password"
          inputMode="numeric"
          className="w-full rounded-md border border-[var(--border)] bg-[var(--bg)] px-3 py-2 text-center text-lg tracking-widest"
          value={pin}
          onChange={(e) => setPin(e.target.value)}
          aria-label="PIN"
        />
        <button
          className="w-full rounded-md bg-[var(--accent)] px-3 py-2 text-sm text-white disabled:opacity-50"
          disabled={!pin || unlock.isPending}
        >
          {t("security.lock.unlock")}
        </button>
        {wrong && <p className="text-xs text-red-500">{t("security.lock.wrong")}</p>}
        {wait > 0 && (
          <p className="text-xs text-amber-500">{t("security.lock.wait", { s: wait })}</p>
        )}
        {unlock.isError && <p className="text-xs text-red-500">{errorMessage(unlock.error, t)}</p>}
      </form>
    </div>
  );
}
