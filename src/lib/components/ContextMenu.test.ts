/**
 * ContextMenu render tests — focused on the "Copy to / Move to folder" actions (CPE-355),
 * which are gated to a real folder (canTerminal) and dispatch the right command.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ContextMenu from "./ContextMenu.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const base = {
  x: 10,
  y: 10,
  target: "item" as const,
  canPaste: false,
  selectionCount: 1,
  folderSelected: false,
  executableSelected: false,
  openIcon: "document",
  pinned: false,
  favorited: false,
  compressible: false,
  extractable: false,
  canTerminal: true,
  sameTypeExt: "",
};

describe("ContextMenu Copy to / Move to folder (CPE-355)", () => {
  it("offers both actions in a real folder and dispatches the right command", async () => {
    const { component } = render(ContextMenu, { props: { ...base } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    const copy = screen.getByText("Copy to folder…");
    const move = screen.getByText("Move to folder…");
    expect(copy).toBeTruthy();
    expect(move).toBeTruthy();

    await fireEvent.click(copy);
    expect(action).toHaveBeenCalledWith("copy-to");
    await fireEvent.click(move);
    expect(action).toHaveBeenCalledWith("move-to");
  });

  it("hides both actions when not in a real folder (Home/archive)", () => {
    render(ContextMenu, { props: { ...base, canTerminal: false } });
    expect(screen.queryByText("Copy to folder…")).toBeNull();
    expect(screen.queryByText("Move to folder…")).toBeNull();
  });

  it("shows Compare files only when comparable, and dispatches compare (CPE-418)", async () => {
    const { component } = render(ContextMenu, { props: { ...base, selectionCount: 2, comparable: true } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));
    await fireEvent.click(screen.getByText("Compare files"));
    expect(action).toHaveBeenCalledWith("compare");
  });

  it("hides Compare files when not comparable", () => {
    render(ContextMenu, { props: { ...base, selectionCount: 2, comparable: false } });
    expect(screen.queryByText("Compare files")).toBeNull();
  });

  it("shows Batch media… only for a multi-selection with an eligible image, and dispatches batch-media (CPE-1093)", async () => {
    const { component } = render(ContextMenu, { props: { ...base, selectionCount: 2, mediaEligible: true } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));
    await fireEvent.click(screen.getByText("Batch media…"));
    expect(action).toHaveBeenCalledWith("batch-media");
  });

  it("hides Batch media… when the selection has no eligible image or is a single item", () => {
    render(ContextMenu, { props: { ...base, selectionCount: 2, mediaEligible: false } });
    expect(screen.queryByText("Batch media…")).toBeNull();

    render(ContextMenu, { props: { ...base, selectionCount: 1, mediaEligible: true } });
    expect(screen.queryByText("Batch media…")).toBeNull();
  });
});
