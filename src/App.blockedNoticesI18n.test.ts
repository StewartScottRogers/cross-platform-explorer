/**
 * CPE-1627: the three sibling branches of `blockedInArchive()` that CPE-1614 left untranslated — saved
 * search, archive, and Replay mode — are now routed through `$t()`. This proves all three render their
 * translated (non-English) text when the app is running in a locale other than English, mirroring the
 * pattern CPE-1614 used for the smart-folder branch (`App.smartFolderBlockedNotice.test.ts`): open the
 * real feature, trigger a blocked mutating action, and assert the notice matches the CATALOG's translated
 * string for that locale — not just that the key resolves to something.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { addSavedSearch, savedSearches } from "./lib/savedSearchStore";
import { locale, translate } from "./lib/i18n";
import { ingestSessionState, clearAgentSessions } from "./lib/agentSessions";
import { ingestActivity, clearActivity } from "./lib/agentActivity";
import type { DirEntry, Place } from "./lib/types";
import type { TreeNode } from "./lib/bindings.gen";

const entry = (name: string, path: string, isDir: boolean, extension = ""): DirEntry => ({
  name,
  path,
  is_dir: isDir,
  size: isDir ? 0 : 1024,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension,
  hidden: false,
  is_symlink: false,
});

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];

const scannedTree: TreeNode[] = [{ name: "keep.md", isDir: false, size: 10, modified: 1_700_000_000_000 }];

// Real-filesystem listings by path: C:\d holds "alpha.md" (for the Replay case) and subfolder "photos"
// (for the archive case); C:\d\photos holds the zip we browse.
const listings: Record<string, DirEntry[]> = {
  "C:\\d": [entry("photos", "C:\\d\\photos", true), entry("alpha.md", "C:\\d\\alpha.md", false, "md")],
  "C:\\d\\photos": [entry("bundle.zip", "C:\\d\\photos\\bundle.zip", false, "zip")],
};

const replayEv = (ts: number, kind: string, path: string) => ({ ts, session: "sess-1", kind, path, actor: null, detail: null });

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
// Opening a structured search / arming a watch both wrap the REAL `@tauri-apps/api/event.listen`, which
// needs the Tauri IPC bridge (`window.__TAURI_INTERNALS__`) that doesn't exist in jsdom. Mock it to a
// no-op listener, same fix as `App.savedSearch.test.ts` / `App.replayGuards.test.ts`.
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  savedSearches.set([]); // module singleton across this file's tests — start clean
  Element.prototype.scrollIntoView = vi.fn();
  // German — one of the 12 COMPLETE_LOCALES (src/lib/i18n.ts) — proves these notices actually translate,
  // not just that $t() resolves back to the English source string.
  locale.set("de");

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries: listings[args?.path as string] ?? [], filtered: 0 };
      case "list_dir_stream": {
        const ch = args?.onEntry as { onmessage: (b: unknown) => void };
        const list = listings[args?.path as string] ?? [];
        if (list.length) ch.onmessage(list);
        return list.length;
      }
      case "parent_dir": return null;
      case "scan_tree": return scannedTree;
      case "read_archive_entries": return [{ name: "inside.txt", size: 5, is_dir: false }];
      case "read_file_text": return "";
      case "agent_watch_start": return null;
      case "agent_watch_stop": return null;
      case "replay_load":
        return {
          replay: {
            events: [replayEv(100, "created", "C:\\d\\alpha.md"), replayEv(200, "created", "C:\\d\\alpha.md")],
            bounds: [100, 200],
            summary: { total: 2, byKind: {}, sessions: ["sess-1"], firstAt: 100, lastAt: 200 },
          },
          baseline: null,
        };
      default: return null;
    }
  });
});

afterEach(() => {
  locale.set("en"); // module singleton — don't bleed a non-English locale into other test files
  clearAgentSessions();
  clearActivity();
});

describe("blockedInArchive() siblings render translated notices in a non-English locale (CPE-1627)", () => {
  it("saved search: the read-only notice matches the German catalog string", async () => {
    addSavedSearch("Markdown docs", [{ kind: "ext", exts: ["md"] }], "all", "C:\\d");
    render(App);
    await screen.findAllByText("Local Disk (C:)");

    const row = await screen.findByText("Markdown docs");
    await fireEvent.click(row);
    await waitFor(() => expect(screen.getByText("keep.md")).toBeTruthy());

    // Delete requires no selection to be blocked — blockedInArchive() fires before the
    // selection-empty check (see askDelete), same as CPE-1614's test.
    await fireEvent.keyDown(window, { key: "Delete" });

    const expected = translate("de", "smart.searchBlockedNotice");
    await waitFor(() => expect(screen.getByText(expected)).toBeTruthy());
    // Proves this is really the German catalog string, not the English fallback rendering by luck.
    expect(expected).not.toBe(translate("en", "smart.searchBlockedNotice"));
  });

  it("archive: the read-only notice matches the German catalog string", async () => {
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("photos")).toBeTruthy());

    await fireEvent.dblClick(screen.getByText("photos"));
    await waitFor(() => expect(screen.getByText("bundle.zip")).toBeTruthy());

    // Enter the archive in place — its inner entry renders in the file list.
    await fireEvent.dblClick(screen.getByText("bundle.zip"));
    await waitFor(() => expect(screen.getByText("inside.txt")).toBeTruthy());

    await fireEvent.keyDown(window, { key: "Delete" });

    const expected = translate("de", "archive.blockedNotice");
    await waitFor(() => expect(screen.getByText(expected)).toBeTruthy());
    expect(expected).not.toBe(translate("en", "archive.blockedNotice"));
  });

  it("Replay mode: the read-only notice matches the German catalog string", async () => {
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha.md")).toBeTruthy());

    ingestSessionState(
      `session:${JSON.stringify({
        event: "started",
        sessionId: "sess-1",
        agentId: "claude-code",
        agentName: "Claude Code",
        provider: "anthropic",
        model: "claude",
        cwd: "C:\\d",
      })}`,
    );
    // At least two timeline entries so the Replay tab's scrubber (sliderRange) is enabled.
    ingestActivity([{ kind: "created", path: "C:\\d\\alpha.md" }], 1000);
    ingestActivity([{ kind: "created", path: "C:\\d\\alpha.md" }], 2000);

    // The button's own title is translated too (agent.showLog) — the app is running in German here.
    const showLogTitle = translate("de", "agent.showLog");
    await waitFor(() => expect(screen.getByTitle(showLogTitle)).toBeTruthy());
    await fireEvent.click(screen.getByTitle(showLogTitle));
    await fireEvent.click(await screen.findByRole("tab", { name: "Replay" }));
    await waitFor(() => expect(screen.getByText(/Reconstruction at scrub time \(read-only\)/i)).toBeTruthy());
    await fireEvent.click(screen.getByLabelText("Show in file pane"));

    // Any guarded mutator fires the notice while the overlay is showing — new-folder mirrors
    // App.replayGuards.test.ts's own coverage of this exact call site.
    await fireEvent.keyDown(window, { key: "N", ctrlKey: true, shiftKey: true });

    const expected = translate("de", "replay.blockedNotice");
    await waitFor(() => expect(screen.getByText(expected)).toBeTruthy());
    expect(expected).not.toBe(translate("en", "replay.blockedNotice"));
  });
});
