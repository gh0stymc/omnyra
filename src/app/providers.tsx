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
    const subs = [
      events.accountChanged.listen(
        () => void queryClient.invalidateQueries({ queryKey: queryKeys.accounts }),
      ),
      events.syncStatus.listen(({ payload }) => {
        void queryClient.invalidateQueries({ queryKey: queryKeys.accounts });
        if (payload.state === "done") {
          void queryClient.invalidateQueries({ queryKey: ["series", payload.accountId] });
          void queryClient.invalidateQueries({ queryKey: queryKeys.contents(payload.accountId) });
          void queryClient.invalidateQueries({ queryKey: queryKeys.quota });
          void queryClient.invalidateQueries({ queryKey: ["bestTime", payload.accountId] });
          for (const k of ["summary", "combined", "top"]) {
            void queryClient.invalidateQueries({ queryKey: [k] });
          }
        }
      }),
      events.uploadChanged.listen(({ payload }) => {
        void queryClient.invalidateQueries({ queryKey: queryKeys.uploads });
        if (payload.status === "published") {
          void queryClient.invalidateQueries({ queryKey: ["contents"] });
          void queryClient.invalidateQueries({ queryKey: ["summary"] });
          void queryClient.invalidateQueries({ queryKey: ["top"] });
        }
      }),
    ].map((p) => p.catch(() => undefined)); // not running inside Tauri (tests, plain browser)
    return () => subs.forEach((p) => void p.then((f) => f?.()));
  }, []);
}

export function Providers({ children }: { children: ReactNode }) {
  const theme = useUi((s) => s.theme);
  useEffect(() => applyTheme(theme), [theme]);
  useBackendEvents();
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}
