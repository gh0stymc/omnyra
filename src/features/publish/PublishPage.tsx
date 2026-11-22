import { useMutation, useQueries, useQuery } from "@tanstack/react-query";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router";
import {
  commands,
  events,
  type Aspect,
  type Fit,
  type PlatformInfo,
  type RenditionSpec,
} from "../../lib/bindings";
import { errorMessage } from "../../lib/ipc";
import { queryKeys } from "../../lib/queryKeys";
import { useDebounced } from "../../lib/useDebounced";
import { issueMessage } from "../uploads/issues";
import {
  DEFAULT_VISIBILITY,
  captionOf,
  draftFor,
  isComplete,
  localToIso,
  type CommonText,
  type TargetState,
} from "./targets";

const ASPECTS: Aspect[] = ["original", "landscape", "vertical", "square"];
const FITS: Fit[] = ["crop", "pad", "blur"];
const VISIBILITIES: Record<string, string[]> = {
  youtube: ["private", "unlisted", "public"],
  tiktok: ["private", "friends", "followers", "public"],
  instagram: [],
};
const VIDEO_EXT = ["mp4", "mov", "m4v", "mkv", "webm", "avi"];

const field = "w-full rounded-md border border-[var(--border)] bg-[var(--bg)] px-2 py-1 text-sm";
const btn = "rounded-md border border-[var(--border)] px-3 py-1.5 text-sm disabled:opacity-50";
const primary =
  "rounded-md bg-[var(--accent)] px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50";

/** Live FFmpeg progress of an export, by key. */
function useRenderProgress() {
  const [p, setP] = useState<Record<string, number>>({});
  useEffect(() => {
    const sub = events.renderProgress
      .listen(({ payload }) => setP((x) => ({ ...x, [payload.key]: payload.progress ?? 0 })))
      .catch(() => undefined);
    return () => void sub.then((f) => f?.());
  }, []);
  return p;
}

function RenderFields({
  value,
  onChange,
}: {
  value: RenditionSpec;
  onChange: (r: RenditionSpec) => void;
}) {
  const { t } = useTranslation();
  const num = (s: string) => (s === "" ? null : Math.max(0, Number(s)));
  return (
    <div className="grid gap-2 sm:grid-cols-4">
      <label className="space-y-1 text-xs">
        <span>{t("publish.aspect")}</span>
        <select
          className={field}
          value={value.aspect}
          onChange={(e) => onChange({ ...value, aspect: e.target.value as Aspect })}
        >
          {ASPECTS.map((a) => (
            <option key={a} value={a}>
              {t(`publish.aspectOption.${a}`)}
            </option>
          ))}
        </select>
      </label>
      <label className="space-y-1 text-xs">
        <span>{t("publish.fit")}</span>
        <select
          className={field}
          value={value.fit}
          disabled={value.aspect === "original"}
          onChange={(e) => onChange({ ...value, fit: e.target.value as Fit })}
        >
          {FITS.map((f) => (
            <option key={f} value={f}>
              {t(`publish.fitOption.${f}`)}
            </option>
          ))}
        </select>
      </label>
      <label className="space-y-1 text-xs">
        <span>{t("publish.start")}</span>
        <input
          type="number"
          min={0}
          step="0.1"
          className={field}
          value={value.startS ?? ""}
          onChange={(e) => onChange({ ...value, startS: num(e.target.value) })}
        />
      </label>
      <label className="space-y-1 text-xs">
        <span>{t("publish.end")}</span>
        <input
          type="number"
          min={0}
          step="0.1"
          className={field}
          value={value.endS ?? ""}
          onChange={(e) => onChange({ ...value, endS: num(e.target.value) })}
        />
      </label>
    </div>
  );
}

function TextFields({ value, onChange }: { value: CommonText; onChange: (v: CommonText) => void }) {
  const { t } = useTranslation();
  return (
    <div className="space-y-2">
      <input
        className={field}
        placeholder={t("upload.title")}
        value={value.title}
        onChange={(e) => onChange({ ...value, title: e.target.value })}
      />
      <textarea
        className={`${field} min-h-20`}
        placeholder={t("upload.description")}
        value={value.description}
        onChange={(e) => onChange({ ...value, description: e.target.value })}
      />
      <input
        className={field}
        placeholder={t("upload.tagsPlaceholder")}
        value={value.tags}
        onChange={(e) => onChange({ ...value, tags: e.target.value })}
      />
    </div>
  );
}

function TargetCard({
  target,
  info,
  name,
  common,
  filePath,
  onChange,
  onRemove,
}: {
  target: TargetState;
  info?: PlatformInfo;
  name: string;
  common: CommonText;
  filePath: string;
  onChange: (t: TargetState) => void;
  onRemove: () => void;
}) {
  const { t, i18n } = useTranslation();
  const debounced = useDebounced(target, 400);
  const validation = useQuery({
    queryKey: ["validateTarget", debounced, common, filePath],
    queryFn: () =>
      commands.validateTarget({
        accountId: debounced.accountId,
        draft: draftFor(debounced, common, filePath),
        render: debounced.render,
        scheduledAt: localToIso(debounced.scheduleLocal),
      }),
    enabled: !!filePath,
  });
  const vis = VISIBILITIES[target.platform] ?? VISIBILITIES.youtube;
  const text = target.custom ? target.text : common;
  return (
    <section className="space-y-3 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-3">
      <header className="flex items-center justify-between">
        <h3 className="flex items-center gap-2 text-sm font-medium">
          <span className="size-2 rounded-full" style={{ background: info?.brandColor }} />
          {name} · {info?.displayName}
        </h3>
        <button className="text-xs underline" onClick={onRemove}>
          {t("publish.removeTarget")}
        </button>
      </header>
      <label className="flex items-center gap-2 text-xs">
        <input
          type="checkbox"
          checked={target.custom}
          onChange={(e) =>
            onChange({
              ...target,
              custom: e.target.checked,
              text: target.custom ? target.text : common,
            })
          }
        />
        {t("publish.customText")}
      </label>
      {target.custom && (
        <TextFields value={target.text} onChange={(v) => onChange({ ...target, text: v })} />
      )}
      {target.platform !== "youtube" && (
        <pre className="max-h-32 overflow-auto whitespace-pre-wrap rounded-md bg-[var(--bg)] p-2 text-xs text-[var(--muted)]">
          {captionOf(text) || t("publish.emptyCaption")}
        </pre>
      )}
      <RenderFields value={target.render} onChange={(r) => onChange({ ...target, render: r })} />
      <div className="grid gap-2 sm:grid-cols-3">
        {vis.length > 0 && (
          <label className="space-y-1 text-xs">
            <span>{t("upload.visibility")}</span>
            <select
              className={field}
              value={target.visibility}
              onChange={(e) => onChange({ ...target, visibility: e.target.value })}
            >
              {vis.map((v) => (
                <option key={v} value={v}>
                  {t(`upload.visibilityOption.${v}`)}
                </option>
              ))}
            </select>
          </label>
        )}
        <label className="space-y-1 text-xs">
          <span>{t("publish.schedule")}</span>
          <input
            type="datetime-local"
            className={field}
            value={target.scheduleLocal}
            onChange={(e) => onChange({ ...target, scheduleLocal: e.target.value })}
          />
        </label>
        {target.platform === "youtube" && (
          <fieldset className="space-y-1 text-xs">
            <legend>{t("upload.madeForKids")}</legend>
            <div className="flex gap-3">
              {[false, true].map((v) => (
                <label key={String(v)} className="flex items-center gap-1">
                  <input
                    type="radio"
                    checked={target.madeForKids === v}
                    onChange={() => onChange({ ...target, madeForKids: v })}
                  />
                  {t(v ? "upload.kidsYes" : "upload.kidsNo")}
                </label>
              ))}
            </div>
          </fieldset>
        )}
      </div>
      {validation.data?.issues.length ? (
        <ul className="space-y-0.5 text-xs">
          {validation.data.issues.map((i) => (
            <li
              key={`${i.field}.${i.code}`}
              className={i.severity === "error" ? "text-red-500" : "text-amber-500"}
            >
              {issueMessage(i, t, i18n.language)}
            </li>
          ))}
        </ul>
      ) : null}
    </section>
  );
}

/** "Publish to several networks": one video, several destinations, one adaptation each. */
export function PublishPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  // From the moments tool: a clip that was just cut.
  const [filePath, setFilePath] = useState(params.get("file") ?? "");
  const [common, setCommon] = useState<CommonText>({ title: "", description: "", tags: "" });
  const [targets, setTargets] = useState<TargetState[]>([]);
  const [exportSpec, setExportSpec] = useState<RenditionSpec>({
    aspect: "vertical",
    fit: "crop",
    startS: null,
    endS: null,
    maxDurationS: null,
    maxSizeMb: null,
  });
  const [copied, setCopied] = useState(false);
  const progress = useRenderProgress();
  const accounts = useQuery({
    queryKey: queryKeys.accounts,
    queryFn: () => commands.listAccounts(),
  });
  const platforms = useQuery({
    queryKey: queryKeys.platforms,
    queryFn: () => commands.listPlatforms(),
  });
  const info = (id: string) => platforms.data?.find((p) => p.id === id);
  const usable = (accounts.data ?? []).filter(
    (a) => a.status === "active" && info(a.platform)?.capabilities.upload !== "none",
  );

  const addTarget = async (accountId: string) => {
    const a = usable.find((x) => x.id === accountId);
    if (!a || targets.some((x) => x.accountId === accountId)) return;
    const render = await commands.renditionPreset(a.platform);
    setTargets((ts) => [
      ...ts,
      {
        accountId,
        platform: a.platform,
        custom: false,
        text: common,
        visibility: DEFAULT_VISIBILITY[a.platform] ?? "private",
        madeForKids: null,
        render,
        scheduleLocal: "",
      },
    ]);
  };

  // Scopes each destination still needs, asked to the backend (REVIEW C1).
  const needed = useQueries({
    queries: targets.map((tg) => ({
      queryKey: ["uploadScopes", tg.accountId, false],
      queryFn: () => commands.uploadScopes(tg.accountId, false),
    })),
  });
  const missingScopes = (accountId: string) => {
    const i = targets.findIndex((x) => x.accountId === accountId);
    const granted = accounts.data?.find((x) => x.id === accountId)?.grantedScopes ?? [];
    return (needed[i]?.data ?? []).some((s) => !granted.includes(s));
  };
  const authorize = useMutation({
    mutationFn: (accountId: string) => commands.authorizeUpload(accountId, false),
    onSettled: () => void accounts.refetch(),
  });

  const publish = useMutation({
    mutationFn: () =>
      commands.publishBatch(
        targets.map((tg) => ({
          accountId: tg.accountId,
          draft: draftFor(tg, common, filePath),
          render: tg.render,
          scheduledAt: localToIso(tg.scheduleLocal),
        })),
      ),
    onSuccess: () => navigate("/uploads"),
  });

  const exportKey = "export";
  const doExport = useMutation({
    mutationFn: async () => {
      const dest = await save({
        defaultPath: filePath.replace(/\.[^./\\]+$/, "") + `-${exportSpec.aspect}.mp4`,
        filters: [{ name: "MP4", extensions: ["mp4"] }],
      });
      if (!dest) return false;
      await commands.exportRendition(filePath, exportSpec, dest, exportKey);
      return true;
    },
  });
  const copyCaption = async () => {
    await navigator.clipboard.writeText(captionOf(common));
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const pick = async () => {
    const p = await open({
      multiple: false,
      filters: [{ name: t("upload.videoFiles"), extensions: VIDEO_EXT }],
    });
    if (typeof p === "string") {
      setFilePath(p);
      setCommon((c) => ({
        ...c,
        title: c.title || (p.split(/[\\/]/).pop() ?? "").replace(/\.[^.]+$/, ""),
      }));
    }
  };

  const canPublish =
    !!filePath &&
    targets.length > 0 &&
    targets.every(isComplete) &&
    !targets.some((tg) => missingScopes(tg.accountId)) &&
    !publish.isPending;

  return (
    <div className="max-w-4xl space-y-6">
      <h1 className="text-2xl font-semibold">{t("publish.pageTitle")}</h1>
      <p className="text-sm text-[var(--muted)]">{t("publish.intro")}</p>

      <section className="space-y-3 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-4">
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <button className={btn} onClick={() => void pick()}>
            {t("upload.pickVideo")}
          </button>
          {filePath && <span className="break-all">{filePath.split(/[\\/]/).pop()}</span>}
        </div>
        <TextFields value={common} onChange={setCommon} />
      </section>

      <section className="space-y-3">
        <h2 className="font-medium">{t("publish.destinations")}</h2>
        <div className="flex flex-wrap gap-2">
          {usable.map((a) => (
            <button
              key={a.id}
              className={btn}
              disabled={targets.some((x) => x.accountId === a.id)}
              onClick={() => void addTarget(a.id)}
            >
              + {a.displayName} ({info(a.platform)?.displayName ?? a.platform})
            </button>
          ))}
          {!usable.length && (
            <p className="text-sm text-[var(--muted)]">{t("upload.noAccounts")}</p>
          )}
        </div>
        {targets.map((tg, i) => (
          <div key={tg.accountId} className="space-y-1">
            {missingScopes(tg.accountId) && (
              <div className="flex items-center gap-2 text-xs">
                <span>{t("upload.needsScope")}</span>
                <button className={btn} onClick={() => authorize.mutate(tg.accountId)}>
                  {t("upload.authorize")}
                </button>
              </div>
            )}
            <TargetCard
              target={tg}
              info={info(tg.platform)}
              name={accounts.data?.find((a) => a.id === tg.accountId)?.displayName ?? ""}
              common={common}
              filePath={filePath}
              onChange={(n) => setTargets((ts) => ts.map((x, j) => (j === i ? n : x)))}
              onRemove={() => setTargets((ts) => ts.filter((_, j) => j !== i))}
            />
          </div>
        ))}
        <div className="flex items-center gap-3">
          <button className={primary} disabled={!canPublish} onClick={() => publish.mutate()}>
            {t("publish.publish", { count: targets.length })}
          </button>
          {publish.isError && (
            <span className="text-sm text-red-500">{errorMessage(publish.error, t)}</span>
          )}
        </div>
      </section>

      <section className="space-y-3 rounded-lg border border-[var(--border)] bg-[var(--panel)] p-4">
        <h2 className="font-medium">{t("publish.exportTitle")}</h2>
        <p className="text-xs text-[var(--muted)]">{t("publish.exportHint")}</p>
        <RenderFields value={exportSpec} onChange={setExportSpec} />
        <div className="flex flex-wrap items-center gap-2">
          <button
            className={btn}
            disabled={!filePath || doExport.isPending}
            onClick={() => doExport.mutate()}
          >
            {doExport.isPending
              ? t("publish.exporting", { pct: Math.floor((progress[exportKey] ?? 0) * 100) })
              : t("publish.export")}
          </button>
          <button className={btn} onClick={() => void copyCaption()}>
            {copied ? t("publish.copied") : t("publish.copyCaption")}
          </button>
          {doExport.isSuccess && doExport.data && (
            <span className="text-xs text-green-600">{t("publish.exported")}</span>
          )}
          {doExport.isError && (
            <span className="text-xs text-red-500">{errorMessage(doExport.error, t)}</span>
          )}
        </div>
      </section>
    </div>
  );
}
