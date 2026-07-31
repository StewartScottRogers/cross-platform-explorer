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

  it("New ▸ on a FILE item dispatches new-folder / new-file (create in the current folder) — CPE-1156", async () => {
    const { component } = render(ContextMenu, { props: { ...base, folderSelected: false } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("Folder"));
    expect(action).toHaveBeenCalledWith("new-folder");

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("Text file"));
    expect(action).toHaveBeenCalledWith("new-file");
  });

  it("New ▸ on a FOLDER item dispatches new-folder-in / new-file-in (create inside that folder) — CPE-1156", async () => {
    const { component } = render(ContextMenu, { props: { ...base, folderSelected: true } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("Folder"));
    expect(action).toHaveBeenCalledWith("new-folder-in");

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("Text file"));
    expect(action).toHaveBeenCalledWith("new-file-in");
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

// The empty-area (background) menu brought to Windows 11 parity (CPE-1153): New ▸ / View ▸ / Sort by ▸
// submenus, Undo, and background Properties.
const empty = {
  x: 10,
  y: 10,
  target: "empty" as const,
  canPaste: false,
  canTerminal: true,
  view: "details" as const,
  sortKey: "name" as const,
  sortDir: "asc" as const,
  canUndo: false,
  undoLabel: "",
};

/** Open a submenu by its parent label and return the flyout's <menu> element. */
async function openSubmenu(label: string): Promise<HTMLElement> {
  const parent = screen.getByText(label).closest("button")!;
  await fireEvent.mouseEnter(parent.parentElement!); // wrapper .submenu handles hover-open
  const flyout = document.querySelector(".flyout") as HTMLElement;
  expect(flyout).toBeTruthy();
  return flyout;
}

describe("ContextMenu empty-area Windows 11 parity (CPE-1153)", () => {
  it("New ▸ opens and its items dispatch new-folder / new-file", async () => {
    const { component } = render(ContextMenu, { props: { ...empty } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("Folder"));
    expect(action).toHaveBeenCalledWith("new-folder");

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("Text file"));
    expect(action).toHaveBeenCalledWith("new-file");
  });

  it("View ▸ opens, checkmarks the current mode, and selecting a mode dispatches view:<mode>", async () => {
    const { component } = render(ContextMenu, { props: { ...empty, view: "icons" } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    await openSubmenu("View");
    // Current mode is checkmarked via aria-checked (single source of truth with the toolbar).
    const large = screen.getByText("Large icons").closest("button")!;
    expect(large.getAttribute("aria-checked")).toBe("true");
    const details = screen.getByText("Details").closest("button")!;
    expect(details.getAttribute("aria-checked")).toBe("false");

    await fireEvent.click(details);
    expect(action).toHaveBeenCalledWith("view:details");
  });

  it("Sort by ▸ opens, checkmarks the current key + direction, and dispatches sort:<key> / sortdir:<dir>", async () => {
    const { component } = render(ContextMenu, { props: { ...empty, sortKey: "size", sortDir: "desc" } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    await openSubmenu("Sort");
    expect(screen.getByText("Size").closest("button")!.getAttribute("aria-checked")).toBe("true");
    expect(screen.getByText("Name").closest("button")!.getAttribute("aria-checked")).toBe("false");
    expect(screen.getByText("Descending").closest("button")!.getAttribute("aria-checked")).toBe("true");
    expect(screen.getByText("Ascending").closest("button")!.getAttribute("aria-checked")).toBe("false");

    await fireEvent.click(screen.getByText("Date modified"));
    expect(action).toHaveBeenCalledWith("sort:modified");
    await openSubmenu("Sort");
    await fireEvent.click(screen.getByText("Ascending"));
    expect(action).toHaveBeenCalledWith("sortdir:asc");
  });

  it("Undo is disabled when the stack is empty and enabled (with its label) when not", async () => {
    const { unmount } = render(ContextMenu, { props: { ...empty, canUndo: false } });
    expect((screen.getByText("Undo").closest("button") as HTMLButtonElement).disabled).toBe(true);
    unmount();

    const { component } = render(ContextMenu, { props: { ...empty, canUndo: true, undoLabel: "Rename to a.txt" } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));
    const undoBtn = screen.getByText(/Undo/).closest("button") as HTMLButtonElement;
    expect(undoBtn.disabled).toBe(false);
    expect(undoBtn.textContent).toContain("Rename to a.txt");
    await fireEvent.click(undoBtn);
    expect(action).toHaveBeenCalledWith("undo");
  });

  it("offers background Properties for the current folder", async () => {
    const { component } = render(ContextMenu, { props: { ...empty } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));
    await fireEvent.click(screen.getByText("Properties"));
    expect(action).toHaveBeenCalledWith("properties-folder");
  });

  it("opens a submenu via the keyboard (Right-arrow) and closes it (Left-arrow)", async () => {
    render(ContextMenu, { props: { ...empty } });
    const parent = screen.getByText("View").closest("button")!;
    expect(document.querySelector(".flyout")).toBeNull();

    await fireEvent.keyDown(parent, { key: "ArrowRight" });
    const flyout = document.querySelector(".flyout") as HTMLElement;
    expect(flyout).toBeTruthy();
    expect(parent.getAttribute("aria-expanded")).toBe("true");

    await fireEvent.keyDown(flyout, { key: "ArrowLeft" });
    expect(document.querySelector(".flyout")).toBeNull();
    expect(parent.getAttribute("aria-expanded")).toBe("false");
  });

  it("keeps the existing background rows working (Paste gated, Refresh, Select all)", async () => {
    const { component } = render(ContextMenu, { props: { ...empty, canPaste: true } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));
    expect((screen.getByText("Paste").closest("button") as HTMLButtonElement).disabled).toBe(false);
    await fireEvent.click(screen.getByText("Refresh"));
    expect(action).toHaveBeenCalledWith("refresh");
    await fireEvent.click(screen.getByText("Select all"));
    expect(action).toHaveBeenCalledWith("select-all");
  });
});
