/**
 * CPE-1557 — `handleKeydown` wired to the persisted keymap (epic CPE-1484 "hotkey customization").
 *
 * Two layers, mirroring the inert-first / opt-in-safe proof pattern the keyboard-nav epic used
 * (App.navMode.test.ts):
 *
 *  1. RESOLUTION layer (pure): the migration routes every remappable built-in through
 *     `actionForChord(effectiveKeymap, chordFromEvent(event))`. These tests prove that at the DEFAULT
 *     keymap every action's default chord resolves back to *its own* id (so the dispatch fires exactly
 *     the action it always did — zero behavior change), that no two default chords collide, that a
 *     remap moves resolution to the new chord AND frees the old one, and that a cleared ("") chord
 *     resolves to nothing.
 *
 *  2. INTEGRATION layer: render the REAL App with a mocked backend and drive keys through the live
 *     `<svelte:window on:keydown>`. The load-bearing assertions are (a) a default chord (Ctrl+T = new
 *     tab) fires EXACTLY ONCE — no double-fire between the new keymap path and a leftover literal
 *     branch; (b) a remap saved via `saveKeymap` takes effect LIVE (no restart) — the new chord works
 *     and the old one is dead; (c) a cleared chord does nothing.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings, saveKeymap } from "./lib/settings";
import {
  ACTIONS,
  defaultKeymap,
  setChord,
  actionForChord,
  chordFromEvent,
  findConflicts,
  type ActionId,
} from "./lib/keymap";
import type { DirEntry, Place } from "./lib/types";

/** Reverse a canonical chord ("Ctrl+Shift+F", "Alt+ArrowLeft", "F5", "?") into the `KeyboardEvent`-
 *  shaped object `chordFromEvent` reads — i.e. what the DOM would actually report for that keypress. */
function eventFromChord(chord: string): {
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  key: string;
} {
  const parts = chord.split("+");
  const key = parts[parts.length - 1];
  return {
    ctrlKey: parts.includes("Ctrl"),
    metaKey: false,
    altKey: parts.includes("Alt"),
    shiftKey: parts.includes("Shift"),
    key,
  };
}

// ---------------------------------------------------------------------------
// 1. RESOLUTION layer — pure, no rendering.
// ---------------------------------------------------------------------------
describe("CPE-1557 keymap resolution — zero behavior change at defaults", () => {
  it("every action's default chord resolves back to that same action (round-trip)", () => {
    const km = defaultKeymap();
    for (const action of ACTIONS) {
      const chord = chordFromEvent(eventFromChord(action.defaultChord));
      expect(actionForChord(km, chord)).toBe(action.id);
    }
  });

  it("no two default chords collide — resolution is deterministic (never double-fires)", () => {
    expect(findConflicts(defaultKeymap())).toEqual([]);
  });

  it("a remap moves resolution to the new chord AND frees the old default chord", () => {
    const base = defaultKeymap();
    // Refresh defaults to F5; move it to a fresh Ctrl/Alt-qualified chord.
    const remapped = setChord(base, "refresh", "Ctrl+Shift+R");

    // The new chord now fires refresh...
    expect(actionForChord(remapped, chordFromEvent(eventFromChord("Ctrl+Shift+R")))).toBe("refresh");
    // ...and the old F5 no longer maps to anything (no other action claims it).
    expect(actionForChord(remapped, chordFromEvent(eventFromChord("F5")))).toBeUndefined();
  });

  it("a cleared ('') chord resolves to nothing — that action simply stops firing", () => {
    const cleared: Record<ActionId, string> = { ...defaultKeymap(), refresh: "" };
    expect(actionForChord(cleared, chordFromEvent(eventFromChord("F5")))).toBeUndefined();
    // An empty chord itself never matches, even against a cleared action.
    expect(actionForChord(cleared, "")).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// 2. INTEGRATION layer — real App, mocked backend (harness shape from App.navMode.test.ts).
// ---------------------------------------------------------------------------
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

const tabCount = () => document.querySelectorAll("button.tab").length;

async function boot() {
  const rendered = render(App);
  // Wait until the tab strip has painted its single initial tab.
  await waitFor(() => expect(tabCount()).toBe(1));
  return rendered;
}

describe("CPE-1557 handleKeydown — remaps take effect (integration)", () => {
  it("DEFAULT: Ctrl+T opens exactly ONE new tab (no double-fire)", async () => {
    await boot();
    expect(tabCount()).toBe(1);

    await fireEvent.keyDown(window, { key: "t", ctrlKey: true });

    // Exactly one new tab — proves the keymap path fired once and no leftover literal branch fired too.
    await waitFor(() => expect(tabCount()).toBe(2));
  });

  it("REMAP is live (no restart): the new chord opens a tab, the old default is dead", async () => {
    await boot();
    expect(tabCount()).toBe(1);

    // Remap "newTab" from Ctrl+T to Ctrl+Shift+Y and persist — saveKeymap publishes to the store the
    // running App subscribes to, so the change applies immediately.
    saveKeymap(setChord(defaultKeymap(), "newTab", "Ctrl+Shift+Y"));

    // Old default no longer opens a tab.
    await fireEvent.keyDown(window, { key: "t", ctrlKey: true });
    expect(tabCount()).toBe(1);

    // The remapped chord does.
    await fireEvent.keyDown(window, { key: "y", ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(tabCount()).toBe(2));
  });

  it("CLEARED: an unbound ('') chord does nothing", async () => {
    await boot();
    expect(tabCount()).toBe(1);

    saveKeymap({ ...defaultKeymap(), newTab: "" });

    await fireEvent.keyDown(window, { key: "t", ctrlKey: true });
    // No new tab — the cleared action simply doesn't fire.
    expect(tabCount()).toBe(1);
  });
});
