import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, events, type UploadRow } from "../../lib/bindings";
import { queryKeys } from "../../lib/queryKeys";

/** Live progress per publication, from the `upload-progress` event. */
function useLiveProgress() {
  const [progress, setProgress] = useState<Record<string, number>>({});
  useEffect(() => {
    const sub = events.uploadProgress
      .listen(({ payload }) =>
        setProgress((p) => ({
          ...p,
          [payload.publicationId]: payload.total ? (payload.sent ?? 0) / payload.total : 0,
        })),
      )
      .catch(() => undefined);
    return () => void sub.then((f) => f?.());
  }, []);
  return progress;
}

export function UploadQueue() {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const uploads = useQuery({ queryKey: queryKeys.uploads, queryFn: () => commands.listUploads() });
  const live = useLiveProgress();
  const refresh = () => void qc.invalidateQueries({ queryKey: queryKeys.uploads });
  const act = useMutation({
    mutationFn: ({ action, id }: { action: "cancel" | "retry" | "remove"; id: string }) =>
      action === "cancel"
        ? commands.cancelUpload(id)
        : action === "retry"
          ? commands.retryUpload(id)
          : commands.removeUpload(id),
    onSettled: refresh,
  });
  const btn = "rounded-md border border-[var(--border)] px-2 py-1 text-xs";

  if (!uploads.data?.length) {
    return <p className="text-sm text-[var(--muted)]">{t("upload.queueEmpty")}</p>;
  }

  const describe = (u: UploadRow) => {
    if (u.status === "queued" && u.retryAt) {
      const at = new Date(u.retryAt).toLocaleTimeString(i18n.language);
      return t("upload.retryAt", {
        at,
        reason: t(`errors.${u.errorCode}`, { defaultValue: u.errorCode ?? "" }),
      });
    }
    if (u.status === "failed" && u.errorCode) {
      return t(`errors.${u.errorCode}`, { defaultValue: t("errors.unknown") });
    }
    return null;
  };

  return (
    <ul className="space-y-2">
      {uploads.data.map((u) => {
        const p = u.status === "uploading" ? (live[u.id] ?? u.progress ?? 0) : (u.progress ?? 0);
        const note = describe(u);
        return (
          <li
            key={u.id}
            className="space-y-2 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-3"
          >
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="min-w-0">
                <p className="truncate text-sm font-medium">{u.title || t("upload.untitled")}</p>
                <p className="text-xs text-[var(--muted)]">
                  {t(`upload.status.${u.status}`)}
                  {u.status === "uploading" && ` · ${Math.floor(p * 100)} %`}
                </p>
              </div>
              <div className="flex gap-1">
                {u.status === "published" && u.externalId && (
                  <button
                    className={btn}
                    onClick={() => void openUrl(`https://youtu.be/${u.externalId}`)}
                  >
                    {t("upload.open")}
                  </button>
                )}
                {(u.status === "queued" || u.status === "uploading") && (
                  <button
                    className={btn}
                    onClick={() => act.mutate({ action: "cancel", id: u.id })}
                  >
                    {t("upload.cancel")}
                  </button>
                )}
                {(u.status === "failed" || u.status === "cancelled") && (
                  <button className={btn} onClick={() => act.mutate({ action: "retry", id: u.id })}>
                    {t("upload.retry")}
                  </button>
                )}
                {["published", "failed", "cancelled"].includes(u.status) && (
                  <button
                    className={btn}
                    onClick={() => act.mutate({ action: "remove", id: u.id })}
                  >
                    {t("upload.remove")}
                  </button>
                )}
              </div>
            </div>
            {(u.status === "uploading" || (u.status === "queued" && p > 0)) && (
              <div
                className="h-1.5 overflow-hidden rounded-full bg-[var(--border)]"
                role="progressbar"
                aria-valuenow={Math.floor(p * 100)}
                aria-valuemin={0}
                aria-valuemax={100}
              >
                <div className="h-full bg-[var(--accent)]" style={{ width: `${p * 100}%` }} />
              </div>
            )}
            {note && <p className="text-xs text-amber-500">{note}</p>}
            {u.warnings.map((w) => (
              <p key={w} className="text-xs text-amber-500">
                {t(`upload.warning.${w}`, { defaultValue: w })}
              </p>
            ))}
          </li>
        );
      })}
    </ul>
  );
}
