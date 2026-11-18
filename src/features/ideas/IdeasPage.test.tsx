import { describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import "../../i18n";
import type { Idea } from "../../lib/bindings";

const mocks = vi.hoisted(() => ({
  listIdeas: vi.fn(),
  listPlatforms: vi.fn(),
  createIdea: vi.fn(),
  moveIdea: vi.fn(),
}));
vi.mock("../../lib/bindings", () => ({ commands: mocks }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

import { IdeasPage } from "./IdeasPage";

const idea = (id: string, stage: string, position: number): Idea => ({
  id,
  title: `Idea ${id}`,
  notes: null,
  stage,
  position,
  targetPlatforms: ["youtube"],
  dueAt: null,
  files: [],
  createdAt: "t",
  updatedAt: "t",
});

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <IdeasPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listPlatforms.mockResolvedValue([
    { id: "youtube", displayName: "YouTube", brandColor: "#f00" },
  ]);
  mocks.listIdeas.mockResolvedValue([
    idea("b", "idea", 2),
    idea("a", "idea", 1),
    idea("c", "script", 1),
  ]);
});

describe("IdeasPage", () => {
  it("shows cards in their columns, ordered by position", async () => {
    renderPage();
    const col = await screen.findByRole("region", { name: "Idea" });
    const titles = within(col)
      .getAllByText(/^Idea [abc]$/)
      .map((n) => n.textContent);
    expect(titles).toEqual(["Idea a", "Idea b"]);
    expect(
      within(screen.getByRole("region", { name: "Guion" })).getByText("Idea c"),
    ).toBeInTheDocument();
  });

  it("creates a card in the chosen column", async () => {
    mocks.createIdea.mockResolvedValue(idea("d", "recorded", 1));
    renderPage();
    const col = await screen.findByRole("region", { name: "Grabado" });
    fireEvent.click(within(col).getByRole("button", { name: "+ Añadir" }));
    fireEvent.change(within(col).getByPlaceholderText("Título de la idea"), {
      target: { value: "Vídeo de Minecraft" },
    });
    fireEvent.click(within(col).getByRole("button", { name: "Añadir" }));
    await waitFor(() =>
      expect(mocks.createIdea).toHaveBeenCalledWith("recorded", {
        title: "Vídeo de Minecraft",
        notes: null,
        targetPlatforms: [],
        dueAt: null,
      }),
    );
  });
});
