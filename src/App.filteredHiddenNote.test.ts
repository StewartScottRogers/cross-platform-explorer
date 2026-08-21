/**
 * CPE-1708: the `filtered` count on a remote listing — how many provider-supplied entries were left out
 * because their name could not be shown safely (CPE-1704 built the count; it used to stop at an
 * `eprintln!` and never reach the UI) — now reaches the frontend as a typed field on `list_dir_stream`'s
 * `StreamDirResult` (the pane's actual first-paint path, STREAMING.md) and is surfaced as a status-bar
 * note, never a synthetic row in the listing (PR #890 round 2's rejected approach).
 *
 * This is the "frontend" end of the whole-path chain the ticket's acceptance criteria calls for
 * (provider → RemoteListing → command → frontend); the Rust-side links are proven independently in
 * `src-tauri/src/lib.rs`'s `mod tests` (`filtered_count_survives_remote_dir_entries_into_*`,
 * `list_dir_result_and_stream_dir_result_serialize_the_filtered_field_by_name`).
 *
 * Two cases, mirroring `App.test.ts`'s drive-click navigation pattern:
 *   1. filtered: 0 (the overwhelming majority of listings, local or remote) → the status bar shows
 *      NOTHING about it. An always-on note nobody reads is how a real warning gets tuned out.
 *   2. filtered: 2 → the status bar shows the exact, person-facing wording — not a maintainer-facing
 *      "filtered: 2", and nothing implying the listing itself failed (it succeeded).
 *
 * CPE-1780 adds this file's sibling case: `unreadable` (a LOCAL walk row that couldn't be stat'd — a
 * DIFFERENT fact from `filtered`'s "a remote name couldn't be shown safely") gets the exact same
 * App-level treatment — its own status-bar note, gated to 0 on Home the same way, worded distinctly.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import type { DirEntry, Place } from "./lib/types";

const entries: DirEntry[] = [
  {
    name: "ok.txt",
    path: "C:\\d\\ok.txt",
    is_dir: false,
    size: 7,
    modified: new Date(2026, 6, 10, 15, 0).getTime(),
    extension: "txt",
    hidden: false,
    is_symlink: false,
  },
];

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

/** `filtered`/`unreadable` streamed back by the mocked `list_dir_stream` for THIS test run — set
 *  per-test before navigating, so every case shares one fixture instead of duplicating the whole mock. */
let filtered = 0;
let unreadable = 0;

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  filtered = 0;
  unreadable = 0;

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries, filtered, unreadable };
      case "list_dir_stream": {
        const ch = args?.onEntry as { onmessage: (b: unknown) => void };
        ch.onmessage(entries);
        return { total: entries.length, filtered, unreadable };
      }
      case "parent_dir": return null;
      default: return null;
    }
  });
});

async function navigateIntoDrive() {
  render(App);
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("ok.txt")).toBeTruthy());
}

describe("filtered-hidden status-bar note (CPE-1708)", () => {
  it("shows nothing when the listing filtered nothing (the common case)", async () => {
    filtered = 0;
    await navigateIntoDrive();
    expect(screen.queryByText(/hidden because/)).toBeNull();
  });

  it("shows the honest, person-facing note on the VERY FIRST paint of a filtered remote-style listing — not only after a later cache revalidation", async () => {
    filtered = 2;
    await navigateIntoDrive();
    await waitFor(() =>
      expect(
        screen.getByText("2 entries were hidden because their names could not be shown safely"),
      ).toBeTruthy(),
    );
    // Never implies the LISTING failed — it succeeded; "ok.txt" is right there on screen too.
    expect(screen.queryByText(/fail|error|could not load/i)).toBeNull();
  });

  // Foreman F1 (regression): loadPath short-circuits Home BEFORE loadListing ever runs
  // (src/App.svelte ~2162-2166 — `entries = []; loading = false; return;`), so neither `loadGen`
  // nor `filteredHidden` is touched on that path. Without gating filteredHidden at its single
  // point of consumption (the <StatusBar> prop in src/App.svelte), a note from the last REAL
  // folder view would keep asserting itself over Home — a view with no listing at all, which is
  // exactly the false-statement-in-the-status-bar bug this ticket exists to remove.
  it("clears the note on Ctrl+T (new tab lands on Home, which has no listing at all)", async () => {
    filtered = 2;
    await navigateIntoDrive();
    await waitFor(() =>
      expect(
        screen.getByText("2 entries were hidden because their names could not be shown safely"),
      ).toBeTruthy(),
    );

    await fireEvent.keyDown(window, { key: "t", ctrlKey: true });

    await waitFor(() => expect(screen.queryByText("ok.txt")).toBeNull());
    expect(screen.queryByText(/hidden because/)).toBeNull();
  });
});

describe("unreadable-entries status-bar note (CPE-1780)", () => {
  it("shows nothing when nothing was unreadable (the common case)", async () => {
    unreadable = 0;
    await navigateIntoDrive();
    expect(screen.queryByText(/could not be read/)).toBeNull();
  });

  it("shows the honest, person-facing note on the VERY FIRST paint of a listing with an unreadable row — not only after a later cache revalidation", async () => {
    unreadable = 3;
    await navigateIntoDrive();
    await waitFor(() => expect(screen.getByText("3 entries could not be read")).toBeTruthy());
    // Never implies the LISTING failed — it succeeded; "ok.txt" is right there on screen too.
    expect(screen.queryByText(/fail|error|could not load/i)).toBeNull();
  });

  // Mirrors the `filteredHidden` Ctrl+T regression test above (same App.svelte gate, same reasoning):
  // `loadPath` short-circuits Home before `loadListing` ever runs, so a stale `unreadableCount` from the
  // last real folder must never leak into Home's status bar either.
  it("clears the note on Ctrl+T (new tab lands on Home, which has no listing at all)", async () => {
    unreadable = 3;
    await navigateIntoDrive();
    await waitFor(() => expect(screen.getByText("3 entries could not be read")).toBeTruthy());

    await fireEvent.keyDown(window, { key: "t", ctrlKey: true });

    await waitFor(() => expect(screen.queryByText("ok.txt")).toBeNull());
    expect(screen.queryByText(/could not be read/)).toBeNull();
  });

  // CPE-1780 AC: the two counts are different facts and must never be conflated. Both non-zero at once
  // proves neither note's rendering swallows or relabels the other's count.
  it("filtered and unreadable both render, as two separate notes, when both are non-zero", async () => {
    filtered = 2;
    unreadable = 3;
    await navigateIntoDrive();
    await waitFor(() =>
      expect(
        screen.getByText("2 entries were hidden because their names could not be shown safely"),
      ).toBeTruthy(),
    );
    expect(screen.getByText("3 entries could not be read")).toBeTruthy();
  });
});
