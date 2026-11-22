import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Asset } from "../../lib/bindings";
import { errorMessage } from "../../lib/ipc";

const KINDS = ["skin", "avatar", "sticker", "background", "image"] as const;
const field = "rounded-md border border-[var(--border)] bg-[var(--bg)] px-2 py-1 text-xs";
const btn = "rounded-md border border-[var(--border)] px-2 py-1 text-xs disabled:opacity-50";

function Preview({ id }: { id: string }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    commands
      .assetDataUrl(id)
      .then((u) => alive && setSrc(u))
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [id]);
  return src ? (
    <img src={src} alt="" className="h-24 w-full object-contain" />
  ) : (
    <div className="h-24 w-full bg-[var(--border)]" />
  );
}

function AssetCard({ a }: { a: Asset }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [name, setName] = useState(a.name);
  const [variant, setVariant] = useState(a.variant ?? "");
  const [kind, setKind] = useState(a.kind);
  const refresh = () => void qc.invalidateQueries({ queryKey: ["assets"] });
  const update = useMutation({
    mutationFn: () =>
      commands.updateAsset(a.id, { kind, name, variant: variant || null, tags: a.tags }),
    onSuccess: refresh,
  });
  const del = useMutation({ mutationFn: () => commands.deleteAsset(a.id), onSuccess: refresh });
  const dirty = name !== a.name || variant !== (a.variant ?? "") || kind !== a.kind;
  return (
    <li className="space-y-1 rounded-md border border-[var(--border)] bg-[var(--panel)] p-2">
      <Preview id={a.id} />
      <input className={`${field} w-full`} value={name} onChange={(e) => setName(e.target.value)} />
      <div className="flex gap-1">
        <select className={field} value={kind} onChange={(e) => setKind(e.target.value)}>
          {KINDS.map((k) => (
            <option key={k} value={k}>
              {t(`library.kind.${k}`)}
            </option>
          ))}
        </select>
        <input
          className={`${field} w-full`}
          placeholder={t("library.variant")}
          value={variant}
          onChange={(e) => setVariant(e.target.value)}
        />
      </div>
      <div className="flex gap-1">
        <button className={btn} disabled={!dirty || !name.trim()} onClick={() => update.mutate()}>
          {t("ideas.save")}
        </button>
        <button className={btn} onClick={() => del.mutate()}>
          {t("thumbs.delete")}
        </button>
      </div>
    </li>
  );
}

/** Creator's reusable images: skins (normal, Halloween, Christmas...), avatars, stickers. */
export function AssetLibrary() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [kind, setKind] = useState<string>("skin");
  const [variant, setVariant] = useState("");
  const [filter, setFilter] = useState("");
  const assets = useQuery({ queryKey: ["assets"], queryFn: () => commands.listAssets() });
  const importMany = useMutation({
    mutationFn: async () => {
      const picked = await open({
        multiple: true,
        filters: [
          { name: t("upload.imageFiles"), extensions: ["png", "jpg", "jpeg", "webp", "gif"] },
        ],
      });
      const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
      for (const p of paths) {
        const name = (p.split(/[\\/]/).pop() ?? "asset").replace(/\.[^.]+$/, "");
        await commands.importAsset(p, { kind, name, variant: variant || null, tags: [] });
      }
    },
    onSettled: () => void qc.invalidateQueries({ queryKey: ["assets"] }),
  });
  const shown = (assets.data ?? []).filter((a) => !filter || a.kind === filter);
  return (
    <div className="space-y-3">
      <p className="text-xs text-[var(--muted)]">{t("library.hint")}</p>
      <div className="flex flex-wrap items-center gap-2">
        <select className={field} value={kind} onChange={(e) => setKind(e.target.value)}>
          {KINDS.map((k) => (
            <option key={k} value={k}>
              {t(`library.kind.${k}`)}
            </option>
          ))}
        </select>
        <input
          className={field}
          placeholder={t("library.variantPlaceholder")}
          value={variant}
          onChange={(e) => setVariant(e.target.value)}
        />
        <button className={btn} onClick={() => importMany.mutate()} disabled={importMany.isPending}>
          {t("library.import")}
        </button>
        <select className={field} value={filter} onChange={(e) => setFilter(e.target.value)}>
          <option value="">{t("library.all")}</option>
          {KINDS.map((k) => (
            <option key={k} value={k}>
              {t(`library.kind.${k}`)}
            </option>
          ))}
        </select>
      </div>
      {importMany.isError && (
        <p className="text-xs text-red-500">{errorMessage(importMany.error, t)}</p>
      )}
      {!shown.length ? (
        <p className="text-sm text-[var(--muted)]">{t("library.empty")}</p>
      ) : (
        <ul className="grid grid-cols-2 gap-2 sm:grid-cols-4 lg:grid-cols-6">
          {shown.map((a) => (
            <AssetCard key={a.id} a={a} />
          ))}
        </ul>
      )}
    </div>
  );
}
