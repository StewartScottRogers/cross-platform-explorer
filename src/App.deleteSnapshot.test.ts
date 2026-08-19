/**
 * CPE-1370 review (Reviewer + UAT follow-up on CPE-1370/1373/1374): two issues found by code-read,
 * covered here at the App-wiring level (not just the pure helpers) because they're about how
 * `handleKeydown`/`askDelete`/`doDelete` are actually wired, not about a pure algorithm:
 *
 * 1. CRITICAL data-loss hole: `askDelete` opens a confirm dialog for one pane's selection, but
 *    `activePane` can still change while the dialog is showing (nothing blocked Tab/pane-focus clicks
 *    from reaching `handleKeydown` underneath it). If `doDelete` re-derived its target from live state
 *    instead of the snapshot `askDelete` captured, clicking "Delete permanently" after the active pane
 *    changed would permanently delete the OTHER pane's files — never shown, never confirmed.
 *    `snapshotConfirmTarget`'s own pure-function coverage lives in `lib/selection.test.ts`; this proves
 *    the actual App wiring uses it correctly end-to-end, plus the defense-in-depth Tab guard.
 * 2. Escape (in the same key `switch` as every pane-routed case) still hard-wrote pane A's selection
 *    instead of going through the active pane like Arrow/Home/End/Ctrl+A/etc. already do.
 *
 * Same mounted-App-with-mocked-backend harness as App.selectionReorder.test.ts / App.replayGuards.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings, saveDualPane, savePaneBPath } from "./lib/settings";
import type { DirEntry, Place } from "./lib/types";

const file = (name: string, dir: string): DirEntry => ({
  name,
  path: `${dir}\\${name}`,
  is_dir: false,
  size: 10,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension: "txt",
  hidden: false,
  is_symlink: false,
});

const PATH_A = "C:\\d";
const PATH_B = "C:\\dB";
const drives: Place[] = [{ name: "Local Disk (C:)", path: PATH_A, kind: "drive" }];
const entriesA: DirEntry[] = [file("alpha.txt", PATH_A)];
const entriesB: DirEntry[] = [file("bravo.txt", PATH_B)];

const { invoke, Channel } = vi.hoisted(() => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: (batch: unknown) => void = () => {};
  },
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke, convertFileSrc: (p: string) => `asset://${p}`, Channel }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

let deletePermanentCalls: string[][] = [];
/** The `confirmed` flag each `delete_permanent` call carried (CPE-1651) — the backend refuses the call
 *  without it, so this records whether the UI actually supplied the user's consent. */
let deletePermanentConsent: unknown[] = [];

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  deletePermanentCalls = [];
  deletePermanentConsent = [];
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const listingFor = (path: unknown) => (path === PATH_B ? entriesB : entriesA);
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries: listingFor(args.path), filtered: 0 };
      case "list_dir_stream": {
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        const data = listingFor(args.path);
        ch.onmessage(data);
        return data.length;
      }
      case "parent_dir": return null;
      case "delete_permanent": {
        const paths = args.paths as string[];
        deletePermanentCalls.push(paths);
        deletePermanentConsent.push(args.confirmed);
        return paths.map((p) => ({ path: p, ok: true, error: "" }));
      }
      case "delete_to_trash": {
        const paths = args.paths as string[];
        return paths.map((p) => ({ path: p, ok: true, error: "" }));
      }
      default: return null;
    }
  });
});

/** Boot straight into dual-pane (persisted before render, like a restored session), navigate pane A
 *  into its drive, and wait for both panes' listings to settle. Returns each pane's `.pane-col` wrapper
 *  — clicking it (not a row inside it, which stops propagation) is how the app itself sets `activePane`. */
async function bootDualPane() {
  saveDualPane(true);
  savePaneBPath(PATH_B);
  render(App);

  // Pane B auto-restores to its persisted folder on mount (dualPane was persisted true) — wait for it
  // to fully settle FIRST, then navigate pane A. Both panes' first load shares generation number 1 in
  // the (dev-only) perf-mark instrumentation (a global `performance.mark` namespace, not per-pane), so
  // running them sequentially instead of letting both loads race avoids a same-named-mark collision
  // that's a pre-existing test-environment quirk, unrelated to this fix.
  await waitFor(() => expect(screen.getByText("bravo.txt")).toBeTruthy());

  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());

  const paneAWrap = screen.getByText("alpha.txt").closest(".pane-col") as HTMLElement;
  const paneBWrap = screen.getByText("bravo.txt").closest(".pane-col") as HTMLElement;
  return { paneAWrap, paneBWrap };
}

describe("App — confirm-delete targets a frozen snapshot, not whatever pane is active when confirmed (CPE-1370 review)", () => {
  it("permanently deletes the pane confirmed at dialog-open time, even though the active pane changes before the click", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();

    // Pane A ALSO has a selection, so if the eventual delete used live state instead of the snapshot
    // it would delete a real, different file (alpha.txt) rather than merely no-op — a more telling
    // regression signal than an empty-selection no-op.
    await fireEvent.click(paneAWrap);
    await fireEvent.click(screen.getByText("alpha.txt"));

    // Now focus pane B (clicking its own wrapper — a row click stops propagation, matching the app's
    // real on:click wiring — see App.svelte's pane-col markup) and select bravo.txt there.
    await fireEvent.click(paneBWrap);
    await fireEvent.click(screen.getByText("bravo.txt"));

    // Shift+Delete opens the "Delete permanently?" confirm dialog for pane B's selection.
    await fireEvent.keyDown(window, { key: "Delete", shiftKey: true });
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/bravo\.txt/)).toBeTruthy();

    // The active pane changes WHILE the dialog is still open — e.g. the user clicks back into pane A.
    // Pre-fix, `doDelete` re-derived its target from live `activePane` and would silently switch to
    // deleting pane A's alpha.txt here instead of the pane B bravo.txt the dialog showed and the user
    // actually confirmed.
    await fireEvent.click(paneAWrap);

    await fireEvent.click(within(dialog).getByText("Delete permanently"));

    await waitFor(() => expect(deletePermanentCalls.length).toBe(1));
    expect(deletePermanentCalls[0]).toEqual([`${PATH_B}\\bravo.txt`]); // NOT alpha.txt
  });

  it("Tab is inert while the confirm dialog is open (defense-in-depth) — the active pane can't flip underneath it", async () => {
    const { paneAWrap } = await bootDualPane();
    void paneAWrap; // pane A is already active by default

    await fireEvent.click(screen.getByText("alpha.txt"));
    await fireEvent.keyDown(window, { key: "Delete", shiftKey: true });
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/alpha\.txt/)).toBeTruthy();

    // Tab normally flips the active pane (CPE-1370) — it must be a no-op while the dialog is showing.
    await fireEvent.keyDown(window, { key: "Tab" });

    await fireEvent.click(within(dialog).getByText("Delete permanently"));
    await waitFor(() => expect(deletePermanentCalls.length).toBe(1));
    expect(deletePermanentCalls[0]).toEqual([`${PATH_A}\\alpha.txt`]); // still pane A
  });

  // CPE-1651: the backend now REFUSES `delete_permanent` unless the call carries explicit consent, so
  // the explorer's own delete path has to supply it — and only from the confirm dialog's Yes handler.
  it("supplies backend consent (confirmed: true) only from the confirm dialog's Yes, never before it", async () => {
    await bootDualPane();

    await fireEvent.click(screen.getByText("alpha.txt"));
    await fireEvent.keyDown(window, { key: "Delete", shiftKey: true });
    const dialog = await screen.findByRole("dialog");

    // Opening the dialog must not have called the destructive command at all.
    expect(deletePermanentCalls.length).toBe(0);

    await fireEvent.click(within(dialog).getByText("Delete permanently"));
    await waitFor(() => expect(deletePermanentCalls.length).toBe(1));

    // Consent is `true` — and it is a real boolean, not `undefined` sliding through as falsy, which is
    // what the pre-CPE-1651 call shape (no flag at all) would have produced.
    expect(deletePermanentConsent).toEqual([true]);
  });
});

describe("App — Escape routes through the active pane in dual-pane mode (CPE-1370 review)", () => {
  it("clears pane B's selection when pane B is active, leaving pane A's selection untouched", async () => {
    const { paneAWrap, paneBWrap } = await bootDualPane();

    // Select in pane A first, so we can prove Escape (fired while pane B is active) doesn't touch it.
    await fireEvent.click(paneAWrap);
    await fireEvent.click(screen.getByText("alpha.txt"));
    await waitFor(() =>
      expect(screen.getByText("alpha.txt").closest(".row")?.className).toContain("selected"),
    );

    // Focus + select in pane B.
    await fireEvent.click(paneBWrap);
    await fireEvent.click(screen.getByText("bravo.txt"));
    await waitFor(() =>
      expect(screen.getByText("bravo.txt").closest(".row")?.className).toContain("selected"),
    );

    await fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() =>
      expect(screen.getByText("bravo.txt").closest(".row")?.className).not.toContain("selected"),
    );
    // Pane A's selection is different state — Escape (routed to the ACTIVE pane, B) must not touch it.
    expect(screen.getByText("alpha.txt").closest(".row")?.className).toContain("selected");
  });
});
