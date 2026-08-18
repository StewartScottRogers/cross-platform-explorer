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

/** `filtered` streamed back by the mocked `list_dir_stream` for THIS test run — set per-test before
 *  navigating, so both cases share one fixture instead of duplicating the whole mock. */
let filtered = 0;

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  filtered = 0;

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return { entries, filtered };
      case "list_dir_stream": {
        const ch = args?.onEntry as { onmessage: (b: unknown) => void };
        ch.onmessage(entries);
        return { total: entries.length, filtered };
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
});
