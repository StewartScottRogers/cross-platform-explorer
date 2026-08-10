/**
 * ContextMenu render tests — focused on the "Copy to / Move to folder" actions (CPE-355), gated on their
 * own `copyMoveEligible` prop (split out from `canTerminal` by CPE-1384 so a pane-B-opened menu can offer
 * them without also turning on the pane-A-only terminal/console/compress rows `canTerminal` still gates).
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/svelte";
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
  copyMoveEligible: true,
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
    render(ContextMenu, { props: { ...base, canTerminal: false, copyMoveEligible: false } });
    expect(screen.queryByText("Copy to folder…")).toBeNull();
    expect(screen.queryByText("Move to folder…")).toBeNull();
  });

  it("offers both actions for a pane-B row (copyMoveEligible true) even though canTerminal stays off there (CPE-1384)", () => {
    render(ContextMenu, { props: { ...base, canTerminal: false, copyMoveEligible: true } });
    expect(screen.getByText("Copy to folder…")).toBeTruthy();
    expect(screen.getByText("Move to folder…")).toBeTruthy();
  });

  it("shows Repair link… only when linkBroken, and dispatches repair-link (CPE-1209)", async () => {
    render(ContextMenu, { props: { ...base, linkBroken: false } });
    expect(screen.queryByText("Repair link…")).toBeNull();

    const { component } = render(ContextMenu, { props: { ...base, linkBroken: true } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));
    const row = screen.getByText("Repair link…");
    expect(row).toBeTruthy();
    await fireEvent.click(row);
    expect(action).toHaveBeenCalledWith("repair-link");
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

  it("New ▸ offers the expanded file types and dispatches new-file:<ext> on a FILE item (CPE-1161)", async () => {
    const { component } = render(ContextMenu, { props: { ...base, folderSelected: false } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("Markdown"));
    expect(action).toHaveBeenCalledWith("new-file:md");

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("JSON"));
    expect(action).toHaveBeenCalledWith("new-file:json");

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("Python"));
    expect(action).toHaveBeenCalledWith("new-file:py");

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("Compressed (zipped) Folder"));
    expect(action).toHaveBeenCalledWith("new-file:zip");
  });

  it("New ▸ on a FOLDER item dispatches new-file-in:<ext> (create the typed file inside it) — CPE-1161", async () => {
    const { component } = render(ContextMenu, { props: { ...base, folderSelected: true } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("Rich Text"));
    expect(action).toHaveBeenCalledWith("new-file-in:rtf");

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("YAML"));
    expect(action).toHaveBeenCalledWith("new-file-in:yaml");
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

// Archive password support (CPE-1182) + extract-to/tar.gz format choice (CPE-1183): new rows
// alongside the existing zip-only Extract/Compress, gated by the same extractable/compressible flags.
describe("ContextMenu archive rows — extract-to + compress format/password (CPE-1182/1183)", () => {
  it("offers Extract to… alongside Extract when extractable, and dispatches extract-to", async () => {
    const { component } = render(ContextMenu, { props: { ...base, extractable: true } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    expect(screen.getByText("Extract")).toBeTruthy();
    const extractTo = screen.getByText("Extract to…");
    expect(extractTo).toBeTruthy();
    await fireEvent.click(extractTo);
    expect(action).toHaveBeenCalledWith("extract-to");
  });

  it("hides Extract/Extract to… when not extractable", () => {
    render(ContextMenu, { props: { ...base, extractable: false } });
    expect(screen.queryByText("Extract")).toBeNull();
    expect(screen.queryByText("Extract to…")).toBeNull();
  });

  it("offers Compress to .tar.gz and Compress with password… alongside the zip-only Compress, dispatching the right actions", async () => {
    const { component } = render(ContextMenu, { props: { ...base, compressible: true } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    await fireEvent.click(screen.getByText("Compress to ZIP"));
    expect(action).toHaveBeenCalledWith("compress");

    await fireEvent.click(screen.getByText("Compress to .tar.gz"));
    expect(action).toHaveBeenCalledWith("compress-targz");

    await fireEvent.click(screen.getByText("Compress with password…"));
    expect(action).toHaveBeenCalledWith("compress-password");
  });

  it("hides all three compress rows when not compressible", () => {
    render(ContextMenu, { props: { ...base, compressible: false } });
    expect(screen.queryByText("Compress to ZIP")).toBeNull();
    expect(screen.queryByText("Compress to .tar.gz")).toBeNull();
    expect(screen.queryByText("Compress with password…")).toBeNull();
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

  it("New ▸ offers the expanded typed file types and dispatches new-file:<ext> (CPE-1161)", async () => {
    const { component } = render(ContextMenu, { props: { ...empty } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("HTML"));
    expect(action).toHaveBeenCalledWith("new-file:html");

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("Compressed (zipped) Folder"));
    expect(action).toHaveBeenCalledWith("new-file:zip");
  });

  it("New ▸ offers New Link… and dispatches new-link (CPE-1207)", async () => {
    const { component } = render(ContextMenu, { props: { ...empty } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    await openSubmenu("New");
    await fireEvent.click(screen.getByText("New Link…"));
    expect(action).toHaveBeenCalledWith("new-link");
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

  it("offers a Command Palette row that dispatches command-palette (CPE-1164)", async () => {
    const { component } = render(ContextMenu, { props: { ...empty } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));
    const row = screen.getByText("Command palette").closest("button")!;
    expect(row).toBeTruthy();
    expect(row.textContent).toContain("Ctrl+Shift+P");
    await fireEvent.click(row);
    expect(action).toHaveBeenCalledWith("command-palette");
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

// Shared-tab row menu (CPE-1163) — the `target:"home-item"` + `homeView:"shared"` branch offers
// Open / Copy path / Properties plus a kind-specific Disconnect (mapped) or Remove (user).
const homeShared = {
  ...base,
  target: "home-item" as const,
  homeView: "shared" as const,
  homeIsDir: true,
};

describe("ContextMenu Shared row menu (CPE-1163)", () => {
  it("a mapped drive offers Disconnect (not Remove) and dispatches share-disconnect", async () => {
    const { component } = render(ContextMenu, { props: { ...homeShared, homeKind: "mapped" } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    expect(screen.getByText("Open")).toBeTruthy();
    expect(screen.getByText(/Copy as path|Copy path/i)).toBeTruthy();
    expect(screen.queryByText(/Remove network location/i)).toBeNull();

    await fireEvent.click(screen.getByText("Disconnect"));
    expect(action).toHaveBeenCalledWith("share-disconnect");
  });

  it("a user-added location offers Remove (not Disconnect) and dispatches share-remove", async () => {
    const { component } = render(ContextMenu, { props: { ...homeShared, homeKind: "user" } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    expect(screen.queryByText("Disconnect")).toBeNull();
    await fireEvent.click(screen.getByText(/Remove network location/i));
    expect(action).toHaveBeenCalledWith("share-remove");
  });

  it("an OS mount offers neither Disconnect nor Remove, but still Open + Copy path", async () => {
    render(ContextMenu, { props: { ...homeShared, homeKind: "mount" } });
    expect(screen.getByText("Open")).toBeTruthy();
    expect(screen.queryByText("Disconnect")).toBeNull();
    expect(screen.queryByText(/Remove network location/i)).toBeNull();
  });
});

// Open/close-race hardening (CPE-1160). The window dismisser must never be tripped by the very
// `contextmenu` event that OPENED the menu — the footgun that bit CPE-1154/1157/1159 whenever an
// opener forgot `e.stopPropagation()`. These tests genuinely exercise the `<svelte:window>`
// dismisser: they dispatch a *bubbling* event on `document.body` so it propagates up to the window
// listener the component attaches, exactly as a real opener's event does. Open-time is detected via
// `performance.now()` (mocked here so we can place events inside / outside the ~50 ms guard window).
describe("ContextMenu open/close-race hardening (CPE-1160)", () => {
  function fireBubblingContextmenu(target: EventTarget = document.body) {
    // A forgetful opener's event: bubbles, no stopPropagation — reaches the window dismisser.
    target.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  }

  it("does NOT self-close when the OPENING contextmenu bubbles to window without stopPropagation", () => {
    // Freeze time at open — the opening event arrives in the same tick (elapsed ≈ 0 ms).
    const now = vi.spyOn(performance, "now").mockReturnValue(1_000);
    const { component } = render(ContextMenu, { props: { ...base } });
    const close = vi.fn();
    component.$on("close", close);

    // Simulate the forgetful opener: the SAME contextmenu event bubbles up to window.
    fireBubblingContextmenu();

    expect(close).not.toHaveBeenCalled();
    expect(document.querySelector(".ctx")).toBeTruthy(); // menu is still mounted/visible
    now.mockRestore();
  });

  it("a LATER right-click elsewhere STILL closes it (regression — legitimate dismissal survives)", () => {
    const now = vi.spyOn(performance, "now").mockReturnValue(1_000);
    const { component } = render(ContextMenu, { props: { ...base } });
    const close = vi.fn();
    component.$on("close", close);

    // A deliberate human right-click well past the open-guard window.
    now.mockReturnValue(1_000 + 500);
    fireBubblingContextmenu();

    expect(close).toHaveBeenCalledTimes(1);
    now.mockRestore();
  });

  it("a left-click outside STILL closes it immediately, even in the same tick (clicks are never gated)", () => {
    // A click never OPENS the menu, so it must dismiss without any guard delay.
    const now = vi.spyOn(performance, "now").mockReturnValue(1_000);
    const { component } = render(ContextMenu, { props: { ...base } });
    const close = vi.fn();
    component.$on("close", close);

    document.body.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    expect(close).toHaveBeenCalledTimes(1);
    now.mockRestore();
  });

  it("right-clicking INSIDE the menu does not close it (stopPropagation on the menu's own DOM)", () => {
    // Past the guard window, so only the menu's own stopPropagation can be protecting it.
    const now = vi.spyOn(performance, "now").mockReturnValue(1_000);
    const { component } = render(ContextMenu, { props: { ...base } });
    const close = vi.fn();
    component.$on("close", close);
    now.mockReturnValue(1_000 + 500);

    const menu = document.querySelector(".ctx") as HTMLElement;
    fireBubblingContextmenu(menu); // bubbles up to .ctx, which stops it before window

    expect(close).not.toHaveBeenCalled();
    now.mockRestore();
  });
});

// Certificate management (CPE-1424, epic CPE-1417): cert/CSR/JWT rows gated by file type, plus the
// folder-row and empty-area "Create certificate here…" entries gated by certCreateEligible.
describe("ContextMenu certificate management (CPE-1424, epic CPE-1417)", () => {
  it("a .csr file offers 'Issue cert from this CSR…' (not 'Sign with this as CA…') plus Inspect", async () => {
    const { component } = render(ContextMenu, { props: { ...base, certFileKind: "csr" as const } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    expect(screen.getByText("Issue cert from this CSR…")).toBeTruthy();
    expect(screen.queryByText("Sign with this as CA…")).toBeNull();
    await fireEvent.click(screen.getByText("Issue cert from this CSR…"));
    expect(action).toHaveBeenCalledWith("cert-issue-from-csr");

    await fireEvent.click(screen.getByText("Inspect"));
    expect(action).toHaveBeenCalledWith("cert-inspect");
  });

  it("a .pem/.crt/.cer/.der file offers 'Sign with this as CA…' (not the CSR row) plus Inspect", async () => {
    const { component } = render(ContextMenu, { props: { ...base, certFileKind: "cert" as const } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    expect(screen.getByText("Sign with this as CA…")).toBeTruthy();
    expect(screen.queryByText("Issue cert from this CSR…")).toBeNull();
    await fireEvent.click(screen.getByText("Sign with this as CA…"));
    expect(action).toHaveBeenCalledWith("cert-sign-as-ca");
  });

  it("hides all cert rows for an ordinary file", () => {
    render(ContextMenu, { props: { ...base, certFileKind: "" as const } });
    expect(screen.queryByText("Issue cert from this CSR…")).toBeNull();
    expect(screen.queryByText("Sign with this as CA…")).toBeNull();
    expect(screen.queryByText("Inspect")).toBeNull();
  });

  it("a .jwt file offers 'Inspect JWT' and dispatches jwt-inspect", async () => {
    const { component } = render(ContextMenu, { props: { ...base, jwtSelected: true } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));
    const row = screen.getByText("Inspect JWT");
    expect(row).toBeTruthy();
    await fireEvent.click(row);
    expect(action).toHaveBeenCalledWith("jwt-inspect");
  });

  it("hides Inspect JWT for a non-JWT file", () => {
    render(ContextMenu, { props: { ...base, jwtSelected: false } });
    expect(screen.queryByText("Inspect JWT")).toBeNull();
  });

  it("offers 'Create certificate here…' on a folder row when certCreateEligible, and dispatches cert-create-here", async () => {
    const { component } = render(ContextMenu, {
      props: { ...base, folderSelected: true, certCreateEligible: true },
    });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));
    const row = screen.getByText("Create certificate here…");
    expect(row).toBeTruthy();
    await fireEvent.click(row);
    expect(action).toHaveBeenCalledWith("cert-create-here");
  });

  it("hides 'Create certificate here…' on a folder row when not certCreateEligible (Home/archive)", () => {
    render(ContextMenu, { props: { ...base, folderSelected: true, certCreateEligible: false } });
    expect(screen.queryByText("Create certificate here…")).toBeNull();
  });

  it("hides 'Create certificate here…' on a FILE row even when certCreateEligible", () => {
    render(ContextMenu, { props: { ...base, folderSelected: false, certCreateEligible: true } });
    expect(screen.queryByText("Create certificate here…")).toBeNull();
  });

  it("offers 'Create certificate here…' on the empty-area menu when certCreateEligible, and dispatches cert-create-here", async () => {
    const { component } = render(ContextMenu, { props: { ...empty, certCreateEligible: true } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));
    const row = screen.getByText("Create certificate here…");
    expect(row).toBeTruthy();
    await fireEvent.click(row);
    expect(action).toHaveBeenCalledWith("cert-create-here");
  });

  it("hides 'Create certificate here…' on the empty-area menu when not certCreateEligible", () => {
    render(ContextMenu, { props: { ...empty, certCreateEligible: false } });
    expect(screen.queryByText("Create certificate here…")).toBeNull();
  });
});

describe("ContextMenu Securely delete… (CPE-1240, epic CPE-738)", () => {
  it("hides the row when not shreddable (default)", () => {
    render(ContextMenu, { props: { ...base } });
    expect(screen.queryByText("Securely delete…")).toBeNull();
  });

  it("shows the row and dispatches 'shred' when shreddable", async () => {
    const { component } = render(ContextMenu, { props: { ...base, shreddable: true } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    const row = screen.getByText("Securely delete…");
    expect(row).toBeTruthy();
    await fireEvent.click(row);
    expect(action).toHaveBeenCalledWith("shred");
  });
});

describe("ContextMenu Split file… / Join parts… (CPE-1509, parent CPE-1491)", () => {
  it("hides both rows by default", () => {
    render(ContextMenu, { props: { ...base } });
    expect(screen.queryByText("Split file…")).toBeNull();
    expect(screen.queryByText("Join parts…")).toBeNull();
  });

  it("shows 'Split file…' and dispatches split-file when splitEligible", async () => {
    const { component } = render(ContextMenu, { props: { ...base, splitEligible: true } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    const row = screen.getByText("Split file…");
    expect(row).toBeTruthy();
    expect(screen.queryByText("Join parts…")).toBeNull();
    await fireEvent.click(row);
    expect(action).toHaveBeenCalledWith("split-file");
  });

  it("shows 'Join parts…' and dispatches join-parts when joinEligible", async () => {
    const { component } = render(ContextMenu, { props: { ...base, joinEligible: true } });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    const row = screen.getByText("Join parts…");
    expect(row).toBeTruthy();
    expect(screen.queryByText("Split file…")).toBeNull();
    await fireEvent.click(row);
    expect(action).toHaveBeenCalledWith("join-parts");
  });
});

describe("ContextMenu Run command ▸ (CPE-1577: Context surface wired)", () => {
  it("hides the submenu entirely when no user commands are bound to Context", () => {
    render(ContextMenu, { props: { ...base, userCommands: [] } });
    expect(screen.queryByText("Run command")).toBeNull();
  });

  it("lists each bound command and dispatches uc:<id> on click", async () => {
    const { component } = render(ContextMenu, {
      props: {
        ...base,
        userCommands: [
          { id: "uc_a", name: "Open in VS Code" },
          { id: "uc_b", name: "Ripgrep here" },
        ],
      },
    });
    const action = vi.fn();
    component.$on("action", (e) => action(e.detail));

    const flyout = await openSubmenu("Run command");
    expect(within(flyout).getByText("Open in VS Code")).toBeTruthy();
    expect(within(flyout).getByText("Ripgrep here")).toBeTruthy();

    await fireEvent.click(within(flyout).getByText("Ripgrep here"));
    expect(action).toHaveBeenCalledWith("uc:uc_b");
  });

  it("Run command ▸ and Run macro ▸ coexist without colliding", async () => {
    render(ContextMenu, {
      props: {
        ...base,
        macros: ["Tag as reviewed"],
        userCommands: [{ id: "uc_a", name: "Open in VS Code" }],
      },
    });
    expect(screen.getByText("Run macro")).toBeTruthy();
    expect(screen.getByText("Run command")).toBeTruthy();
  });
});
