import type { ComponentType } from "react";

/** Frontend side of a connector. One line here per network (plus the Rust registry). */
export interface PlatformUI {
  id: string;
  /** Network-specific analytics view; the generic view is used when absent. */
  AnalyticsView?: ComponentType<{ accountId: string }>;
  UploadOptionsForm?: ComponentType<Record<string, unknown>>;
  PreviewCard?: ComponentType<Record<string, unknown>>;
}

export const platformUIs: Record<string, PlatformUI> = {
  mock: { id: "mock" },
};

export function platformUI(id: string): PlatformUI | undefined {
  return platformUIs[id];
}
