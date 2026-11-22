import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Account, type CommentFilter, type CommentRow } from "../../lib/bindings";
import { errorMessage } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";
import { useDebounced } from "../../lib/useDebounced";
import { QuestionClusters } from "./QuestionClusters";

const STATUSES = ["pending", "replied", "archived"] as const;
const CATEGORIES = ["question", "positive", "negative", "spam", "neutral"] as const;

const field = "rounded-md border border-[var(--border)] bg-[var(--panel)] px-2 py-1 text-sm";
const btn = "rounded-md border border-[var(--border)] px-2 py-1 text-xs disabled:opacity-50";
const CATEGORY_TONE: Record<string, string> = {
  question: "border-sky-500/50 text-sky-600",
  positive: "border-green-500/50 text-green-600",
  negative: "border-orange-500/50 text-orange-600",
  spam: "border-red-500/50 text-red-600",
  neutral: "border-[var(--border)] text-[var(--muted)]",
};

function CommentCard({ c, account }: { c: CommentRow; account?: Account }) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const [replying, setReplying] = useState(false);
  const [text, setText] = useState("");
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["inbox"] });
    void qc.invalidateQueries({ queryKey: queryKeys.inboxCounts });
  };
  const scopes = useQuery({
    queryKey: ["commentScopes", c.accountId],
    queryFn: () => commands.commentScopes(c.accountId),
    enabled: replying,
    staleTime: Infinity,
  });
  const missing = (scopes.data ?? []).filter((s) => !account?.grantedScopes.includes(s));
  const authorize = useMutation({
    mutationFn: () => commands.authorizeComments(c.accountId),
    onSettled: () => void qc.invalidateQueries({ queryKey: queryKeys.accounts }),
  });
  const send = useMutation({
    mutationFn: () => commands.replyToComment(c.id, text),
    onSuccess: () => {
      setText("");
      setReplying(false);
      refresh();
    },
  });
  const status = useMutation({
    mutationFn: (s: string) => commands.setCommentStatus(c.id, s),
    onSettled: refresh,
  });
  const suspicious = (c.phishingScore ?? 0) >= 0.5;

  return (
    <li
      className={`space-y-2 rounded-lg border bg-[var(--panel)] p-3 ${suspicious ? "border-red-500/60" : "border-[var(--border)]"}`}
    >
      <div className="flex flex-wrap items-center gap-2 text-xs text-[var(--muted)]">
        <span className="font-medium text-[var(--text)]">{c.authorName ?? "?"}</span>
        <span>· {account?.displayName ?? c.platform}</span>
        {c.publishedAt && <span>· {new Date(c.publishedAt).toLocaleString(i18n.language)}</span>}
        {c.contentTitle && (
          <button
            className="truncate underline"
            onClick={() => c.contentUrl && void openUrl(c.contentUrl)}
          >
            · {c.contentTitle}
          </button>
        )}
        {c.category && (
          <span className={`rounded-full border px-1.5 ${CATEGORY_TONE[c.category] ?? ""}`}>
            {t(`community.category.${c.category}`)}
          </span>
        )}
        {suspicious && (
          <span
            className="rounded-full border border-red-500/60 px-1.5 text-red-600"
            title={c.phishingReasons
              .map((r) => t(`security.reason.${r}`, { defaultValue: r }))
              .join(", ")}
          >
            ⚠ {t("community.suspicious")}
          </span>
        )}
        <span className="ml-auto">{t(`community.status.${c.status}`)}</span>
      </div>
      <p className="whitespace-pre-wrap text-sm">{c.text}</p>
      {suspicious && c.phishingReasons.length > 0 && (
        <p className="text-xs text-red-600">
          {c.phishingReasons.map((r) => t(`security.reason.${r}`, { defaultValue: r })).join(" · ")}
        </p>
      )}
      {c.replies.length > 0 && (
        <ul className="space-y-1 border-l-2 border-[var(--border)] pl-3">
          {c.replies.map((r) => (
            <li key={r.id} className="text-xs">
              <span className={r.isOwn ? "font-medium text-[var(--accent)]" : "font-medium"}>
                {r.isOwn ? t("community.you") : (r.authorName ?? "?")}
              </span>{" "}
              {r.text}
            </li>
          ))}
        </ul>
      )}
      <div className="flex flex-wrap gap-1">
        <button className={btn} onClick={() => setReplying((v) => !v)}>
          {t("community.reply")}
        </button>
        {c.status !== "archived" ? (
          <button className={btn} onClick={() => status.mutate("archived")}>
            {t("community.archive")}
          </button>
        ) : (
          <button className={btn} onClick={() => status.mutate("pending")}>
            {t("community.unarchive")}
          </button>
        )}
        {c.status === "pending" && (
          <button className={btn} onClick={() => status.mutate("replied")}>
            {t("community.markReplied")}
          </button>
        )}
      </div>
      {replying && (
        <div className="space-y-1">
          {missing.length > 0 && (
            <div className="flex items-center gap-2 text-xs">
              <span>{t("community.needsScope")}</span>
              <button className={btn} onClick={() => authorize.mutate()}>
                {t("upload.authorize")}
              </button>
            </div>
          )}
          <textarea
            className={`${field} min-h-16 w-full`}
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={t("community.replyPlaceholder")}
          />
          <div className="flex items-center gap-2">
            <button
              className="rounded-md bg-[var(--accent)] px-3 py-1 text-xs text-white disabled:opacity-50"
              disabled={!text.trim() || send.isPending || missing.length > 0}
              onClick={() => send.mutate()}
            >
              {t("community.send")}
            </button>
            {send.isError && (
              <span className="text-xs text-red-500">{errorMessage(send.error, t)}</span>
            )}
            {authorize.isError && (
              <span className="text-xs text-red-500">{errorMessage(authorize.error, t)}</span>
            )}
          </div>
        </div>
      )}
    </li>
  );
}

/** Unified comment inbox: every network, filters, replies, repeated questions. */
export function CommunityPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [filter, setFilter] = useState<CommentFilter>({ status: "pending" });
  const [search, setSearch] = useState("");
  const debouncedSearch = useDebounced(search, 300);
  const effective = { ...filter, search: debouncedSearch || null };
  const accounts = useQuery({
    queryKey: queryKeys.accounts,
    queryFn: () => commands.listAccounts(),
  });
  const platforms = useQuery({
    queryKey: queryKeys.platforms,
    queryFn: () => commands.listPlatforms(),
  });
  const withComments = (accounts.data ?? []).filter(
    (a) =>
      a.status === "active" &&
      platforms.data?.find((p) => p.id === a.platform)?.capabilities.comments !== "none",
  );
  const inbox = useQuery({
    queryKey: ["inbox", effective],
    queryFn: () => commands.listInbox(effective),
  });
  const counts = useQuery({
    queryKey: queryKeys.inboxCounts,
    queryFn: () => commands.inboxCounts(),
  });
  const sync = useMutation({
    mutationFn: async () => {
      for (const a of withComments) await commands.syncComments(a.id);
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: ["inbox"] });
      void qc.invalidateQueries({ queryKey: queryKeys.inboxCounts });
      void qc.invalidateQueries({ queryKey: queryKeys.clusters });
    },
  });
  const set = (k: keyof CommentFilter, v: string) =>
    setFilter((f) => ({ ...f, [k]: v === "" ? null : v }));
  const networks = [...new Set(withComments.map((a) => a.platform))];

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-2xl font-semibold">{t("community.pageTitle")}</h1>
        <div className="flex items-center gap-2 text-sm">
          {counts.data && (
            <span className="text-[var(--muted)]">
              {t("community.counts", {
                pending: counts.data.pending,
                questions: counts.data.questions,
                suspicious: counts.data.suspicious,
              })}
            </span>
          )}
          <button
            className="rounded-md bg-[var(--accent)] px-3 py-1.5 text-sm text-white disabled:opacity-50"
            disabled={sync.isPending || !withComments.length}
            onClick={() => sync.mutate()}
          >
            {sync.isPending ? t("community.syncing") : t("community.sync")}
          </button>
        </div>
      </div>
      {sync.isError && <p className="text-sm text-red-500">{errorMessage(sync.error, t)}</p>}
      {!withComments.length && (
        <p className="text-sm text-[var(--muted)]">{t("community.noAccounts")}</p>
      )}

      <QuestionClusters onPick={(id) => setFilter({ clusterId: id })} />

      <div className="flex flex-wrap gap-2">
        <input
          className={field}
          placeholder={t("community.search")}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <select
          className={field}
          value={filter.platform ?? ""}
          onChange={(e) => set("platform", e.target.value)}
        >
          <option value="">{t("calendar.allNetworks")}</option>
          {networks.map((p) => (
            <option key={p} value={p}>
              {platforms.data?.find((x) => x.id === p)?.displayName ?? p}
            </option>
          ))}
        </select>
        <select
          className={field}
          value={filter.accountId ?? ""}
          onChange={(e) => set("accountId", e.target.value)}
        >
          <option value="">{t("community.allAccounts")}</option>
          {withComments.map((a) => (
            <option key={a.id} value={a.id}>
              {a.displayName}
            </option>
          ))}
        </select>
        <select
          className={field}
          value={filter.status ?? ""}
          onChange={(e) => set("status", e.target.value)}
        >
          <option value="">{t("community.allStatuses")}</option>
          {STATUSES.map((s) => (
            <option key={s} value={s}>
              {t(`community.status.${s}`)}
            </option>
          ))}
        </select>
        <select
          className={field}
          value={filter.category ?? ""}
          onChange={(e) => set("category", e.target.value)}
        >
          <option value="">{t("community.allCategories")}</option>
          {CATEGORIES.map((c) => (
            <option key={c} value={c}>
              {t(`community.category.${c}`)}
            </option>
          ))}
        </select>
        <label className="flex items-center gap-1 text-sm">
          <input
            type="checkbox"
            checked={!!filter.suspicious}
            onChange={(e) => setFilter((f) => ({ ...f, suspicious: e.target.checked || null }))}
          />
          {t("community.onlySuspicious")}
        </label>
        {filter.clusterId && (
          <button className={btn} onClick={() => setFilter((f) => ({ ...f, clusterId: null }))}>
            {t("community.clearCluster")} ✕
          </button>
        )}
      </div>

      {inbox.isPending ? (
        <p className="text-sm text-[var(--muted)]">{t("common.loading")}</p>
      ) : inbox.isError ? (
        <p className="text-sm text-red-500">{errorMessage(inbox.error, t)}</p>
      ) : inbox.data.length === 0 ? (
        <p className="text-sm text-[var(--muted)]">{t("community.empty")}</p>
      ) : (
        <ul className="space-y-2">
          {inbox.data.map((c) => (
            <CommentCard
              key={c.id}
              c={c}
              account={accounts.data?.find((a) => a.id === c.accountId)}
            />
          ))}
        </ul>
      )}
    </div>
  );
}
