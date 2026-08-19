/**
 * CPE-1577 — user-defined commands' Toolbar and Context surfaces were persisted (the manager saved the
 * checkboxes) but nothing consumed them: only the Palette surface actually rendered/ran a command
 * (`App.svelte`'s command-palette list, wired via `commandsForSurface(userCommands, "palette")`).
 * A brand-new command also defaulted to Context-only, which — before this fix — meant it was invisible
 * everywhere (Toolbar/Context did nothing, Palette wasn't checked).
 *
 * This is the integration-layer proof, in the same mounted-App-with-mocked-backend harness as
 * App.hotkeyRemap.test.ts / App.paneBContextMenu.test.ts: a Context-bound command shows in the
 * right-click "Run command ▸" submenu and a Toolbar-bound command shows as its own CommandBar button —
 * both routing through the SAME `openRunCommand` → `RunCommandConfirm` confirm-before-launch flow the
 * Palette surface already used, and both resolving `{name}` against the live selection. Component-level
 * render/dispatch coverage for the two surfaces individually lives in ContextMenu.test.ts and
 * CommandBar.test.ts; this file proves the two are actually wired to App's real command list + selection.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import App from "./App.svelte";
import { resetSettings, saveUserCommands } from "./lib/settings";
import type { UserCommand } from "./lib/userCommands";
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
const entries: DirEntry[] = [file("alpha.txt")];
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

let runCommandCalls: { command: string; cwd: unknown }[] = [];

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  Element.prototype.scrollIntoView = vi.fn();
  runCommandCalls = [];
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const listingFor = (path: unknown) => (path === PATH ? entries : []);
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
      case "read_file_text": return "";
      case "run_command": {
        runCommandCalls.push({ command: args.command as string, cwd: args.cwd });
        return { stdout: "", stderr: "", code: 0, truncated: false };
      }
      default: return null;
    }
  });
});

const ctxCmd: UserCommand = {
  id: "uc_ctx", name: "Show it", template: "echo {name}", mode: "each", surfaces: ["context"],
};
const toolbarCmd: UserCommand = {
  id: "uc_tb", name: "Toolbar cmd", template: "echo tb {name}", mode: "each", surfaces: ["toolbar"],
};

/** Boot into the one populated drive, wait for the fixture file to paint. */
async function boot() {
  render(App);
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("alpha.txt")).toBeTruthy());
}

describe("App user-command Context surface (CPE-1577)", () => {
  it("a Context-bound command shows under the right-click 'Run command ▸' submenu and opens the resolved confirm dialog", async () => {
    saveUserCommands([ctxCmd]);
    await boot();

    await fireEvent.click(screen.getByText("alpha.txt"));
    const row = screen.getByText("alpha.txt").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(row);

    const menu = within(await screen.findByRole("menu"));
    await fireEvent.click(menu.getByText("Run command")); // Submenu parent row opens its flyout on click
    await fireEvent.click(menu.getByText("Show it"));

    // The context menu closes; RunCommandConfirm opens with the resolved command line for the selection.
    expect(await screen.findByText("Run “Show it”?")).toBeTruthy();
    expect(screen.getByText("echo alpha.txt")).toBeTruthy();
  });

  it("a command with no Context binding does NOT appear in the submenu (surfaces are per-command, not global)", async () => {
    saveUserCommands([toolbarCmd]); // toolbar-only
    await boot();

    const row = screen.getByText("alpha.txt").closest(".row") as HTMLElement;
    await fireEvent.contextMenu(row);
    const menu = within(await screen.findByRole("menu"));
    expect(menu.queryByText("Run command")).toBeNull();
  });
});

describe("App user-command Toolbar surface (CPE-1577)", () => {
  it("a Toolbar-bound command renders its own CommandBar button and opens the resolved confirm dialog", async () => {
    saveUserCommands([toolbarCmd]);
    await boot();
    await fireEvent.click(screen.getByText("alpha.txt")); // select, so {name} resolves to a real file

    await fireEvent.click(screen.getByTitle("Toolbar cmd (user command)"));

    expect(await screen.findByText("Run “Toolbar cmd”?")).toBeTruthy();
    expect(screen.getByText("echo tb alpha.txt")).toBeTruthy();
  });

  it("clicking Run in the confirm dialog actually invokes the backend run_command with the resolved line", async () => {
    saveUserCommands([toolbarCmd]);
    await boot();
    await fireEvent.click(screen.getByText("alpha.txt"));
    await fireEvent.click(screen.getByTitle("Toolbar cmd (user command)"));
    await screen.findByText("Run “Toolbar cmd”?");

    await fireEvent.click(screen.getByText("Run"));

    await waitFor(() => expect(runCommandCalls).toHaveLength(1));
    expect(runCommandCalls[0].command).toBe("echo tb alpha.txt");
  });

  it("no Toolbar button renders when no commands are bound to Toolbar (default state, zero cost)", async () => {
    saveUserCommands([ctxCmd]); // context-only
    await boot();
    expect(screen.queryByTitle(/user command/i)).toBeNull();
  });
});
