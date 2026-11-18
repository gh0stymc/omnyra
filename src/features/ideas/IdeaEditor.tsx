import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Idea } from "../../lib/bindings";
import { fromLocalInput, toLocalInput } from "../../lib/dates";
import { errorMessage } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";

/** Side panel to edit a card: title, notes, target networks, due date and files. */
export function IdeaEditor({ idea, onClose }: { idea: Idea; onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [title, setTitle] = useState(idea.title);
  const [notes, setNotes] = useState(idea.notes ?? "");
  const [targets, setTargets] = useState<string[]>(idea.targetPlatforms);
  const [due, setDue] = useState(toLocalInput(idea.dueAt));
  const [confirmDelete, setConfirmDelete] = useState(false);
  const platforms = useQuery({
    queryKey: queryKeys.platforms,
    queryFn: () => commands.listPlatforms(),
  });
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: queryKeys.ideas });
    void qc.invalidateQueries({ queryKey: ["calendar"] });
  };

  const save = useMutation({
    mutationFn: () =>
      commands.updateIdea(idea.id, {
        title,
        notes: notes.trim() ? notes : null,
        targetPlatforms: targets,
        dueAt: fromLocalInput(due),
      }),
    onSuccess: () => {
      refresh();
      onClose();
    },
  });
  const remove = useMutation({
    mutationFn: () => commands.deleteIdea(idea.id),
    onSuccess: () => {
      refresh();
      onClose();
    },
  });
  const files = useMutation({
    mutationFn: async (op: { attach: string[] } | { detach: string }) => {
      if ("detach" in op) return commands.detachIdeaFile(idea.id, op.detach);
      let last: Idea | null = null;
      for (const p of op.attach) last = await commands.attachIdeaFile(idea.id, p);
      return last;
    },
    onSettled: refresh,
  });
  const openFile = useMutation({ mutationFn: (fileId: string) => commands.openIdeaFile(fileId) });
  // Files come from the list query, so they update as soon as it is refetched.
  const current = useQuery({
    queryKey: queryKeys.ideas,
    queryFn: () => commands.listIdeas(),
  }).data?.find((i) => i.id === idea.id);
  const attached = current?.files ?? idea.files;

  const pick = async () => {
    const p = await open({ multiple: true, directory: false });
    const list = Array.isArray(p) ? p : p ? [p] : [];
    if (list.length) files.mutate({ attach: list });
  };

  const field = "w-full rounded-md border border-[var(--border)] bg-[var(--panel)] px-3 py-1.5";
  const btn = "rounded-md border border-[var(--border)] px-3 py-1.5 text-sm";
  const error = save.error ?? remove.error ?? files.error ?? openFile.error;

  return (
    <aside
      className="fixed inset-y-0 right-0 z-20 w-full max-w-md space-y-4 overflow-y-auto border-l border-[var(--border)] bg-[var(--panel)] p-4 shadow-xl"
      aria-label={t("ideas.edit")}
    >
      <div className="flex items-center justify-between">
        <h2 className="font-medium">{t("ideas.edit")}</h2>
        <button className="text-sm" onClick={onClose} aria-label={t("accounts.dismiss")}>
          ✕
        </button>
      </div>
      <label className="block space-y-1 text-sm">
        <span>{t("ideas.title")}</span>
        <input className={field} value={title} onChange={(e) => setTitle(e.target.value)} />
      </label>
      <label className="block space-y-1 text-sm">
        <span>{t("ideas.notes")}</span>
        <textarea
          className={`${field} min-h-40 font-mono text-xs`}
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
        />
      </label>
      <fieldset className="space-y-1 text-sm">
        <legend>{t("ideas.targets")}</legend>
        <div className="flex flex-wrap gap-3">
          {platforms.data
            ?.filter((p) => p.id !== "mock")
            .map((p) => (
              <label key={p.id} className="flex items-center gap-1">
                <input
                  type="checkbox"
                  checked={targets.includes(p.id)}
                  onChange={(e) =>
                    setTargets((ts) =>
                      e.target.checked ? [...ts, p.id] : ts.filter((x) => x !== p.id),
                    )
                  }
                />
                {p.displayName}
              </label>
            ))}
        </div>
      </fieldset>
      <label className="block space-y-1 text-sm">
        <span>{t("ideas.due")}</span>
        <input
          type="datetime-local"
          className={field}
          value={due}
          onChange={(e) => setDue(e.target.value)}
        />
      </label>
      <div className="space-y-1 text-sm">
        <div className="flex items-center justify-between">
          <span>{t("ideas.files")}</span>
          <button className="text-xs underline" onClick={() => void pick()}>
            {t("ideas.attach")}
          </button>
        </div>
        {attached.length === 0 ? (
          <p className="text-xs text-[var(--muted)]">{t("ideas.noFiles")}</p>
        ) : (
          <ul className="space-y-1">
            {attached.map((f) => (
              <li key={f.id} className="flex items-center justify-between gap-2">
                <button
                  className="truncate text-left hover:underline"
                  title={f.path}
                  onClick={() => openFile.mutate(f.id)}
                >
                  {f.name}
                </button>
                <button
                  className="shrink-0 text-xs text-[var(--muted)] underline"
                  onClick={() => files.mutate({ detach: f.id })}
                >
                  {t("ideas.detach")}
                </button>
              </li>
            ))}
          </ul>
        )}
        <p className="text-xs text-[var(--muted)]">{t("ideas.filesNote")}</p>
      </div>
      {error && <p className="text-sm text-red-500">{errorMessage(error, t)}</p>}
      <div className="flex flex-wrap justify-between gap-2">
        {confirmDelete ? (
          <span className="flex items-center gap-2 text-sm">
            {t("ideas.confirmDelete")}
            <button
              className="rounded-md bg-red-600 px-3 py-1.5 text-white"
              onClick={() => remove.mutate()}
            >
              {t("ideas.delete")}
            </button>
            <button className={btn} onClick={() => setConfirmDelete(false)}>
              {t("accounts.cancel")}
            </button>
          </span>
        ) : (
          <button className={btn} onClick={() => setConfirmDelete(true)}>
            {t("ideas.delete")}
          </button>
        )}
        <button
          className="rounded-md bg-[var(--accent)] px-3 py-1.5 text-sm text-white disabled:opacity-50"
          disabled={!title.trim() || save.isPending}
          onClick={() => save.mutate()}
        >
          {t("ideas.save")}
        </button>
      </div>
    </aside>
  );
}
