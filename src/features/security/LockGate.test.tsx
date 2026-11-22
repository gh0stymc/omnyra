import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import "../../i18n";

const mocks = vi.hoisted(() => ({ lockStatus: vi.fn(), unlock: vi.fn(), lockNow: vi.fn() }));
vi.mock("../../lib/bindings", () => ({ commands: mocks }));

import { LockGate } from "./LockGate";

const wrap = () =>
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <LockGate>
        <p>secret content</p>
      </LockGate>
    </QueryClientProvider>,
  );

beforeEach(() => vi.clearAllMocks());

describe("LockGate", () => {
  it("shows the app when not locked", async () => {
    mocks.lockStatus.mockResolvedValue({
      enabled: false,
      locked: false,
      retryAfterS: 0,
      idleMinutes: 10,
    });
    wrap();
    expect(await screen.findByText("secret content")).toBeTruthy();
  });

  it("hides the app until the right PIN is entered", async () => {
    mocks.lockStatus.mockResolvedValueOnce({
      enabled: true,
      locked: true,
      retryAfterS: 0,
      idleMinutes: 10,
    });
    mocks.unlock.mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    wrap();
    const input = await screen.findByLabelText("PIN");
    expect(screen.queryByText("secret content")).toBeNull();
    fireEvent.change(input, { target: { value: "0000" } });
    mocks.lockStatus.mockResolvedValueOnce({
      enabled: true,
      locked: true,
      retryAfterS: 0,
      idleMinutes: 10,
    });
    fireEvent.click(screen.getByText("Desbloquear"));
    expect(await screen.findByText("PIN incorrecto.")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("PIN"), { target: { value: "1234" } });
    mocks.lockStatus.mockResolvedValue({
      enabled: true,
      locked: false,
      retryAfterS: 0,
      idleMinutes: 10,
    });
    fireEvent.click(screen.getByText("Desbloquear"));
    await waitFor(() => expect(screen.getByText("secret content")).toBeTruthy());
    expect(mocks.unlock).toHaveBeenLastCalledWith("1234");
  });
});
