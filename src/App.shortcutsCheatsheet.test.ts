/**
 * CPE-1584 — a bare `?` never opened the keyboard-shortcuts cheat sheet. `handleKeydown`'s type-ahead
 * find block (`event.key.length === 1 && /\S/.test(event.key)`, ahead of the switch) greedily claimed
 * every single-character printable key — including "?" — before it could ever reach the switch's
 * `case "?"`, which was consequently pure dead code (the code even said so in a comment). The fix
 * special-cases a bare "?" ahead of the type-ahead block; this is the integration-layer proof, in the
 * same mounted-App-with-mocked-backend harness as App.hotkeyRemap.test.ts.
 *
 * Two things must both hold: "?" now opens the cheat sheet, AND ordinary type-ahead (any other printable
 * key) is provably still intact — the fix must not have widened the special-case past "?" itself.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import type { DirEntry, Place } from "./lib/types";

const PATH = "C:\\d";
const file = (name: string): DirEntry => ({
  name,
  path: `${PATH}\\${name}`,
  is_dir: false,
  size: 10,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension: "txt",
  hidden: false,
  is_symlink: false,
});
// Names chosen so a single-key type-ahead unambiguously selects one of them.
const entries: DirEntry[] = [file("alpha.txt"), file("bravo.txt")];
const drives: Place[] = [{ name: "Local Disk (C:)", path: PATH, kind: "drive" }];

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

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const listingFor = (path: unknown) => (path === PATH ? entries : []);
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return listingFor(args.path);
      case "list_dir_stream": {
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        const data = listingFor(args.path);
        ch.onmessage(data);
        return data.length;
      }
      case "parent_dir": return null;
      case "read_file_text": return "";
      default: return null;
    }
  });
});

async function boot() {
  render(App);
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("bravo.txt")).toBeTruthy());
}

describe("App '?' opens the shortcuts cheat sheet (CPE-1584 fix)", () => {
  it("a bare '?' with the file list focused opens the cheat sheet dialog", async () => {
    await boot();
    expect(screen.queryByRole("dialog", { name: "Keyboard shortcuts" })).toBeNull();

    await fireEvent.keyDown(window, { key: "?" });

    expect(await screen.findByRole("dialog", { name: "Keyboard shortcuts" })).toBeTruthy();
  });

  it("does NOT fire while typing '?' into a text field — the INPUT/TEXTAREA guard still wins", async () => {
    await boot();
    // The toolbar's own search input is a plain <input>; handleKeydown returns immediately for it.
    const search = document.querySelector('input[type="search"], input[placeholder*="Search" i]') as HTMLInputElement | null;
    // Fall back to any rendered <input> if the search box isn't identifiable by those hints — the point
    // is only that SOME text-entry control has focus when "?" is pressed.
    const input = search ?? (document.querySelector("input") as HTMLInputElement);
    expect(input).toBeTruthy();

    await fireEvent.keyDown(input, { key: "?" });

    expect(screen.queryByRole("dialog", { name: "Keyboard shortcuts" })).toBeNull();
  });
});

describe("App type-ahead find still works for ordinary printable keys (CPE-1584 regression guard)", () => {
  it("pressing 'b' (not '?') still jumps selection to the matching item, unaffected by the '?' special-case", async () => {
    await boot();

    await fireEvent.keyDown(window, { key: "b" });

    await waitFor(() => {
      expect(screen.getByText("bravo.txt").closest(".row")?.className).toContain("selected");
    });
    expect(screen.getByText("alpha.txt").closest(".row")?.className).not.toContain("selected");
    // Type-ahead consumed the key — it must not have also opened the cheat sheet.
    expect(screen.queryByRole("dialog", { name: "Keyboard shortcuts" })).toBeNull();
  });
});
