export const queryKeys = {
  platforms: ["platforms"] as const,
  accounts: ["accounts"] as const,
  series: (accountId: string, metric: string) => ["series", accountId, metric] as const,
  contents: (accountId: string) => ["contents", accountId] as const,
  quota: ["quota"] as const,
  uploads: ["uploads"] as const,
  summary: (from: string | null) => ["summary", from] as const,
  combined: (metric: string, from: string | null) => ["combined", metric, from] as const,
  top: (from: string | null) => ["top", from] as const,
  ideas: ["ideas"] as const,
  calendar: (from: string, to: string) => ["calendar", from, to] as const,
  live: (accountId: string, from: string | null) => ["live", accountId, from] as const,
  bestTime: (accountId: string, format: string, includeUnlisted: boolean, tz: string) =>
    ["bestTime", accountId, format, includeUnlisted, tz] as const,
};
