export const queryKeys = {
  platforms: ["platforms"] as const,
  accounts: ["accounts"] as const,
  series: (accountId: string, metric: string) => ["series", accountId, metric] as const,
  contents: (accountId: string) => ["contents", accountId] as const,
  quota: ["quota"] as const,
};
