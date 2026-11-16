import type { TFunction } from "i18next";

/** i18n keys for the OAuth scopes we request. Unknown scopes are shown verbatim. */
const SCOPE_KEYS: Record<string, string> = {
  "https://www.googleapis.com/auth/youtube.readonly": "scopes.ytReadonly",
  "https://www.googleapis.com/auth/yt-analytics.readonly": "scopes.ytAnalytics",
  "https://www.googleapis.com/auth/youtube.upload": "scopes.ytUpload",
  "https://www.googleapis.com/auth/youtube.force-ssl": "scopes.ytForceSsl",
};

/** Human-readable labels; identity scopes Google may add on its own are hidden. */
export function scopeLabels(scopes: string[], t: TFunction): string[] {
  return scopes
    .filter((s) => !["openid", "email", "profile"].includes(s))
    .map((s) => (SCOPE_KEYS[s] ? t(SCOPE_KEYS[s]) : s));
}
