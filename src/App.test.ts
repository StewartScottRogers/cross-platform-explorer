/**
 * Integration test: render the real App with a mocked Tauri backend, navigate
 * into a folder, and assert the file list actually shows rows.
 *
 * v0.5.0 shipped with the list rendering ZERO rows while the status bar said
 * "18 items". FileList tested fine in isolation, so the fault was in the App
 * wiring — which nothing tested. This is that test.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import type { DirEntry, Place } from "./lib/types";

const entries: DirEntry[] = [
  {
    name: "alpha.md",
    path: "C:\\d\\alpha.md",
    is_dir: false,
    size: 2048,
    modified: new Date(2026, 6, 10, 15, 0).getTime(),
    extension: "md",
    hidden: false,
  },
  {
    name: "notes",
    path: "C:\\d\\notes",
    is_dir: true,
    size: 0,
    modified: new Date(2026, 6, 9, 9, 0).getTime(),
    extension: "",
    hidden: false,
  },
];

const drives: Place[] = [
  { name: "Local Disk (C:)", path: "C:\\d", kind: "drive" },
];

// vi.mock is hoisted above every declaration, so the mock fn must be created
// inside vi.hoisted or the factory closes over an uninitialised binding.
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));

beforeEach(() => {
  localStorage.clear();
  // jsdom has no layout engine.
  Element.prototype.scrollIntoView = vi.fn();

  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return entries;
      case "parent_dir": return null;
      default: return null;
    }
  });
});

describe("App — navigating into a folder", () => {
  it("DEBUG dump", async () => {
    const { container } = render(App);
    await new Promise((r) => setTimeout(r, 300));
    console.log("=== DOM ===\n" + container.innerHTML.slice(0, 3000));
    console.log("=== invoke calls ===", invoke.mock.calls.map((c) => c[0]));
  });

  it("renders a row for every entry the backend returns", async () => {
    render(App);

    // Home first: the drive shows in Quick access / sidebar.
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);

    // THE REGRESSION: these rows silently failed to render in v0.5.0.
    await waitFor(() => {
      expect(screen.getByText("alpha.md")).toBeTruthy();
      expect(screen.getByText("notes")).toBeTruthy();
    });
  });

  it("reports an item count that matches the rows actually shown", async () => {
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);

    await waitFor(() => expect(screen.getByText("alpha.md")).toBeTruthy());

    // The status bar said "18 items" while showing none. Count and rows must agree.
    expect(screen.getByText("2 items")).toBeTruthy();
  });
});
