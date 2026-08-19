/**
 * CPE-1614: the notice shown when a mutating action (Delete/Rename/Cut/Copy/Paste) is attempted
 * inside an open smart folder must (a) name the surface correctly — a smart folder is a single-tag
 * live view, NOT a saved search (a different, multi-condition feature; see CPE-1605, which fixed the
 * same conflation in the sidebar tooltip) — and (b) be routed through `$t()` so it actually translates
 * instead of hardcoded English baked into every locale (the bug this ticket fixes).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { smartFolders, saveSmartFolder } from "./lib/smartFolders";
import { translate } from "./lib/i18n";
import type { Place } from "./lib/types";

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];

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
// Opening a smart folder arms the CPE-1230 live-refresh listener, which wraps the REAL
// `@tauri-apps/api/event.listen` — that needs the Tauri IPC bridge (`window.__TAURI_INTERNALS__`) that
// doesn't exist in jsdom. Mock it to a no-op listener, same fix as `App.smartFolderLiveRefresh.test.ts`.
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  smartFolders.set([]); // the store is a module singleton across this file's tests — start clean
  Element.prototype.scrollIntoView = vi.fn();

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries: [], filtered: 0 };
      case "list_dir_stream": {
        const ch = args?.onEntry as { onmessage: (b: unknown) => void };
        ch.onmessage([]);
        return 0;
      }
      case "parent_dir": return null;
      case "entries_for_paths": return [];
      default: return null;
    }
  });
});

describe("smart folder blocked-action notice (CPE-1614)", () => {
  it("names the surface as a smart folder — not a saved search — and renders it via $t()", async () => {
    saveSmartFolder("Invoices", "invoice");
    render(App);
    await screen.findAllByText("Local Disk (C:)");

    const row = await screen.findByText("Invoices");
    await fireEvent.click(row);

    // Opening it puts the name in the breadcrumb too — proves the smart folder is now the active view.
    await waitFor(() => expect(screen.getAllByText("Invoices").length).toBeGreaterThan(1));

    // Delete requires no selection to be blocked — `blockedInArchive()` fires before the
    // selection-empty check (see `askDelete`), so no file selection is needed to trigger the notice.
    await fireEvent.keyDown(window, { key: "Delete" });

    const expected = translate("en", "smart.blockedNotice");
    await waitFor(() => expect(screen.getByText(expected)).toBeTruthy());

    // The old bug: this exact notice called a smart folder "a saved search view". Assert that
    // conflation is gone, not just that SOME translated string rendered.
    expect(expected).not.toContain("saved search");
    expect(expected).toContain("smart folder");
  });
});
