import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { save } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState, type PointerEvent } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { commands, type Asset } from "../../../lib/bindings";
import { errorMessage } from "../../../lib/ipc";
import { drawScene, exportScene, loadImage, type Images } from "./render";
import {
  FONTS,
  SIZE_PRESETS,
  STICKERS,
  addLayer,
  assetIds,
  duplicateLayer,
  emptyScene,
  hitTest,
  imageLayer,
  moveLayer,
  parseScene,
  removeLayer,
  resizeScene,
  shapeLayer,
  stickerLayer,
  textLayer,
  updateLayer,
  type Layer,
  type Scene,
} from "./scene";

const field = "w-full rounded-md border border-[var(--border)] bg-[var(--bg)] px-2 py-1 text-xs";
const btn = "rounded-md border border-[var(--border)] px-2 py-1 text-xs disabled:opacity-50";
const DISPLAY_WIDTH = 760;

/** Loads (and caches) the data URLs of the assets a scene uses. */
function useImages(ids: string[]) {
  const [images, setImages] = useState<Images>(new Map());
  const key = ids.join(",");
  useEffect(() => {
    let alive = true;
    const missing = ids.filter((id) => !images.has(id));
    if (!missing.length) return;
    void Promise.all(
      missing.map(async (id) => {
        try {
          const img = await loadImage(await commands.assetDataUrl(id));
          return [id, img] as const;
        } catch {
          return null;
        }
      }),
    ).then((loaded) => {
      if (!alive) return;
      setImages((prev) => {
        const next = new Map(prev);
        for (const e of loaded) if (e) next.set(e[0], e[1]);
        return next;
      });
    });
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return images;
}

function Num({
  label,
  value,
  onChange,
  step = 1,
}: {
  label: string;
  value: number;
  onChange: (n: number) => void;
  step?: number;
}) {
  return (
    <label className="space-y-0.5 text-xs">
      <span className="text-[var(--muted)]">{label}</span>
      <input
        type="number"
        step={step}
        className={field}
        value={Math.round(value * 100) / 100}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </label>
  );
}

function Props({ layer, onChange }: { layer: Layer; onChange: (p: Partial<Layer>) => void }) {
  const { t } = useTranslation();
  return (
    <div className="space-y-2">
      <div className="grid grid-cols-2 gap-2">
        <Num label="X" value={layer.x} onChange={(x) => onChange({ x })} />
        <Num label="Y" value={layer.y} onChange={(y) => onChange({ y })} />
        <Num label={t("thumbs.width")} value={layer.w} onChange={(w) => onChange({ w })} />
        <Num label={t("thumbs.height")} value={layer.h} onChange={(h) => onChange({ h })} />
        <Num
          label={t("thumbs.rotation")}
          value={layer.rotation}
          onChange={(rotation) => onChange({ rotation })}
        />
        <Num
          label={t("thumbs.opacity")}
          value={layer.opacity}
          step={0.05}
          onChange={(opacity) => onChange({ opacity: Math.min(1, Math.max(0, opacity)) })}
        />
      </div>
      {layer.kind === "text" && (
        <>
          <textarea
            className={`${field} min-h-14`}
            value={layer.text ?? ""}
            onChange={(e) => onChange({ text: e.target.value, name: e.target.value.slice(0, 20) })}
          />
          <div className="grid grid-cols-2 gap-2">
            <label className="space-y-0.5 text-xs">
              <span className="text-[var(--muted)]">{t("thumbs.font")}</span>
              <select
                className={field}
                value={layer.font}
                onChange={(e) => onChange({ font: e.target.value })}
              >
                {FONTS.map((f) => (
                  <option key={f}>{f}</option>
                ))}
              </select>
            </label>
            <Num
              label={t("thumbs.size")}
              value={layer.size ?? 64}
              onChange={(size) => onChange({ size })}
            />
            <label className="flex items-center gap-1 text-xs">
              <input
                type="checkbox"
                checked={!!layer.bold}
                onChange={(e) => onChange({ bold: e.target.checked })}
              />
              {t("thumbs.bold")}
            </label>
            <select
              className={field}
              value={layer.align ?? "left"}
              onChange={(e) => onChange({ align: e.target.value as Layer["align"] })}
            >
              {(["left", "center", "right"] as const).map((a) => (
                <option key={a} value={a}>
                  {t(`thumbs.align.${a}`)}
                </option>
              ))}
            </select>
          </div>
        </>
      )}
      {layer.kind !== "image" && (
        <div className="grid grid-cols-3 items-end gap-2">
          <label className="space-y-0.5 text-xs">
            <span className="text-[var(--muted)]">{t("thumbs.fill")}</span>
            <input
              type="color"
              className="h-7 w-full"
              value={layer.fill ?? "#ffffff"}
              onChange={(e) => onChange({ fill: e.target.value })}
            />
          </label>
          <label className="space-y-0.5 text-xs">
            <span className="text-[var(--muted)]">{t("thumbs.stroke")}</span>
            <input
              type="color"
              className="h-7 w-full"
              value={layer.stroke ?? "#000000"}
              onChange={(e) => onChange({ stroke: e.target.value })}
            />
          </label>
          <Num
            label={t("thumbs.strokeWidth")}
            value={layer.strokeWidth ?? 0}
            onChange={(strokeWidth) => onChange({ strokeWidth: Math.max(0, strokeWidth) })}
          />
        </div>
      )}
      {layer.kind === "shape" && layer.shape === "rect" && (
        <Num
          label={t("thumbs.radius")}
          value={layer.radius ?? 0}
          onChange={(radius) => onChange({ radius })}
        />
      )}
      {layer.kind === "image" && (
        <label className="flex items-center gap-1 text-xs">
          <input
            type="checkbox"
            checked={!!layer.flipX}
            onChange={(e) => onChange({ flipX: e.target.checked })}
          />
          {t("thumbs.flip")}
        </label>
      )}
      <label className="flex items-center gap-1 text-xs">
        <input
          type="checkbox"
          checked={!!layer.shadow}
          onChange={(e) =>
            onChange({
              shadow: e.target.checked ? { color: "rgba(0,0,0,0.6)", blur: 12, x: 4, y: 6 } : null,
            })
          }
        />
        {t("thumbs.shadow")}
      </label>
      {layer.shadow && (
        <div className="grid grid-cols-3 gap-2">
          <Num
            label={t("thumbs.blur")}
            value={layer.shadow.blur}
            onChange={(blur) => onChange({ shadow: { ...layer.shadow!, blur } })}
          />
          <Num
            label="dx"
            value={layer.shadow.x}
            onChange={(x) => onChange({ shadow: { ...layer.shadow!, x } })}
          />
          <Num
            label="dy"
            value={layer.shadow.y}
            onChange={(y) => onChange({ shadow: { ...layer.shadow!, y } })}
          />
        </div>
      )}
    </div>
  );
}

/** Layered canvas editor: background, library images, outlined text, shapes, stickers. */
export function ThumbnailEditor() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [scene, setScene] = useState<Scene>(emptyScene);
  const [selected, setSelected] = useState<string | null>(null);
  const [templateId, setTemplateId] = useState<string | null>(null);
  const [templateName, setTemplateName] = useState("");
  const [exportPreset, setExportPreset] = useState(SIZE_PRESETS[0].id);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const drag = useRef<{ id: string; dx: number; dy: number } | null>(null);
  const assets = useQuery({ queryKey: ["assets"], queryFn: () => commands.listAssets() });
  const templates = useQuery({
    queryKey: ["thumbTemplates"],
    queryFn: () => commands.listThumbnailTemplates(),
  });
  const images = useImages(assetIds(scene));
  const scale = DISPLAY_WIDTH / scene.width;
  const sel = scene.layers.find((l) => l.id === selected) ?? null;

  useEffect(() => {
    const ctx = canvasRef.current?.getContext("2d");
    if (ctx) drawScene(ctx, scene, images, scene.width, scene.height, selected);
  }, [scene, images, selected]);

  const toScene = (e: PointerEvent<HTMLCanvasElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    return {
      x: ((e.clientX - r.left) / r.width) * scene.width,
      y: ((e.clientY - r.top) / r.height) * scene.height,
    };
  };
  const onDown = (e: PointerEvent<HTMLCanvasElement>) => {
    const p = toScene(e);
    const hit = hitTest(scene, p.x, p.y);
    setSelected(hit?.id ?? null);
    if (hit) {
      drag.current = { id: hit.id, dx: p.x - hit.x, dy: p.y - hit.y };
      e.currentTarget.setPointerCapture(e.pointerId);
    }
  };
  const onMove = (e: PointerEvent<HTMLCanvasElement>) => {
    if (!drag.current) return;
    const p = toScene(e);
    const d = drag.current;
    setScene((s) => updateLayer(s, d.id, { x: p.x - d.dx, y: p.y - d.dy }));
  };
  const onUp = () => {
    drag.current = null;
  };

  const add = (l: Layer) => {
    setScene((s) => addLayer(s, l));
    setSelected(l.id);
  };
  const addImage = async (a: Asset) => {
    let aspect = 1;
    try {
      const img = await loadImage(await commands.assetDataUrl(a.id));
      aspect = img.width / img.height;
    } catch {
      /* keep square */
    }
    add(imageLayer(scene, a.id, a.variant ? `${a.name} (${a.variant})` : a.name, aspect));
  };
  const patch = (p: Partial<Layer>) => sel && setScene((s) => updateLayer(s, sel.id, p));

  const saveTpl = useMutation({
    mutationFn: (asNew: boolean) =>
      commands.saveThumbnailTemplate({
        id: asNew ? null : templateId,
        name: templateName || t("thumbs.untitled"),
        width: scene.width,
        height: scene.height,
        scene: JSON.stringify(scene),
      }),
    onSuccess: (tpl) => {
      setTemplateId(tpl.id);
      setTemplateName(tpl.name);
      void qc.invalidateQueries({ queryKey: ["thumbTemplates"] });
    },
  });
  const delTpl = useMutation({
    mutationFn: (id: string) => commands.deleteThumbnailTemplate(id),
    onSuccess: () => {
      setTemplateId(null);
      void qc.invalidateQueries({ queryKey: ["thumbTemplates"] });
    },
  });
  const loadTpl = (id: string) => {
    const tpl = templates.data?.find((x) => x.id === id);
    const s = tpl && parseScene(tpl.scene);
    if (tpl && s) {
      setScene(s);
      setTemplateId(tpl.id);
      setTemplateName(tpl.name);
      setSelected(null);
    }
  };

  const preset = SIZE_PRESETS.find((p) => p.id === exportPreset) ?? SIZE_PRESETS[0];
  const render = () => exportScene(scene, images, preset.width, preset.height, preset.format);
  const saveFile = useMutation({
    mutationFn: async () => {
      const ext = preset.format === "image/png" ? "png" : "jpg";
      const dest = await save({
        defaultPath: `${templateName || "miniatura"}-${preset.id}.${ext}`,
        filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
      });
      if (dest) await commands.writeThumbnailFile(render(), dest);
      return !!dest;
    },
  });
  const useForUpload = useMutation({
    mutationFn: () => commands.saveThumbnailExport(render(), templateName || "miniatura"),
    onSuccess: (path) => navigate(`/uploads?thumbnail=${encodeURIComponent(path)}`),
  });

  return (
    <div className="flex flex-wrap gap-4">
      <div className="space-y-2">
        <div className="flex flex-wrap items-center gap-1">
          <button className={btn} onClick={() => add(textLayer(scene))}>
            + {t("thumbs.text")}
          </button>
          <button className={btn} onClick={() => add(shapeLayer(scene, "rect"))}>
            + {t("thumbs.rect")}
          </button>
          <button className={btn} onClick={() => add(shapeLayer(scene, "ellipse"))}>
            + {t("thumbs.ellipse")}
          </button>
          <select
            className={btn}
            value=""
            onChange={(e) => {
              const a = assets.data?.find((x) => x.id === e.target.value);
              if (a) void addImage(a);
            }}
          >
            <option value="">+ {t("thumbs.image")}</option>
            {assets.data?.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
                {a.variant ? ` (${a.variant})` : ""}
              </option>
            ))}
          </select>
          <select
            className={btn}
            value=""
            onChange={(e) => e.target.value && add(stickerLayer(scene, e.target.value))}
          >
            <option value="">+ {t("thumbs.sticker")}</option>
            {STICKERS.map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
          <select
            className={btn}
            aria-label={t("thumbs.canvasSize")}
            value={`${scene.width}x${scene.height}`}
            onChange={(e) => {
              const [w, h] = e.target.value.split("x").map(Number);
              setScene((s) => resizeScene(s, w, h));
            }}
          >
            {SIZE_PRESETS.map((p) => (
              <option key={p.id} value={`${p.width}x${p.height}`}>
                {t(`thumbs.preset.${p.id}`)} · {p.width}×{p.height}
              </option>
            ))}
          </select>
        </div>
        <canvas
          ref={canvasRef}
          width={scene.width}
          height={scene.height}
          style={{ width: DISPLAY_WIDTH, height: scene.height * scale }}
          className="cursor-move rounded-md border border-[var(--border)]"
          onPointerDown={onDown}
          onPointerMove={onMove}
          onPointerUp={onUp}
        />
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <span>{t("thumbs.background")}</span>
          <input
            type="color"
            value={scene.background.color}
            onChange={(e) =>
              setScene((s) => ({ ...s, background: { ...s.background, color: e.target.value } }))
            }
          />
          <select
            className={btn}
            value={scene.background.assetId ?? ""}
            onChange={(e) =>
              setScene((s) => ({
                ...s,
                background: { ...s.background, assetId: e.target.value || null },
              }))
            }
          >
            <option value="">{t("thumbs.noBackgroundImage")}</option>
            {assets.data?.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
                {a.variant ? ` (${a.variant})` : ""}
              </option>
            ))}
          </select>
        </div>
      </div>

      <aside className="w-72 space-y-3">
        <section className="space-y-1 rounded-md border border-[var(--border)] p-2">
          <h3 className="text-xs font-medium">{t("thumbs.layers")}</h3>
          {[...scene.layers].reverse().map((l) => (
            <div
              key={l.id}
              className={`flex items-center gap-1 rounded px-1 text-xs ${l.id === selected ? "bg-[var(--border)]" : ""}`}
            >
              <button className="flex-1 truncate text-left" onClick={() => setSelected(l.id)}>
                {l.name || l.kind}
              </button>
              <button
                title={t("thumbs.toggle")}
                onClick={() => setScene((s) => updateLayer(s, l.id, { visible: !l.visible }))}
              >
                {l.visible ? "👁" : "◌"}
              </button>
              <button
                title={t("thumbs.up")}
                onClick={() => setScene((s) => moveLayer(s, l.id, "up"))}
              >
                ↑
              </button>
              <button
                title={t("thumbs.down")}
                onClick={() => setScene((s) => moveLayer(s, l.id, "down"))}
              >
                ↓
              </button>
              <button
                title={t("thumbs.duplicate")}
                onClick={() => setScene((s) => duplicateLayer(s, l.id))}
              >
                ⧉
              </button>
              <button
                title={t("thumbs.delete")}
                onClick={() => {
                  setScene((s) => removeLayer(s, l.id));
                  setSelected(null);
                }}
              >
                ✕
              </button>
            </div>
          ))}
          {!scene.layers.length && (
            <p className="text-xs text-[var(--muted)]">{t("thumbs.noLayers")}</p>
          )}
        </section>
        {sel && (
          <section className="rounded-md border border-[var(--border)] p-2">
            <Props layer={sel} onChange={patch} />
          </section>
        )}
        <section className="space-y-1 rounded-md border border-[var(--border)] p-2">
          <h3 className="text-xs font-medium">{t("thumbs.templates")}</h3>
          <input
            className={field}
            placeholder={t("thumbs.templateName")}
            value={templateName}
            onChange={(e) => setTemplateName(e.target.value)}
          />
          <div className="flex flex-wrap gap-1">
            <button className={btn} disabled={!templateId} onClick={() => saveTpl.mutate(false)}>
              {t("thumbs.save")}
            </button>
            <button className={btn} onClick={() => saveTpl.mutate(true)}>
              {t("thumbs.saveAs")}
            </button>
            {templateId && (
              <button className={btn} onClick={() => delTpl.mutate(templateId)}>
                {t("thumbs.deleteTemplate")}
              </button>
            )}
          </div>
          <select
            className={field}
            value={templateId ?? ""}
            onChange={(e) => loadTpl(e.target.value)}
          >
            <option value="">{t("thumbs.loadTemplate")}</option>
            {templates.data?.map((tp) => (
              <option key={tp.id} value={tp.id}>
                {tp.name} · {tp.width}×{tp.height}
              </option>
            ))}
          </select>
          {saveTpl.isError && (
            <p className="text-xs text-red-500">{errorMessage(saveTpl.error, t)}</p>
          )}
        </section>
        <section className="space-y-1 rounded-md border border-[var(--border)] p-2">
          <h3 className="text-xs font-medium">{t("thumbs.export")}</h3>
          <select
            className={field}
            value={exportPreset}
            onChange={(e) => setExportPreset(e.target.value)}
          >
            {SIZE_PRESETS.map((p) => (
              <option key={p.id} value={p.id}>
                {t(`thumbs.preset.${p.id}`)} · {p.width}×{p.height}
              </option>
            ))}
          </select>
          <div className="flex flex-wrap gap-1">
            <button className={btn} onClick={() => saveFile.mutate()}>
              {t("thumbs.saveFile")}
            </button>
            <button className={btn} onClick={() => useForUpload.mutate()}>
              {t("thumbs.useForUpload")}
            </button>
          </div>
          {(saveFile.isError || useForUpload.isError) && (
            <p className="text-xs text-red-500">
              {errorMessage(saveFile.error ?? useForUpload.error, t)}
            </p>
          )}
          {saveFile.data && <p className="text-xs text-green-600">{t("thumbs.saved")}</p>}
        </section>
      </aside>
    </div>
  );
}
