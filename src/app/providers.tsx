import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useEffect, type ReactNode } from "react";
import "../i18n";
import { applyTheme } from "../stores/ui";
import { useUi } from "../stores/ui";

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
});

export function Providers({ children }: { children: ReactNode }) {
  const theme = useUi((s) => s.theme);
  useEffect(() => applyTheme(theme), [theme]);
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}
