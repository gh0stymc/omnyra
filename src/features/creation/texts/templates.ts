/** Template-based text generator (5b): titles, descriptions and hashtags per network. */

export type Field = "title" | "description" | "hashtags";

export interface Vars {
  /** What the video is about. */
  tema: string;
  juego: string;
  serie: string;
  episodio: string;
  canal: string;
  /** Free keywords, comma-separated: used for hashtags. */
  palabras: string;
  fecha: string;
}

export interface Template {
  id: string;
  name: string;
  field: Field;
  /** null = any network. */
  platform: string | null;
  body: string;
  builtIn?: boolean;
}

export const LIMITS: Record<
  string,
  { title: number | null; description: number; hashtags: [number, number] }
> = {
  youtube: { title: 100, description: 5000, hashtags: [3, 5] },
  tiktok: { title: null, description: 2200, hashtags: [3, 5] },
  instagram: { title: null, description: 2200, hashtags: [5, 15] },
  twitch: { title: 140, description: 300, hashtags: [0, 3] },
};

const B = (
  id: string,
  field: Field,
  platform: string | null,
  name: string,
  body: string,
): Template => ({
  id: `builtin-${id}`,
  field,
  platform,
  name,
  body,
  builtIn: true,
});

export const BUILT_IN: Template[] = [
  B("t1", "title", null, "Tema + juego", "{{tema}} | {{juego}}"),
  B("t2", "title", null, "Serie con episodio", "{{serie}} #{{episodio}}: {{tema}}"),
  B("t3", "title", "youtube", "Pregunta", "¿{{tema}}? 🤔 ({{juego}})"),
  B("t4", "title", "youtube", "Mayúsculas", "{{TEMA}} en {{juego}}"),
  B("t5", "title", "twitch", "Directo", "🔴 {{tema}} | {{juego}} | !discord"),
  B("t6", "title", "tiktok", "Gancho", "No sabías esto de {{juego}}: {{tema}}"),
  B(
    "d1",
    "description",
    "youtube",
    "Descripción completa",
    "{{tema}} en {{juego}}.\n\n{{serie}} · episodio {{episodio}}\n\n👍 Si te ha gustado, dale a me gusta y suscríbete a {{canal}}.\n\n{{hashtags}}",
  ),
  B("d2", "description", "tiktok", "Pie corto", "{{tema}} 👀 {{hashtags}}"),
  B(
    "d3",
    "description",
    "instagram",
    "Pie Reel",
    "{{tema}} 🎮\n\nSíguenos para más {{juego}}.\n.\n.\n{{hashtags}}",
  ),
  B("d4", "description", "twitch", "Panel", "Hoy: {{tema}} en {{juego}}. ¡Pásate!"),
];

/** `{{var}}` → value; `{{VAR}}` → value in upper case. Unknown or empty vars are removed,
 *  along with leftover separators. */
export function render(body: string, vars: Partial<Vars> & { hashtags?: string }): string {
  const out = body.replace(/\{\{\s*([A-Za-z]+)\s*\}\}/g, (_, raw: string) => {
    const key = raw.toLowerCase() as keyof Vars | "hashtags";
    const v = ((vars as Record<string, string | undefined>)[key] ?? "").trim();
    return raw === raw.toUpperCase() && raw !== raw.toLowerCase() ? v.toUpperCase() : v;
  });
  return out
    .split("\n")
    .map((line) =>
      line
        .replace(/\s+/g, " ")
        .replace(/#\s*(?=[:|·]|$)/g, "")
        .replace(/(\s*[|·:]\s*)+$/g, "")
        .replace(/^(\s*[|·:]\s*)+/g, "")
        .replace(/\(\s*\)/g, "")
        .replace(/\s+([:?!.,])/g, "$1")
        .trim(),
    )
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

const STRIP = /[^\p{L}\p{N}]/gu;

/** `#Tag` from words: accents removed, CamelCase for phrases. */
export function toHashtag(phrase: string): string {
  const words = phrase
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .split(/\s+/)
    .map((w) => w.replace(STRIP, ""))
    .filter(Boolean);
  if (!words.length) return "";
  const tag =
    words.length === 1
      ? words[0].toLowerCase()
      : words.map((w) => w[0].toUpperCase() + w.slice(1).toLowerCase()).join("");
  return `#${tag}`;
}

const PLATFORM_TAGS: Record<string, string[]> = {
  youtube: ["shorts"],
  tiktok: ["fyp", "parati"],
  instagram: ["reels"],
  twitch: [],
};

/** Hashtags from the vars, within the network's recommended range. */
export function hashtags(vars: Partial<Vars>, platform: string): string[] {
  const [, max] = LIMITS[platform]?.hashtags ?? [3, 5];
  const sources = [
    vars.juego,
    vars.serie,
    ...(vars.palabras ?? "").split(","),
    vars.tema && vars.tema.split(/\s+/).length <= 3 ? vars.tema : "",
  ];
  const tags: string[] = [];
  for (const s of sources) {
    const t = s ? toHashtag(s) : "";
    if (t.length > 2 && !tags.some((x) => x.toLowerCase() === t.toLowerCase())) tags.push(t);
  }
  for (const p of PLATFORM_TAGS[platform] ?? []) if (tags.length < max) tags.push(`#${p}`);
  return tags.slice(0, max);
}

export function templatesFor(all: Template[], field: Field, platform: string): Template[] {
  return all.filter((t) => t.field === field && (t.platform === null || t.platform === platform));
}

/** Every suggestion for a field, deduplicated, empty ones dropped, with length checks. */
export function generate(all: Template[], field: Field, platform: string, vars: Partial<Vars>) {
  const tags = hashtags(vars, platform).join(" ");
  const limit = field === "title" ? LIMITS[platform]?.title : LIMITS[platform]?.description;
  const seen = new Set<string>();
  return templatesFor(all, field, platform)
    .map((t) => ({ template: t, text: render(t.body, { ...vars, hashtags: tags }) }))
    .filter((r) => r.text && !seen.has(r.text) && seen.add(r.text))
    .map((r) => ({ ...r, tooLong: limit != null && [...r.text].length > limit }));
}
