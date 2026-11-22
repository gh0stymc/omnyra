import type { RenditionSpec, UploadDraft } from "../../lib/bindings";

export interface CommonText {
  title: string;
  description: string;
  tags: string;
}

/** Per-destination settings of the multi-network wizard. */
export interface TargetState {
  accountId: string;
  platform: string;
  /** Own text instead of the common one. */
  custom: boolean;
  text: CommonText;
  visibility: string;
  madeForKids: boolean | null;
  render: RenditionSpec;
  /** `datetime-local` value, or "" for right away. */
  scheduleLocal: string;
}

export const parseTags = (s: string) =>
  s
    .split(",")
    .map((x) => x.trim())
    .filter(Boolean);

export const DEFAULT_VISIBILITY: Record<string, string> = {
  youtube: "private",
  tiktok: "private",
  instagram: "public",
};

/** `datetime-local` (local time, no zone) → RFC 3339 UTC; "" → null. */
export function localToIso(local: string): string | null {
  if (!local) return null;
  const d = new Date(local);
  return Number.isNaN(d.getTime()) ? null : d.toISOString();
}

export function draftFor(t: TargetState, common: CommonText, filePath: string): UploadDraft {
  const text = t.custom ? t.text : common;
  return {
    filePath,
    title: text.title,
    description: text.description,
    tags: parseTags(text.tags),
    visibility: t.visibility,
    categoryId: null,
    playlistId: null,
    thumbnailPath: null,
    madeForKids: t.madeForKids ?? false,
  };
}

/** The caption a network without titles gets (mirrors `rules::caption` in Rust). */
export function captionOf(text: CommonText): string {
  const parts = [text.title.trim(), text.description.trim()].filter(Boolean);
  let out = parts.join("\n\n");
  const tags = parseTags(text.tags)
    .map((t) => `#${t.replace(/^#/, "").replace(/\s+/g, "")}`)
    .filter((t) => t.length > 1 && !out.toLowerCase().includes(t.toLowerCase()));
  if (tags.length) out = out ? `${out}\n\n${tags.join(" ")}` : tags.join(" ");
  return out;
}

/** YouTube needs an explicit "made for kids" answer. */
export function isComplete(t: TargetState): boolean {
  return t.platform !== "youtube" || t.madeForKids !== null;
}
