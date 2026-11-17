import type { ComponentType } from "react";
import { TwitchView } from "./twitch/TwitchView";

/** Frontend side of a connector. One line here per network (plus the Rust registry). */
export interface PlatformUI {
  id: string;
  /** Network-specific analytics view; the generic view is used when absent. */
  AnalyticsView?: ComponentType<{ accountId: string }>;
  UploadOptionsForm?: ComponentType<Record<string, unknown>>;
  PreviewCard?: ComponentType<Record<string, unknown>>;
  /** Provider page where the user can review/revoke third-party access by hand. */
  permissionsUrl?: string;
}

export const platformUIs: Record<string, PlatformUI> = {
  youtube: { id: "youtube", permissionsUrl: "https://myaccount.google.com/connections" },
  twitch: {
    id: "twitch",
    AnalyticsView: TwitchView,
    permissionsUrl: "https://www.twitch.tv/settings/connections",
  },
  mock: { id: "mock" },
};

export function platformUI(id: string): PlatformUI | undefined {
  return platformUIs[id];
}
