import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useEffect, type ReactNode } from "react";
import "../i18n";
import { events } from "../lib/bindings";
import { queryKeys } from "../lib/queryKeys";
import { applyTheme } from "../stores/ui";
import { useUi } from "../stores/ui";

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
});

/** Backend events → query invalidation. */
function useBackendEvents() {
  useEffect(() => {
    const unlisten = events.accountChanged
      .listen(() => void queryClient.invalidateQueries({ queryKey: queryKeys.accounts }))
      .catch(() => undefined); // not running inside Tauri (tests, plain browser)
    return () => void unlisten.then((f) => f?.());
  }, []);
}

export function Providers({ children }: { children: ReactNode }) {
  const theme = useUi((s) => s.theme);
  useEffect(() => applyTheme(theme), [theme]);
  useBackendEvents();
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}
