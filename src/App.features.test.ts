/**
 * Integration tests that drive shipped features end-to-end through the real App
 * with a mocked Tauri backend. These verify the App WIRING (not just the pure
 * helpers), which is exactly the layer that visual/manual checks were covering.
 *
 * Covers: CPE-052 (wildcard search), CPE-050 (new-folder auto-numbering),
 * CPE-051 (file-name validation on rename).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import type { DirEntry, Place } from "./lib/types";

const dir = (name: string): DirEntry => ({
  name,
  path: `C:\\d\\${name}`,
  is_dir: true,
  size: 0,
  modified: 0,
  extension: "",
  hidden: false,
});

const file = (name: string, extension: string): DirEntry => ({
  name,
  path: `C:\\d\\${name}`,
  is_dir: false,
  size: 1024,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension,
  hidden: false,
});

const drives: Place[] = [{ name: "Local Disk (C:)", path: "C:\\d", kind: "drive" }];

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));

/** Install a backend whose list_dir returns `listing`. */
function mockBackend(listing: DirEntry[]) {
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return listing;
      case "parent_dir": return null;
      case "create_dir": return `${args?.path}\\${args?.name}`;
      case "rename_entry": return `${args?.path}.renamed`;
      case "copy_entries":
        return (args?.paths as string[]).map((p) => ({ path: `${p} (copy)`, ok: true, error: "" }));
      default: return null;
    }
  });
}

/** Render App and navigate into the C: drive so we're in a real folder. */
async function enterDrive() {
  render(App);
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
}

beforeEach(() => {
  localStorage.clear();
  Element.prototype.scrollIntoView = vi.fn();
  invoke.mockReset();
});

describe("wildcard search (CPE-052)", () => {
  it("filters the list to names matching a glob", async () => {
    mockBackend([file("alpha.md", "md"), file("beta.png", "png"), dir("notes")]);
    await enterDrive();
    await waitFor(() => expect(screen.getByText("alpha.md")).toBeTruthy());

    const box = screen.getByLabelText(/^Search/);
    await fireEvent.input(box, { target: { value: "*.md" } });

    await waitFor(() => {
      expect(screen.getByText("alpha.md")).toBeTruthy();
      expect(screen.queryByText("beta.png")).toBeNull();
      expect(screen.queryByText("notes")).toBeNull();
    });
  });
});

describe("new folder auto-numbering (CPE-050)", () => {
  it("asks the backend for 'New folder (2)' when 'New folder' already exists", async () => {
    mockBackend([file("alpha.md", "md"), dir("New folder")]);
    await enterDrive();
    await waitFor(() => expect(screen.getByText("alpha.md")).toBeTruthy());

    await fireEvent.click(screen.getByTitle(/New folder/));

    await waitFor(() => {
      const call = invoke.mock.calls.find((c) => c[0] === "create_dir");
      expect(call).toBeTruthy();
      expect((call![1] as { name: string }).name).toBe("New folder (2)");
    });
  });
});

describe("copy as path (CPE-056)", () => {
  it("Ctrl+Shift+C writes the quoted path to the clipboard", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });

    mockBackend([file("alpha.md", "md")]);
    await enterDrive();
    await waitFor(() => expect(screen.getByText("alpha.md")).toBeTruthy());

    await fireEvent.click(screen.getByText("alpha.md"));
    await fireEvent.keyDown(window, { key: "C", ctrlKey: true, shiftKey: true });

    await waitFor(() => expect(writeText).toHaveBeenCalledWith('"C:\\d\\alpha.md"'));
  });
});

describe("duplicate (CPE-055)", () => {
  it("Ctrl+D copies the selected item into the current folder", async () => {
    mockBackend([file("alpha.md", "md"), file("beta.png", "png")]);
    await enterDrive();
    await waitFor(() => expect(screen.getByText("alpha.md")).toBeTruthy());

    await fireEvent.click(screen.getByText("alpha.md"));
    await fireEvent.keyDown(window, { key: "d", ctrlKey: true });

    await waitFor(() => {
      const call = invoke.mock.calls.find((c) => c[0] === "copy_entries");
      expect(call).toBeTruthy();
      const args = call![1] as { paths: string[]; dest: string };
      expect(args.paths).toEqual(["C:\\d\\alpha.md"]);
      expect(args.dest).toBe("C:\\d");
    });
  });
});

describe("file-name validation on rename (CPE-051)", () => {
  it("blocks an illegal name with a notice and never calls the backend", async () => {
    mockBackend([file("alpha.md", "md")]);
    const { container } = render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha.md")).toBeTruthy());

    // Select the row, then F2 to begin the inline rename.
    await fireEvent.click(screen.getByText("alpha.md"));
    await fireEvent.keyDown(window, { key: "F2" });

    const input = (await waitFor(() => {
      const el = container.querySelector("input.rename");
      expect(el).toBeTruthy();
      return el as HTMLInputElement;
    }));

    await fireEvent.input(input, { target: { value: "bad/name.md" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(screen.getByText(/can't contain/i)).toBeTruthy());
    expect(invoke.mock.calls.some((c) => c[0] === "rename_entry")).toBe(false);
  });
});
