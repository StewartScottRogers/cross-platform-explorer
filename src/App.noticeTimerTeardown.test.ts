/**
 * CPE-1643 regression (leak 2 of 2): `noticeTimer` — `showNotice`'s 5s auto-dismiss `setTimeout`
 * (`App.svelte` ~L2082) — was cleared only when a NEW notice replaced it, never in `onDestroy`. A
 * pending notice timer therefore outlived the component. Found by the CPE-1633 worker sweeping
 * `App.svelte` for the same leak shape and asked to report rather than expand its diff.
 *
 * Reuses the exact fixture and notice trigger `App.smartFolderBlockedNotice.test.ts` (CPE-1614)
 * establishes — opening a smart folder, then pressing Delete with no selection, which is blocked by
 * `blockedInArchive()` and calls `showNotice(...)` unconditionally (no file selection required) — since
 * that's already a proven-reliable way to make `App.svelte` call `showNotice` from a plain integration
 * test without touching any backend command beyond the bare minimum drive/listing stubs.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/svelte";
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
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  smartFolders.set([]);
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

describe("onDestroy clears a pending notice timer (CPE-1643)", () => {
  it("clears showNotice's 5s auto-dismiss timer on destroy instead of leaving it to fire later", async () => {
    // Spy on the REAL global timer functions BEFORE mounting (same reasoning as
    // `App.smartFolderLiveRefresh.test.ts`'s CPE-1633 test): matches that file's established pattern.
    const setTimeoutSpy = vi.spyOn(window, "setTimeout");
    const clearTimeoutSpy = vi.spyOn(window, "clearTimeout");

    saveSmartFolder("Invoices", "invoice");
    render(App);
    await screen.findAllByText("Local Disk (C:)");

    const row = await screen.findByText("Invoices");
    await fireEvent.click(row);
    await waitFor(() => expect(screen.getAllByText("Invoices").length).toBeGreaterThan(1));

    setTimeoutSpy.mockClear();

    // Delete with no selection inside a smart folder is blocked and calls `showNotice(...)`
    // (`App.smartFolderBlockedNotice.test.ts` establishes this exact trigger) — which schedules the 5s
    // `noticeTimer` this ticket is about. Match the EXACT translated notice text (not a loose regex —
    // "smart folder" alone also appears in unrelated sidebar/tooltip copy already on screen).
    const expectedNotice = translate("en", "smart.blockedNotice");
    await fireEvent.keyDown(window, { key: "Delete" });
    await waitFor(() => expect(screen.getByText(expectedNotice)).toBeTruthy());

    const noticeCallIndex = setTimeoutSpy.mock.calls.findIndex(([, delay]) => delay === 5000);
    expect(noticeCallIndex).toBeGreaterThanOrEqual(0); // sanity: the notice timer really was armed
    const noticeHandle = setTimeoutSpy.mock.results[noticeCallIndex]!.value;

    // Destroy WITHOUT waiting the 5s out and without a later notice replacing this one.
    cleanup();

    // Verified as a real negative control by temporarily removing the `if (noticeTimer)
    // clearTimeout(noticeTimer);` line this ticket added to `onDestroy` and re-running this file: this
    // assertion failed (`clearTimeoutSpy` never called with `noticeHandle`) against the pre-fix code.
    expect(clearTimeoutSpy).toHaveBeenCalledWith(noticeHandle);

    setTimeoutSpy.mockRestore();
    clearTimeoutSpy.mockRestore();
  });
});
