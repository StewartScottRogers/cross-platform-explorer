/**
 * CPE-1556 — Navigation Mode (opt-in vim-modal layer, epic CPE-1487) wired into App.svelte's
 * `handleKeydown`. These tests render the REAL App with a mocked Tauri backend (same harness shape as
 * App.clipboardPaneRouting.test.ts) and drive keys through the live `<svelte:window on:keydown>`.
 *
 * The load-bearing assertion is the HARD CONSTRAINT: with the Settings toggle OFF (the default), a key
 * the mode WOULD consume (`j`) must NOT move the selection — the modal layer is inert and every existing
 * handler runs exactly as before. The remaining cases prove the layer works when the toggle is ON
 * (`j`/`k` move the selection, `v` toggles visual mode, `:` opens the command line) and that a focused
 * text input is never intercepted even with the mode enabled.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings, saveNavigationModeEnabled } from "./lib/settings";
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

// Names chosen so NONE start with j/k/v — so with the mode OFF a `j`/`k`/`v` keypress is a type-ahead
// no-op (nothing to jump to), isolating "did nav-mode move the selection?" as the only variable.
const entries: DirEntry[] = [file("alpha.txt"), file("bravo.txt"), file("charlie.txt")];
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

/** Render, navigate into the drive, wait for rows. Caller flips `saveNavigationModeEnabled` FIRST when
 *  it wants the mode on (App reads the setting once at init). */
async function boot() {
  const rendered = render(App);
  const drive = (await screen.findAllByText("Local Disk (C:)"))[0];
  await fireEvent.click(drive);
  await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());
  await waitFor(() => expect(screen.getByText("charlie.txt")).toBeTruthy());
  return rendered;
}

const rowFor = (name: string) => screen.getByText(name).closest(".row") as HTMLElement;
const isSelected = (name: string) => rowFor(name).classList.contains("selected");

describe("App — Navigation Mode integration (CPE-1556)", () => {
  it("OFF (default): a key nav-mode WOULD consume ('j') does NOT move the selection — zero behavior change", async () => {
    await boot(); // navigationModeEnabled defaults false

    // The mode's surfaces are not even mounted when off.
    expect(screen.queryByTestId("nav-mode-indicator")).toBeNull();

    await fireEvent.click(screen.getByText("alpha.txt"));
    expect(isSelected("alpha.txt")).toBe(true);

    // With the mode ON this would move the selection DOWN to bravo. With it off, `j` is just a
    // type-ahead key that matches nothing here, so the selection is unchanged.
    await fireEvent.keyDown(window, { key: "j" });

    expect(isSelected("alpha.txt")).toBe(true);
    expect(isSelected("bravo.txt")).toBe(false);
    expect(screen.queryByTestId("nav-command-line")).toBeNull();
  });

  it("ON: 'j' / 'k' move the selection down / up in the file list", async () => {
    saveNavigationModeEnabled(true);
    await boot();

    expect(screen.getByTestId("nav-mode-label").textContent).toBe("NORMAL");

    await fireEvent.click(screen.getByText("alpha.txt"));
    expect(isSelected("alpha.txt")).toBe(true);

    await fireEvent.keyDown(window, { key: "j" }); // down -> bravo
    await waitFor(() => expect(isSelected("bravo.txt")).toBe(true));
    expect(isSelected("alpha.txt")).toBe(false);

    await fireEvent.keyDown(window, { key: "k" }); // up -> alpha
    await waitFor(() => expect(isSelected("alpha.txt")).toBe(true));
    expect(isSelected("bravo.txt")).toBe(false);
  });

  it("ON: 'v' enters visual mode and Escape returns to normal", async () => {
    saveNavigationModeEnabled(true);
    await boot();

    expect(screen.getByTestId("nav-mode-label").textContent).toBe("NORMAL");

    await fireEvent.keyDown(window, { key: "v" });
    await waitFor(() => expect(screen.getByTestId("nav-mode-label").textContent).toBe("VISUAL"));

    await fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.getByTestId("nav-mode-label").textContent).toBe("NORMAL"));
  });

  it("ON: ':' opens the command line and its cancel closes it", async () => {
    saveNavigationModeEnabled(true);
    await boot();

    expect(screen.queryByTestId("nav-command-line")).toBeNull();

    await fireEvent.keyDown(window, { key: ":" });
    await waitFor(() => expect(screen.getByTestId("nav-command-line")).toBeTruthy());

    // Escape inside the command-line input dispatches `cancel`, which closes it.
    await fireEvent.keyDown(screen.getByTestId("ncl-input"), { key: "Escape" });
    await waitFor(() => expect(screen.queryByTestId("nav-command-line")).toBeNull());
  });

  it("ON: typing in a text input is NOT intercepted (the input-focus guard wins even with the mode enabled)", async () => {
    saveNavigationModeEnabled(true);
    const { container } = await boot();

    await fireEvent.click(screen.getByText("alpha.txt"));
    expect(isSelected("alpha.txt")).toBe(true);

    // Fire `j` FROM a real text input (the toolbar search box). handleKeydown's INPUT/TEXTAREA guard
    // returns before the nav layer, so the selection must not move.
    const searchInput = container.querySelector<HTMLInputElement>(".search input");
    expect(searchInput).toBeTruthy();
    await fireEvent.keyDown(searchInput as HTMLInputElement, { key: "j" });

    expect(isSelected("alpha.txt")).toBe(true);
    expect(isSelected("bravo.txt")).toBe(false);
  });
});
