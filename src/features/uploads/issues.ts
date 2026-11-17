import type { TFunction } from "i18next";
import type { Issue } from "../../lib/bindings";

/** Formats a limit for an issue message: bytes as MB/GB, seconds as minutes/hours. */
function formatLimit(issue: Issue, lang: string): string {
  const v = issue.limit ?? 0;
  const n = (x: number) => x.toLocaleString(lang, { maximumFractionDigits: 1 });
  if (issue.code === "too_big") {
    return v >= 1024 ** 3 ? `${n(v / 1024 ** 3)} GB` : `${n(v / 1024 ** 2)} MB`;
  }
  if (issue.code === "too_long" && issue.field === "file") return `${n(v / 3600)} h`;
  if (issue.code === "needs_verification") return `${n(v / 60)} min`;
  return n(v);
}

export function issueMessage(issue: Issue, t: TFunction, lang: string): string {
  return t(`upload.issue.${issue.field}.${issue.code}`, {
    limit: formatLimit(issue, lang),
    defaultValue: `${issue.field}: ${issue.code}`,
  });
}
