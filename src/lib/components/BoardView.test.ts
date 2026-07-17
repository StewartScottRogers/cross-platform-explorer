/**
 * BoardView empty-state + project-picker (CPE-551). The board scans `<root>/Tickets/`; off-repo that's
 * empty and used to read as "broken". These assert the helpful empty-state renders and that choosing a
 * project repoints + persists the board root.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";

const invokeMock = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => []);
vi.mock("../invoke", () => ({ invoke: (...a: unknown[]) => (invokeMock as (...x: unknown[]) => unknown)(...a) }));
const openMock = vi.fn(async () => null as unknown);
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: (...a: unknown[]) => (openMock as (...x: unknown[]) => unknown)(...a) }));

import BoardView from "./BoardView.svelte";

// Default: no project auto-detected (find_project_root → null), and board_* return empty.
function defaultInvoke(cmd: string): unknown {
  return cmd === "find_project_root" ? null : [];
}

beforeEach(() => {
  try { localStorage.clear(); } catch { /* ignore */ }
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => defaultInvoke(cmd));
  openMock.mockReset();
  openMock.mockResolvedValue(null);
});

describe("BoardView empty-state (CPE-551)", () => {
  it("shows a choose-project prompt (not a blank panel) when the folder has no Tickets/", async () => {
    render(BoardView, { root: "/some/browsed/folder" });
    expect(await screen.findByText("No tickets found here.")).toBeTruthy();
    expect(screen.getAllByText("/some/browsed/folder").length).toBeGreaterThan(0); // the searched root is shown
    expect(screen.getByText("📁 Choose a project folder…")).toBeTruthy();
  });

  it("chooseProject repoints the board at the picked folder and remembers it", async () => {
    openMock.mockResolvedValue("/picked/project");
    render(BoardView, { root: "/some/browsed/folder" });
    await screen.findByText("No tickets found here.");
    invokeMock.mockClear();

    await fireEvent.click(screen.getByText("📁 Choose a project folder…"));

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("board_cards", { root: "/picked/project" });
    });
    expect(localStorage.getItem("cpe.boardRoot")).toBe("/picked/project");
  });

  it("starts from the remembered project root, not the browsed folder", async () => {
    localStorage.setItem("cpe.boardRoot", "/remembered/project");
    render(BoardView, { root: "/some/other/folder" });
    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("board_cards", { root: "/remembered/project" });
    });
  });

  it("auto-detects the enclosing project root when none is saved (CPE-554)", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "find_project_root" ? "/detected/project" : []);
    render(BoardView, { root: "/detected/project/src/lib/components" });
    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("find_project_root", { start: "/detected/project/src/lib/components" });
      expect(invokeMock).toHaveBeenCalledWith("board_cards", { root: "/detected/project" });
    });
  });
});
