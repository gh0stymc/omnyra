import { coverTransform, type Layer, type Scene } from "./scene";

export type Images = Map<string, HTMLImageElement>;

function drawCover(ctx: CanvasRenderingContext2D, img: HTMLImageElement, w: number, h: number) {
  const s = Math.max(w / img.width, h / img.height);
  const iw = img.width * s;
  const ih = img.height * s;
  ctx.drawImage(img, (w - iw) / 2, (h - ih) / 2, iw, ih);
}

function roundRect(ctx: CanvasRenderingContext2D, w: number, h: number, r: number) {
  const rr = Math.max(0, Math.min(r, w / 2, h / 2));
  ctx.beginPath();
  ctx.moveTo(rr, 0);
  ctx.arcTo(w, 0, w, h, rr);
  ctx.arcTo(w, h, 0, h, rr);
  ctx.arcTo(0, h, 0, 0, rr);
  ctx.arcTo(0, 0, w, 0, rr);
  ctx.closePath();
}

function applyShadow(ctx: CanvasRenderingContext2D, l: Layer) {
  if (l.shadow) {
    ctx.shadowColor = l.shadow.color;
    ctx.shadowBlur = l.shadow.blur;
    ctx.shadowOffsetX = l.shadow.x;
    ctx.shadowOffsetY = l.shadow.y;
  }
}

function clearShadow(ctx: CanvasRenderingContext2D) {
  ctx.shadowColor = "transparent";
  ctx.shadowBlur = 0;
  ctx.shadowOffsetX = 0;
  ctx.shadowOffsetY = 0;
}

function drawLayer(ctx: CanvasRenderingContext2D, l: Layer, images: Images) {
  ctx.save();
  ctx.globalAlpha = l.opacity;
  ctx.translate(l.x + l.w / 2, l.y + l.h / 2);
  ctx.rotate((l.rotation * Math.PI) / 180);
  ctx.translate(-l.w / 2, -l.h / 2);
  if (l.kind === "image") {
    const img = l.assetId ? images.get(l.assetId) : undefined;
    if (img) {
      if (l.flipX) {
        ctx.translate(l.w, 0);
        ctx.scale(-1, 1);
      }
      applyShadow(ctx, l);
      ctx.drawImage(img, 0, 0, l.w, l.h);
    }
  } else if (l.kind === "shape") {
    applyShadow(ctx, l);
    if (l.shape === "ellipse") {
      ctx.beginPath();
      ctx.ellipse(l.w / 2, l.h / 2, l.w / 2, l.h / 2, 0, 0, Math.PI * 2);
    } else {
      roundRect(ctx, l.w, l.h, l.radius ?? 0);
    }
    ctx.fillStyle = l.fill ?? "#fff";
    ctx.fill();
    if (l.strokeWidth) {
      clearShadow(ctx);
      ctx.lineWidth = l.strokeWidth;
      ctx.strokeStyle = l.stroke ?? "#000";
      ctx.stroke();
    }
  } else {
    const size = l.size ?? 64;
    ctx.font = `${l.bold ? "bold " : ""}${size}px "${l.font ?? "sans-serif"}", sans-serif`;
    ctx.textBaseline = "top";
    ctx.textAlign = l.align ?? "left";
    ctx.lineJoin = "round";
    const x = l.align === "center" ? l.w / 2 : l.align === "right" ? l.w : 0;
    const lines = (l.text ?? "").split("\n");
    lines.forEach((line, i) => {
      const y = i * size * 1.1;
      if (l.strokeWidth) {
        // Outline first (doubled: half of it is covered by the fill), with the shadow.
        applyShadow(ctx, l);
        ctx.lineWidth = l.strokeWidth * 2;
        ctx.strokeStyle = l.stroke ?? "#000";
        ctx.strokeText(line, x, y);
        clearShadow(ctx);
      } else {
        applyShadow(ctx, l);
      }
      ctx.fillStyle = l.fill ?? "#fff";
      ctx.fillText(line, x, y);
    });
  }
  ctx.restore();
}

/** Draws `scene` scaled to cover a `w × h` canvas. */
export function drawScene(
  ctx: CanvasRenderingContext2D,
  scene: Scene,
  images: Images,
  w = scene.width,
  h = scene.height,
  selectedId?: string | null,
) {
  const { scale, dx, dy } = coverTransform(scene, w, h);
  ctx.save();
  ctx.clearRect(0, 0, w, h);
  ctx.fillStyle = scene.background.color;
  ctx.fillRect(0, 0, w, h);
  ctx.translate(dx, dy);
  ctx.scale(scale, scale);
  const bg = scene.background.assetId ? images.get(scene.background.assetId) : undefined;
  if (bg) drawCover(ctx, bg, scene.width, scene.height);
  for (const l of scene.layers) if (l.visible) drawLayer(ctx, l, images);
  const sel = scene.layers.find((l) => l.id === selectedId);
  if (sel) {
    ctx.setLineDash([12, 8]);
    ctx.lineWidth = 3 / scale;
    ctx.strokeStyle = "#38bdf8";
    ctx.strokeRect(sel.x, sel.y, sel.w, sel.h);
  }
  ctx.restore();
}

export function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = reject;
    img.src = src;
  });
}

/** Renders the scene at `w × h` into a data URL. */
export function exportScene(
  scene: Scene,
  images: Images,
  w: number,
  h: number,
  format: "image/png" | "image/jpeg",
): string {
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("no 2d context");
  drawScene(ctx, scene, images, w, h, null);
  return canvas.toDataURL(format, 0.9);
}
