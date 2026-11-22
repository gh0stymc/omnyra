import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { commands } from "../../lib/bindings";
import { errorMessage } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";

/** Repeated questions: answer once, or turn them into a video idea. */
export function QuestionClusters({ onPick }: { onPick: (clusterId: string) => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const clusters = useQuery({
    queryKey: queryKeys.clusters,
    queryFn: () => commands.listQuestionClusters(),
  });
  const toIdea = useMutation({
    mutationFn: (id: string) => commands.clusterToIdea(id),
    onSuccess: (ideaId) => {
      void qc.invalidateQueries({ queryKey: queryKeys.clusters });
      void qc.invalidateQueries({ queryKey: queryKeys.ideas });
      navigate(`/ideas?open=${ideaId}`);
    },
  });
  if (!clusters.data?.length) return null;
  const btn = "rounded-md border border-[var(--border)] px-2 py-1 text-xs";
  return (
    <section className="space-y-2 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-3">
      <h2 className="text-sm font-medium">{t("community.clustersTitle")}</h2>
      <ul className="space-y-2">
        {clusters.data.map((c) => (
          <li key={c.id} className="flex flex-wrap items-start justify-between gap-2 text-sm">
            <div className="min-w-0 flex-1">
              <p className="font-medium">
                {c.label}{" "}
                <span className="text-xs text-[var(--muted)]">
                  {t("community.clusterSize", { count: c.size })}
                </span>
              </p>
              <p className="truncate text-xs text-[var(--muted)]">
                {c.samples.slice(1).join(" · ")}
              </p>
            </div>
            <div className="flex gap-1">
              <button className={btn} onClick={() => onPick(c.id)}>
                {t("community.seeQuestions")}
              </button>
              <button
                className={btn}
                onClick={() => toIdea.mutate(c.id)}
                disabled={toIdea.isPending}
              >
                {c.ideaId ? t("community.openIdea") : t("community.toIdea")}
              </button>
            </div>
          </li>
        ))}
      </ul>
      {toIdea.isError && <p className="text-xs text-red-500">{errorMessage(toIdea.error, t)}</p>}
    </section>
  );
}
