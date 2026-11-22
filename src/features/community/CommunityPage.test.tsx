import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import "../../i18n";

const mocks = vi.hoisted(() => ({
  listAccounts: vi.fn(),
  listPlatforms: vi.fn(),
  listInbox: vi.fn(),
  inboxCounts: vi.fn(),
  listQuestionClusters: vi.fn(),
  commentScopes: vi.fn(),
  replyToComment: vi.fn(),
  setCommentStatus: vi.fn(),
  setCommentCategory: vi.fn(),
  syncComments: vi.fn(),
  clusterToIdea: vi.fn(),
}));
vi.mock("../../lib/bindings", () => ({ commands: mocks }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

import { CommunityPage } from "./CommunityPage";

const comment = {
  id: "c1",
  accountId: "a1",
  platform: "youtube",
  contentId: null,
  contentTitle: "Mi vídeo",
  contentUrl: null,
  authorName: "Ana",
  authorExternalId: null,
  text: "¿Qué mod usas?",
  publishedAt: "2026-11-20T10:00:00Z",
  likeCount: 2,
  status: "pending",
  category: "question",
  categorySource: "rules",
  phishingScore: 0.9,
  phishingReasons: ["shortener"],
  clusterId: "k1",
  replies: [],
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listAccounts.mockResolvedValue([
    {
      id: "a1",
      platform: "youtube",
      displayName: "gh0sty",
      status: "active",
      grantedScopes: ["s"],
    },
  ]);
  mocks.listPlatforms.mockResolvedValue([
    {
      id: "youtube",
      displayName: "YouTube",
      brandColor: "#f00",
      capabilities: { comments: "readReply" },
    },
  ]);
  mocks.listInbox.mockResolvedValue([comment]);
  mocks.inboxCounts.mockResolvedValue({ pending: 1, questions: 1, suspicious: 1 });
  mocks.listQuestionClusters.mockResolvedValue([
    { id: "k1", label: "¿Qué mod usas?", size: 3, ideaId: null, samples: ["a", "b"] },
  ]);
  mocks.commentScopes.mockResolvedValue(["s"]);
  mocks.replyToComment.mockResolvedValue(null);
});

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <CommunityPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("CommunityPage", () => {
  it("shows comments with category, scam warning and repeated questions", async () => {
    renderPage();
    expect(await screen.findByText("¿Qué mod usas?", { selector: "p" })).toBeTruthy();
    expect(screen.getAllByText(/Posible estafa/).length).toBeGreaterThan(0);
    expect(screen.getByText("Preguntas repetidas")).toBeTruthy();
    expect(mocks.listInbox).toHaveBeenCalledWith(expect.objectContaining({ status: "pending" }));
  });

  it("sends a reply", async () => {
    renderPage();
    fireEvent.click(await screen.findByText("Responder"));
    fireEvent.change(screen.getByPlaceholderText("Escribe tu respuesta…"), {
      target: { value: "Sodium" },
    });
    await waitFor(() =>
      expect((screen.getByText("Enviar respuesta") as HTMLButtonElement).disabled).toBe(false),
    );
    fireEvent.click(screen.getByText("Enviar respuesta"));
    await waitFor(() => expect(mocks.replyToComment).toHaveBeenCalledWith("c1", "Sodium"));
  });
});
