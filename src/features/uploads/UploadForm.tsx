import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Account, type PlatformInfo, type UploadDraft } from "../../lib/bindings";
import { errorMessage } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";
import { useDebounced } from "../../lib/useDebounced";
import { issueMessage } from "./issues";

const VIDEO_EXTENSIONS = [
  "mp4",
  "mov",
  "m4v",
  "avi",
  "wmv",
  "flv",
  "webm",
  "mkv",
  "mpg",
  "mpeg",
  "3gp",
  "3g2",
  "mts",
  "m2ts",
  "ts",
  "ogv",
];
const VISIBILITIES = ["private", "unlisted", "public"] as const;
/** Gaming: the most likely default for this app's users; falls back to the first category. */
const PREFERRED_CATEGORY = "20";

const empty = (): Omit<UploadDraft, "madeForKids"> & { madeForKids: boolean | null } => ({
  filePath: "",
  title: "",
  description: "",
  tags: [],
  visibility: "private",
  categoryId: null,
  playlistId: null,
  thumbnailPath: null,
  madeForKids: null,
});

const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;

export function UploadForm({
  accounts,
  platforms,
  initialAccountId,
}: {
  accounts: Account[];
  platforms: PlatformInfo[];
  initialAccountId?: string;
}) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const [accountId, setAccountId] = useState(initialAccountId ?? accounts[0]?.id ?? "");
  const [form, setForm] = useState(empty);
  const [tagsText, setTagsText] = useState("");
  const account = accounts.find((a) => a.id === accountId);
  const platform = platforms.find((p) => p.id === account?.platform);
  const set = <K extends keyof typeof form>(k: K, v: (typeof form)[K]) =>
    setForm((f) => ({ ...f, [k]: v }));

  const options = useQuery({
    queryKey: ["uploadOptions", accountId, i18n.language],
    queryFn: () => commands.getUploadOptions(accountId, i18n.language),
    enabled: !!accountId,
    staleTime: Infinity,
  });
  // Default category until the user picks one: Gaming if offered, else the first.
  const cats = options.data?.categories ?? [];
  const categoryId = cats.some((c) => c.id === form.categoryId)
    ? form.categoryId
    : ((cats.find((c) => c.id === PREFERRED_CATEGORY) ?? cats[0])?.id ?? null);

  const scopes = useQuery({
    queryKey: ["uploadScopes", accountId, !!form.playlistId],
    queryFn: () => commands.uploadScopes(accountId, !!form.playlistId),
    enabled: !!accountId,
  });
  const missingScopes = (scopes.data ?? []).filter((s) => !account?.grantedScopes.includes(s));
  const authorize = useMutation({
    mutationFn: () => commands.authorizeUpload(accountId, !!form.playlistId),
    onSettled: () => qc.invalidateQueries({ queryKey: queryKeys.accounts }),
  });

  const draft: UploadDraft = useMemo(
    () => ({
      ...form,
      categoryId,
      tags: tagsText
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean),
      madeForKids: form.madeForKids ?? false,
    }),
    [form, tagsText, categoryId],
  );
  const debounced = useDebounced(draft, 400);
  const validation = useQuery({
    queryKey: ["validateUpload", accountId, debounced],
    queryFn: () => commands.validateUpload(accountId, debounced),
    enabled: !!accountId && !!debounced.filePath,
  });
  const issues = validation.data?.issues ?? [];
  const hasErrors = issues.some((i) => i.severity === "error");

  const enqueue = useMutation({
    mutationFn: () => commands.enqueueUpload(accountId, draft),
    onSuccess: () => {
      setForm(empty());
      setTagsText("");
      void qc.invalidateQueries({ queryKey: queryKeys.uploads });
    },
  });

  const pickVideo = async () => {
    const p = await open({
      multiple: false,
      directory: false,
      filters: [{ name: t("upload.videoFiles"), extensions: VIDEO_EXTENSIONS }],
    });
    if (typeof p === "string") {
      setForm((f) => ({
        ...f,
        filePath: p,
        // Default title: the file name without extension.
        title: f.title || fileName(p).replace(/\.[^.]+$/, ""),
      }));
    }
  };
  const pickThumb = async () => {
    const p = await open({
      multiple: false,
      directory: false,
      filters: [{ name: t("upload.imageFiles"), extensions: ["jpg", "jpeg", "png"] }],
    });
    if (typeof p === "string") set("thumbnailPath", p);
  };

  const field = "w-full rounded-md border border-[var(--border)] bg-[var(--panel)] px-3 py-1.5";
  const btn = "rounded-md border border-[var(--border)] px-3 py-1.5 text-sm";
  const primary =
    "rounded-md bg-[var(--accent)] px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50";
  const media = validation.data?.media;
  const size = validation.data?.sizeBytes;
  const canSubmit =
    !!form.filePath &&
    form.madeForKids !== null &&
    !hasErrors &&
    missingScopes.length === 0 &&
    validation.isSuccess &&
    !validation.isFetching &&
    !enqueue.isPending;

  if (!accounts.length) return <p className="text-[var(--muted)]">{t("upload.noAccounts")}</p>;

  return (
    <section className="space-y-4 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-4">
      <h2 className="font-medium">{t("upload.newTitle")}</h2>

      <label className="block space-y-1 text-sm">
        <span>{t("upload.account")}</span>
        <select className={field} value={accountId} onChange={(e) => setAccountId(e.target.value)}>
          {accounts.map((a) => (
            <option key={a.id} value={a.id}>
              {a.displayName} ({a.platform})
            </option>
          ))}
        </select>
      </label>

      {platform?.capabilities.upload === "privateOnlyUnaudited" && (
        <p className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm">
          {t("upload.auditWarning")}
        </p>
      )}

      {missingScopes.length > 0 && (
        <div className="space-y-2 rounded-md border border-[var(--border)] p-3 text-sm">
          <p>{t(form.playlistId ? "upload.needsPlaylistScope" : "upload.needsScope")}</p>
          <button
            className={primary}
            onClick={() => authorize.mutate()}
            disabled={authorize.isPending}
          >
            {t("upload.authorize")}
          </button>
          {authorize.isError && <p className="text-red-500">{errorMessage(authorize.error, t)}</p>}
        </div>
      )}

      <div className="space-y-1 text-sm">
        <div className="flex flex-wrap items-center gap-2">
          <button className={btn} onClick={() => void pickVideo()}>
            {t("upload.pickVideo")}
          </button>
          {form.filePath && <span className="break-all">{fileName(form.filePath)}</span>}
        </div>
        {media && (
          <p className="text-xs text-[var(--muted)]">
            {[
              media.durationS != null && `${Math.round(media.durationS)} s`,
              media.width && media.height && `${media.width}×${media.height}`,
              media.videoCodec,
              size != null && `${(size / 1024 ** 2).toFixed(1)} MB`,
            ]
              .filter(Boolean)
              .join(" · ")}
          </p>
        )}
      </div>

      <label className="block space-y-1 text-sm">
        <span className="flex justify-between">
          {t("upload.title")}{" "}
          <span className="text-[var(--muted)]">{[...form.title].length}/100</span>
        </span>
        <input
          className={field}
          value={form.title}
          onChange={(e) => set("title", e.target.value)}
        />
      </label>

      <label className="block space-y-1 text-sm">
        <span className="flex justify-between">
          {t("upload.description")}
          <span className="text-[var(--muted)]">
            {new TextEncoder().encode(form.description).length}/5000
          </span>
        </span>
        <textarea
          className={`${field} min-h-24`}
          value={form.description}
          onChange={(e) => set("description", e.target.value)}
        />
      </label>

      <label className="block space-y-1 text-sm">
        <span>{t("upload.tags")}</span>
        <input
          className={field}
          value={tagsText}
          placeholder={t("upload.tagsPlaceholder")}
          onChange={(e) => setTagsText(e.target.value)}
        />
      </label>

      <div className="grid gap-3 sm:grid-cols-3">
        <label className="block space-y-1 text-sm">
          <span>{t("upload.visibility")}</span>
          <select
            className={field}
            value={form.visibility}
            onChange={(e) => set("visibility", e.target.value)}
          >
            {VISIBILITIES.map((v) => (
              <option key={v} value={v}>
                {t(`upload.visibilityOption.${v}`)}
              </option>
            ))}
          </select>
        </label>
        <label className="block space-y-1 text-sm">
          <span>{t("upload.category")}</span>
          <select
            className={field}
            value={categoryId ?? ""}
            onChange={(e) => set("categoryId", e.target.value || null)}
          >
            {options.data?.categories.map((c) => (
              <option key={c.id} value={c.id}>
                {c.title}
              </option>
            ))}
          </select>
        </label>
        <label className="block space-y-1 text-sm">
          <span>{t("upload.playlist")}</span>
          <select
            className={field}
            value={form.playlistId ?? ""}
            onChange={(e) => set("playlistId", e.target.value || null)}
          >
            <option value="">{t("upload.noPlaylist")}</option>
            {options.data?.playlists.map((p) => (
              <option key={p.id} value={p.id}>
                {p.title}
              </option>
            ))}
          </select>
        </label>
      </div>
      {options.isError && <p className="text-sm text-red-500">{errorMessage(options.error, t)}</p>}

      <div className="flex flex-wrap items-center gap-2 text-sm">
        <button className={btn} onClick={() => void pickThumb()}>
          {t("upload.pickThumbnail")}
        </button>
        {form.thumbnailPath ? (
          <>
            <span className="break-all">{fileName(form.thumbnailPath)}</span>
            <button className="text-xs underline" onClick={() => set("thumbnailPath", null)}>
              {t("upload.removeThumbnail")}
            </button>
          </>
        ) : (
          <span className="text-[var(--muted)]">{t("upload.thumbnailOptional")}</span>
        )}
      </div>

      <fieldset className="space-y-1 text-sm">
        <legend>{t("upload.madeForKids")}</legend>
        <div className="flex gap-4">
          {[false, true].map((v) => (
            <label key={String(v)} className="flex items-center gap-1">
              <input
                type="radio"
                name="madeForKids"
                checked={form.madeForKids === v}
                onChange={() => set("madeForKids", v)}
              />
              {t(v ? "upload.kidsYes" : "upload.kidsNo")}
            </label>
          ))}
        </div>
      </fieldset>

      {issues.length > 0 && (
        <ul className="space-y-1 text-sm">
          {issues.map((i) => (
            <li
              key={`${i.field}.${i.code}`}
              className={i.severity === "error" ? "text-red-500" : "text-amber-500"}
            >
              {issueMessage(i, t, i18n.language)}
            </li>
          ))}
        </ul>
      )}

      <div className="flex items-center gap-3">
        <button className={primary} disabled={!canSubmit} onClick={() => enqueue.mutate()}>
          {t("upload.enqueue")}
        </button>
        {validation.isFetching && (
          <span className="text-xs text-[var(--muted)]">{t("upload.checking")}</span>
        )}
        {enqueue.isError && (
          <span className="text-sm text-red-500">{errorMessage(enqueue.error, t)}</span>
        )}
      </div>
    </section>
  );
}
