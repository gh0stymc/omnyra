/** Thumbnail editor scene: plain JSON, saved as-is in templates. */
export interface Shadow {
  color: string;
  blur: number;
  x: number;
  y: number;
}

export interface Layer {
  id: string;
  name: string;
  kind: "image" | "text" | "shape";
  x: number;
  y: number;
  w: number;
  h: number;
  /** Degrees. */
  rotation: number;
  opacity: number;
  visible: boolean;
  /** image: library asset. */
  assetId?: string | null;
  flipX?: boolean;
  /** text */
  text?: string;
  font?: string;
  size?: number;
  bold?: boolean;
  align?: "left" | "center" | "right";
  /** text and shape */
  fill?: string;
  stroke?: string;
  strokeWidth?: number;
  shadow?: Shadow | null;
  /** shape */
  shape?: "rect" | "ellipse";
  radius?: number;
}

export interface Scene {
  width: number;
  height: number;
  background: { color: string; assetId: string | null };
  layers: Layer[];
}

export interface SizePreset {
  id: string;
  width: number;
  height: number;
  /** JPEG keeps YouTube thumbnails under 2 MB. */
  format: "image/png" | "image/jpeg";
}

export const SIZE_PRESETS: SizePreset[] = [
  { id: "youtube", width: 1280, height: 720, format: "image/jpeg" },
  { id: "vertical", width: 1080, height: 1920, format: "image/jpeg" },
  { id: "square", width: 1080, height: 1080, format: "image/jpeg" },
  { id: "twitch", width: 1920, height: 1080, format: "image/png" },
];

export const FONTS = [
  "Impact",
  "Arial Black",
  "Montserrat",
  "Bebas Neue",
  "Comic Sans MS",
  "sans-serif",
];
export const STICKERS = [
  "🔥",
  "😱",
  "💀",
  "⭐",
  "❤️",
  "👉",
  "❗",
  "❓",
  "💎",
  "⛏️",
  "🧟",
  "🎃",
  "🎄",
];

let counter = 0;
export const newId = () => `l${Date.now().toString(36)}${(counter++).toString(36)}`;

export function emptyScene(width = 1280, height = 720): Scene {
  return { width, height, background: { color: "#1e1b4b", assetId: null }, layers: [] };
}

const base = (scene: Scene, name: string): Omit<Layer, "kind"> => ({
  id: newId(),
  name,
  x: scene.width * 0.1,
  y: scene.height * 0.1,
  w: scene.width * 0.4,
  h: scene.height * 0.3,
  rotation: 0,
  opacity: 1,
  visible: true,
});

export function textLayer(scene: Scene, text = "TEXTO"): Layer {
  const size = Math.round(scene.height * 0.14);
  return {
    ...base(scene, text),
    kind: "text",
    text,
    font: "Impact",
    size,
    bold: true,
    align: "left",
    fill: "#ffffff",
    stroke: "#000000",
    strokeWidth: Math.max(2, Math.round(size * 0.08)),
    shadow: { color: "rgba(0,0,0,0.6)", blur: 12, x: 4, y: 6 },
    h: size * 1.2,
    w: scene.width * 0.6,
  };
}

export function shapeLayer(scene: Scene, shape: "rect" | "ellipse"): Layer {
  return {
    ...base(scene, shape),
    kind: "shape",
    shape,
    fill: "#facc15",
    stroke: "#000000",
    strokeWidth: 0,
    radius: shape === "rect" ? 24 : 0,
    shadow: null,
  };
}

export function imageLayer(scene: Scene, assetId: string, name: string, aspect = 1): Layer {
  const h = scene.height * 0.8;
  return {
    ...base(scene, name),
    kind: "image",
    assetId,
    flipX: false,
    h,
    w: h * aspect,
    y: scene.height * 0.1,
  };
}

export function stickerLayer(scene: Scene, emoji: string): Layer {
  const size = Math.round(scene.height * 0.2);
  return {
    ...textLayer(scene, emoji),
    name: emoji,
    size,
    strokeWidth: 0,
    shadow: null,
    w: size * 1.3,
    h: size * 1.3,
  };
}

export function addLayer(scene: Scene, layer: Layer): Scene {
  return { ...scene, layers: [...scene.layers, layer] };
}

export function updateLayer(scene: Scene, id: string, patch: Partial<Layer>): Scene {
  return { ...scene, layers: scene.layers.map((l) => (l.id === id ? { ...l, ...patch } : l)) };
}

export function removeLayer(scene: Scene, id: string): Scene {
  return { ...scene, layers: scene.layers.filter((l) => l.id !== id) };
}

export function duplicateLayer(scene: Scene, id: string): Scene {
  const i = scene.layers.findIndex((l) => l.id === id);
  if (i < 0) return scene;
  const copy = {
    ...scene.layers[i],
    id: newId(),
    x: scene.layers[i].x + 20,
    y: scene.layers[i].y + 20,
  };
  const layers = [...scene.layers];
  layers.splice(i + 1, 0, copy);
  return { ...scene, layers };
}

/** `up` = towards the front (drawn later). */
export function moveLayer(scene: Scene, id: string, dir: "up" | "down"): Scene {
  const i = scene.layers.findIndex((l) => l.id === id);
  const j = dir === "up" ? i + 1 : i - 1;
  if (i < 0 || j < 0 || j >= scene.layers.length) return scene;
  const layers = [...scene.layers];
  [layers[i], layers[j]] = [layers[j], layers[i]];
  return { ...scene, layers };
}

/** Top-most visible layer under the point (rotation ignored: boxes are axis-aligned). */
export function hitTest(scene: Scene, x: number, y: number): Layer | null {
  for (let i = scene.layers.length - 1; i >= 0; i--) {
    const l = scene.layers[i];
    if (l.visible && x >= l.x && x <= l.x + l.w && y >= l.y && y <= l.y + l.h) return l;
  }
  return null;
}

/**
 * How to draw a `scene` into a `w × h` export: scale to cover and centre, so a 16:9 design
 * exported as 9:16 keeps its middle.
 */
export function coverTransform(scene: Scene, w: number, h: number) {
  const scale = Math.max(w / scene.width, h / scene.height);
  return { scale, dx: (w - scene.width * scale) / 2, dy: (h - scene.height * scale) / 2 };
}

/** Changes the canvas size, moving layers so they keep their relative position. */
export function resizeScene(scene: Scene, width: number, height: number): Scene {
  const sx = width / scene.width;
  const sy = height / scene.height;
  const s = Math.min(sx, sy);
  return {
    ...scene,
    width,
    height,
    layers: scene.layers.map((l) => {
      const cx = (l.x + l.w / 2) * sx;
      const cy = (l.y + l.h / 2) * sy;
      const w = l.w * s;
      const h = l.h * s;
      return {
        ...l,
        x: cx - w / 2,
        y: cy - h / 2,
        w,
        h,
        size: l.size ? l.size * s : l.size,
        strokeWidth: l.strokeWidth ? l.strokeWidth * s : l.strokeWidth,
      };
    }),
  };
}

/** Asset IDs the scene needs loaded. */
export function assetIds(scene: Scene): string[] {
  const ids = scene.layers
    .filter((l) => l.kind === "image" && l.assetId)
    .map((l) => l.assetId as string);
  if (scene.background.assetId) ids.push(scene.background.assetId);
  return [...new Set(ids)];
}

export function parseScene(json: string): Scene | null {
  try {
    const s = JSON.parse(json) as Scene;
    if (typeof s.width !== "number" || !Array.isArray(s.layers)) return null;
    return s;
  } catch {
    return null;
  }
}
