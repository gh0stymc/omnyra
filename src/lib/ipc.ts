import type { TFunction } from "i18next";
import type { AppError } from "./bindings";

export function isAppError(e: unknown): e is AppError {
  return typeof e === "object" && e !== null && "code" in e && typeof e.code === "string";
}

/** Maps a backend AppError to a translated, user-facing message. Never exposes technical detail. */
export function errorMessage(e: unknown, t: TFunction): string {
  if (!isAppError(e)) return t("errors.unknown");
  switch (e.code) {
    case "RATE_LIMITED":
      return t("errors.RATE_LIMITED", { seconds: e.detail.retry_after_s });
    case "VALIDATION":
      return t("errors.VALIDATION", { field: e.detail.field });
    default:
      return t(`errors.${e.code}`, { defaultValue: t("errors.unknown") });
  }
}
