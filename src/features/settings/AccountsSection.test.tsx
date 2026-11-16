import { describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import "../../i18n";
import type { Account, PlatformInfo } from "../../lib/bindings";

const mocks = vi.hoisted(() => ({
  listPlatforms: vi.fn(),
  listAccounts: vi.fn(),
  connectAccount: vi.fn(),
  cancelConnect: vi.fn(),
  disconnectAccount: vi.fn(),
  openUrl: vi.fn(),
}));

vi.mock("../../lib/bindings", () => ({
  commands: {
    listPlatforms: mocks.listPlatforms,
    listAccounts: mocks.listAccounts,
    connectAccount: mocks.connectAccount,
    cancelConnect: mocks.cancelConnect,
    disconnectAccount: mocks.disconnectAccount,
  },
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: mocks.openUrl }));

import { AccountsSection } from "./AccountsSection";

function platform(id: string, configured = true): PlatformInfo {
  return {
    id,
    displayName: id === "youtube" ? "YouTube" : "Mock",
    brandColor: "#f00",
    auth: id === "youtube" ? "oauth" : "none",
    authConfigured: configured,
    capabilities: {} as PlatformInfo["capabilities"],
  };
}

const ytAccount: Account = {
  id: "acc-1",
  platform: "youtube",
  externalId: "UC1",
  handle: "@gh0stymc",
  displayName: "gh0stymc",
  grantedScopes: [
    "https://www.googleapis.com/auth/youtube.readonly",
    "https://www.googleapis.com/auth/yt-analytics.readonly",
  ],
  status: "active",
  connectedAt: "2026-11-16T12:00:00Z",
  lastSyncAt: null,
};

function renderSection() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <AccountsSection />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listPlatforms.mockResolvedValue([platform("youtube"), platform("mock")]);
  mocks.listAccounts.mockResolvedValue([ytAccount]);
});

describe("AccountsSection", () => {
  it("lists accounts with status and readable permissions", async () => {
    renderSection();
    expect(await screen.findByText("gh0stymc")).toBeInTheDocument();
    expect(screen.getByText("@gh0stymc")).toBeInTheDocument();
    expect(screen.getByText("Conectada")).toBeInTheDocument();
    expect(screen.getByText(/Ver las analíticas de YouTube/)).toBeInTheDocument();
    // Only OAuth platforms get a connect button.
    expect(screen.getByRole("button", { name: "Conectar YouTube" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Conectar Mock" })).toBeNull();
  });

  it("disables connecting when the client ID is missing", async () => {
    mocks.listPlatforms.mockResolvedValue([platform("youtube", false)]);
    mocks.listAccounts.mockResolvedValue([]);
    renderSection();
    expect(await screen.findByRole("button", { name: "Conectar YouTube" })).toBeDisabled();
    expect(screen.getByText(/client ID/)).toBeInTheDocument();
    expect(screen.getByText("Conecta tu primera red para empezar.")).toBeInTheDocument();
  });

  it("connects and can cancel while waiting for the browser", async () => {
    let reject: (e: unknown) => void = () => {};
    mocks.connectAccount.mockReturnValue(new Promise((_, r) => (reject = r)));
    mocks.cancelConnect.mockImplementation(async () => reject({ code: "AUTH_CANCELLED" }));
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: "Conectar YouTube" }));
    await waitFor(() => expect(mocks.connectAccount).toHaveBeenCalledWith("youtube"));
    expect(await screen.findByText(/navegador/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(mocks.cancelConnect).toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByText(/navegador/)).toBeNull());
    // A user cancellation is not shown as an error.
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("shows backend errors translated", async () => {
    mocks.connectAccount.mockRejectedValue({ code: "KEYRING_UNAVAILABLE" });
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: "Conectar YouTube" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/llavero del sistema/);
  });

  it("asks for confirmation before disconnecting and warns if revocation failed", async () => {
    mocks.disconnectAccount.mockResolvedValue({ revoked: false });
    renderSection();
    fireEvent.click(await screen.findByRole("button", { name: "Desconectar" }));
    expect(mocks.disconnectAccount).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Sí, desconectar" }));
    await waitFor(() => expect(mocks.disconnectAccount).toHaveBeenCalledWith("acc-1"));
    fireEvent.click(await screen.findByRole("button", { name: "Abrir los permisos de la cuenta" }));
    expect(mocks.openUrl).toHaveBeenCalledWith("https://myaccount.google.com/connections");
  });

  it("offers reconnection when the token expired", async () => {
    mocks.listAccounts.mockResolvedValue([{ ...ytAccount, status: "needs_reauth" }]);
    mocks.connectAccount.mockResolvedValue(ytAccount);
    renderSection();
    expect(await screen.findByText("Reconexión necesaria")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Reconectar" }));
    await waitFor(() => expect(mocks.connectAccount).toHaveBeenCalledWith("youtube"));
  });
});
