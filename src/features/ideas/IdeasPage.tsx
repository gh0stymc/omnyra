import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type DragEvent } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router";
import { commands, type Idea } from "../../lib/bindings";
import { errorMessage } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";
import { IdeaEditor } from "./IdeaEditor";

export const STAGES = ["idea", "script", "recorded", "edited", "published"] as const;
const DRAG_TYPE = "application/x-omnyra-idea";

/** Kanban: Idea → Script → Recorded → Edited → Published. */
export function IdeasPage() {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const [params, setParams] = useSearchParams();
  const [adding, setAdding] = useState<string | null>(null);
  const [newTitle, setNewTitle] = useState("");
  const [over, setOver] = useState<string | null>(null);
  const ideas = useQuery({ queryKey: queryKeys.ideas, queryFn: () => commands.listIdeas() });
  const platforms = useQuery({
    queryKey: queryKeys.platforms,
    queryFn: () => commands.listPlatforms(),
  });
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: queryKeys.ideas });
    void qc.invalidateQueries({ queryKey: ["calendar"] });
  };
  const create = useMutation({
    mutationFn: (stage: string) =>
      commands.createIdea(stage, {
        title: newTitle,
        notes: null,
        targetPlatforms: [],
        dueAt: null,
      }),
    onSuccess: () => {
      setNewTitle("");
      setAdding(null);
      refresh();
    },
  });
  const move = useMutation({
    mutationFn: (m: { id: string; stage: string; index: number }) =>
      commands.moveIdea(m.id, m.stage, m.index),
    // Optimistic: reorder locally right away, then refetch.
    onMutate: (m) => {
      qc.setQueryData<Idea[]>(queryKeys.ideas, (old) => {
        if (!old) return old;
        const moved = old.find((i) => i.id === m.id);
        if (!moved) return old;
        const rest = old.filter((i) => i.id !== m.id);
        const col = rest.filter((i) => i.stage === m.stage);
        const others = rest.filter((i) => i.stage !== m.stage);
        col.splice(Math.min(m.index, col.length), 0, { ...moved, stage: m.stage });
        return [...others, ...col.map((c, i) => ({ ...c, position: i + 1 }))];
      });
    },
    onSettled: refresh,
  });

  const openId = params.get("open");
  const editing = ideas.data?.find((i) => i.id === openId) ?? null;
  const setEditing = (id: string | null) => setParams(id ? { open: id } : {});
  const column = (stage: string) =>
    (ideas.data ?? [])
      .filter((i) => i.stage === stage)
      .sort((a, b) => (a.position ?? 0) - (b.position ?? 0));
  const brand = (p: string) => platforms.data?.find((x) => x.id === p);

  const drop = (ev: DragEvent, stage: string, index: number) => {
    const id = ev.dataTransfer.getData(DRAG_TYPE);
    setOver(null);
    if (!id) return;
    ev.preventDefault();
    ev.stopPropagation();
    // Dropping below itself in the same column: account for its own slot.
    const col = column(stage);
    const from = col.findIndex((i) => i.id === id);
    move.mutate({ id, stage, index: from >= 0 && from < index ? index - 1 : index });
  };
  const allow = (ev: DragEvent, stage: string) => {
    if (ev.dataTransfer.types.includes(DRAG_TYPE)) {
      ev.preventDefault();
      setOver(stage);
    }
  };

  if (ideas.isPending) return <p className="text-[var(--muted)]">{t("common.loading")}</p>;
  if (ideas.isError) return <p className="text-red-500">{errorMessage(ideas.error, t)}</p>;

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold">{t("ideas.pageTitle")}</h1>
      {(create.isError || move.isError) && (
        <p className="text-sm text-red-500">{errorMessage(create.error ?? move.error, t)}</p>
      )}
      <div
        className="grid gap-3 overflow-x-auto"
        style={{ gridTemplateColumns: `repeat(${STAGES.length}, minmax(12rem, 1fr))` }}
      >
        {STAGES.map((stage) => {
          const cards = column(stage);
          return (
            <section
              key={stage}
              onDragOver={(ev) => allow(ev, stage)}
              onDragLeave={() => setOver((o) => (o === stage ? null : o))}
              onDrop={(ev) => drop(ev, stage, cards.length)}
              className={`flex min-h-64 flex-col gap-2 rounded-lg border p-2 ${over === stage ? "border-[var(--accent)]" : "border-[var(--border)]"} bg-[var(--panel)]`}
              aria-label={t(`ideas.stage.${stage}`)}
            >
              <h2 className="flex justify-between px-1 text-sm font-medium">
                {t(`ideas.stage.${stage}`)}
                <span className="text-[var(--muted)]">{cards.length}</span>
              </h2>
              {cards.map((c, index) => (
                <article
                  key={c.id}
                  draggable
                  onDragStart={(ev) => {
                    ev.dataTransfer.setData(DRAG_TYPE, c.id);
                    ev.dataTransfer.effectAllowed = "move";
                  }}
                  onDragOver={(ev) => allow(ev, stage)}
                  onDrop={(ev) => drop(ev, stage, index)}
                  onClick={() => setEditing(c.id)}
                  className="cursor-grab space-y-1 rounded-md border border-[var(--border)] bg-[var(--bg)] p-2 text-sm hover:border-[var(--accent)]"
                >
                  <p className="font-medium">{c.title}</p>
                  {c.notes && <p className="line-clamp-2 text-xs text-[var(--muted)]">{c.notes}</p>}
                  <div className="flex flex-wrap items-center gap-1 text-[11px] text-[var(--muted)]">
                    {c.targetPlatforms.map((p) => (
                      <span
                        key={p}
                        className="flex items-center gap-1 rounded-full border border-[var(--border)] px-1.5"
                      >
                        <span
                          className="size-1.5 rounded-full"
                          style={{ background: brand(p)?.brandColor ?? "#888" }}
                        />
                        {brand(p)?.displayName ?? p}
                      </span>
                    ))}
                    {c.dueAt && (
                      <span>📅 {new Date(c.dueAt).toLocaleDateString(i18n.language)}</span>
                    )}
                    {c.files.length > 0 && <span>📎 {c.files.length}</span>}
                  </div>
                </article>
              ))}
              {adding === stage ? (
                <form
                  className="space-y-1"
                  onSubmit={(ev) => {
                    ev.preventDefault();
                    if (newTitle.trim()) create.mutate(stage);
                  }}
                >
                  <input
                    autoFocus
                    className="w-full rounded-md border border-[var(--border)] bg-[var(--bg)] px-2 py-1 text-sm"
                    placeholder={t("ideas.newPlaceholder")}
                    value={newTitle}
                    onChange={(e) => setNewTitle(e.target.value)}
                    onKeyDown={(e) => e.key === "Escape" && setAdding(null)}
                  />
                  <div className="flex gap-1">
                    <button
                      className="rounded-md bg-[var(--accent)] px-2 py-1 text-xs text-white"
                      disabled={!newTitle.trim()}
                    >
                      {t("ideas.add")}
                    </button>
                    <button
                      type="button"
                      className="px-2 py-1 text-xs"
                      onClick={() => setAdding(null)}
                    >
                      {t("accounts.cancel")}
                    </button>
                  </div>
                </form>
              ) : (
                <button
                  className="mt-auto rounded-md px-2 py-1 text-left text-xs text-[var(--muted)] hover:bg-[var(--border)]"
                  onClick={() => {
                    setAdding(stage);
                    setNewTitle("");
                  }}
                >
                  + {t("ideas.add")}
                </button>
              )}
            </section>
          );
        })}
      </div>
      {editing && <IdeaEditor key={editing.id} idea={editing} onClose={() => setEditing(null)} />}
    </div>
  );
}
