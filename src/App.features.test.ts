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

const { invoke, Channel } = vi.hoisted(() => ({
  invoke: vi.fn(),
  // Minimal stand-in for Tauri's IPC Channel (CPE-664): the mocked list_dir_stream calls onmessage.
  Channel: class {
    onmessage: (batch: unknown) => void = () => {};
  },
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke,
  convertFileSrc: (p: string) => `asset://${p}`,
  Channel,
}));
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
      case "list_dir_stream": {
        const ch = args?.onEntry as { onmessage: (b: unknown) => void };
        if (listing.length) ch.onmessage(listing);
        return listing.length;
      }
      case "parent_dir": return null;
      case "create_dir": return `${args?.path}\\${args?.name}`;
      case "rename_entry": return `${args?.path}.renamed`;
      case "copy_entries":
        return (args?.paths as string[]).map((p) => ({ path: `${p} (copy)`, ok: true, error: "" }));
      case "read_file_text": return "FILE PREVIEW CONTENT";
      case "find_files_by_name": return { matches: [], dirs_scanned: 1, truncated: false };
      case "find_files_by_name_stream": {
        const ch = args?.onMatch as { onmessage: (b: unknown) => void };
        ch.onmessage([{ path: "C:\\d\\alpha.md", name: "alpha.md", is_dir: false }]);
        return { matches: [], dirs_scanned: 1, truncated: false };
      }
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

describe("show hidden toggle (CPE-676 net)", () => {
  it("hides hidden entries by default and reveals them when toggled", async () => {
    mockBackend([file("visible.txt", "txt"), { ...file(".secret", "secret"), hidden: true }]);
    await enterDrive();
    await waitFor(() => expect(screen.getByText("visible.txt")).toBeTruthy());
    // Hidden by default (the `shown` pipeline filter).
    expect(screen.queryByText(".secret")).toBeNull();

    // The view controls live in the file-list toolbar's gear popover.
    await fireEvent.click(screen.getByLabelText("File list settings"));
    const checkbox = screen.getByText("Show hidden files").parentElement!.querySelector(
      'input[type="checkbox"]',
    ) as HTMLInputElement;
    await fireEvent.click(checkbox);

    await waitFor(() => expect(screen.getByText(".secret")).toBeTruthy());
    expect(screen.getByText("visible.txt")).toBeTruthy(); // still there
  });
});

describe("sort direction (CPE-676 net)", () => {
  it("reverses the listing order when direction is switched to descending", async () => {
    mockBackend([file("apple.txt", "txt"), file("cherry.txt", "txt"), file("banana.txt", "txt")]);
    await enterDrive();
    await waitFor(() => expect(screen.getByText("banana.txt")).toBeTruthy());

    // File names in DOM order (testing-library returns matches in document order).
    const names = () => screen.getAllByText(/\.txt$/).map((e) => e.textContent!.trim());
    await waitFor(() => expect(names()).toEqual(["apple.txt", "banana.txt", "cherry.txt"]));

    await fireEvent.click(screen.getByLabelText("File list settings"));
    const dir = screen.getByText("Direction").parentElement!.querySelector("select") as HTMLSelectElement;
    await fireEvent.change(dir, { target: { value: "desc" } });

    await waitFor(() => expect(names()).toEqual(["cherry.txt", "banana.txt", "apple.txt"]));
  });
});

describe("selection + status bar (CPE-676 net)", () => {
  it("reports the item and selected counts as rows are picked", async () => {
    mockBackend([file("apple.txt", "txt"), file("banana.txt", "txt"), file("cherry.txt", "txt")]);
    await enterDrive();
    await waitFor(() => expect(screen.getByText("apple.txt")).toBeTruthy());
    expect(screen.getByText(/^3 items$/)).toBeTruthy(); // itemCount from the pipeline

    await fireEvent.click(screen.getByText("apple.txt"));
    await waitFor(() => expect(screen.getByText(/^1 selected/)).toBeTruthy());

    await fireEvent.click(screen.getByText("banana.txt"), { ctrlKey: true });
    await waitFor(() => expect(screen.getByText(/^2 selected/)).toBeTruthy());
  });
});

describe("file-type filter (CPE-676 net)", () => {
  it("narrows the list to a category via the type filter", async () => {
    mockBackend([file("photo.png", "png"), file("notes.txt", "txt")]);
    await enterDrive();
    await waitFor(() => expect(screen.getByText("photo.png")).toBeTruthy());
    expect(screen.getByText("notes.txt")).toBeTruthy();

    await fireEvent.click(screen.getByTitle("Filter by type"));
    await fireEvent.click(screen.getByText("Images"));

    await waitFor(() => {
      expect(screen.getByText("photo.png")).toBeTruthy(); // an image passes
      expect(screen.queryByText("notes.txt")).toBeNull(); // a text file is filtered out
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

describe("resizable panels (CPE-069)", () => {
  it("drags the sidebar divider to resize and persists the width", async () => {
    mockBackend([]);
    const { container } = render(App);
    await screen.findAllByText("Local Disk (C:)");

    const resizer = screen.getByLabelText("Resize navigation pane");
    await fireEvent.mouseDown(resizer, { clientX: 220 });
    await fireEvent.mouseMove(window, { clientX: 300 }); // +80px
    await fireEvent.mouseUp(window);

    const main = container.querySelector(".main") as HTMLElement;
    expect(main.getAttribute("style")).toContain("grid-template-columns: 300px 6px 1fr");
    // Width is persisted to the single settings file via the backend (CPE-226),
    // debounced — so wait for the write_settings call carrying the new value.
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "write_settings",
        expect.objectContaining({ contents: expect.stringContaining('"cpe.sidebarWidth":300') }),
      ),
    );
  });

  it("clamps the sidebar to its safe minimum width", async () => {
    mockBackend([]);
    const { container } = render(App);
    await screen.findAllByText("Local Disk (C:)");

    const resizer = screen.getByLabelText("Resize navigation pane");
    await fireEvent.mouseDown(resizer, { clientX: 220 });
    await fireEvent.mouseMove(window, { clientX: 0 }); // -220px → clamps to 160
    await fireEvent.mouseUp(window);

    const main = container.querySelector(".main") as HTMLElement;
    expect(main.getAttribute("style")).toContain("grid-template-columns: 160px 6px 1fr");
  });
});

describe("preview pane (CPE-061)", () => {
  it("shows an image preview for a selected image file", async () => {
    mockBackend([file("photo.png", "png"), file("a.txt", "txt")]);
    const { container } = render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("photo.png")).toBeTruthy());

    await fireEvent.click(screen.getByText("photo.png"));

    await waitFor(() => {
      const img = container.querySelector(".preview img.preview-img") as HTMLImageElement | null;
      expect(img).toBeTruthy();
      expect(img!.getAttribute("src")).toBe("asset://C:\\d\\photo.png");
    });
  });

  it("shows a text preview (via read_file_text) for a selected text file", async () => {
    mockBackend([file("a.txt", "txt")]);
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("a.txt")).toBeTruthy());

    await fireEvent.click(screen.getByText("a.txt"));

    await waitFor(() => expect(screen.getByText("FILE PREVIEW CONTENT")).toBeTruthy());
    expect(invoke.mock.calls.some((c) => c[0] === "read_file_text")).toBe(true);
  });

  it("switches to metadata when the Details toggle is chosen", async () => {
    mockBackend([file("a.txt", "txt")]);
    const { container } = render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("a.txt")).toBeTruthy());
    await fireEvent.click(screen.getByText("a.txt"));
    await waitFor(() => expect(screen.getByText("FILE PREVIEW CONTENT")).toBeTruthy());

    await fireEvent.click(screen.getByRole("tab", { name: "Details" }));

    await waitFor(() => {
      expect(container.querySelector(".preview-pane .details")).toBeTruthy();
      expect(container.querySelector(".preview-text")).toBeNull();
    });
  });
});

describe("open in new tab (CPE-058)", () => {
  it("adds a background tab titled after the folder, keeping the current tab", async () => {
    mockBackend([file("alpha.md", "md"), dir("notes")]);
    const { container } = render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("alpha.md")).toBeTruthy());

    // Right-click the "notes" folder row to open its context menu.
    const notesRow = [...container.querySelectorAll(".row")].find((r) =>
      (r.textContent ?? "").includes("notes"),
    )!;
    await fireEvent.contextMenu(notesRow);

    await fireEvent.click(await screen.findByText("Open in new tab"));

    // A new tab labelled "notes" appears in the tab bar; the current tab (d) stays.
    await waitFor(() => {
      const labels = [...container.querySelectorAll(".tabbar .tab-label")].map(
        (l) => l.textContent,
      );
      expect(labels).toContain("notes");
      expect(labels).toContain("d");
    });
  });
});

describe("type-ahead find (CPE-057)", () => {
  it("selects the next item whose name starts with the typed letter", async () => {
    mockBackend([file("alpha.md", "md"), file("beta.png", "png"), dir("notes")]);
    await enterDrive();
    await waitFor(() => expect(screen.getByText("beta.png")).toBeTruthy());

    await fireEvent.keyDown(window, { key: "b" });

    // The selected list row (not the details pane, which also shows the name).
    await waitFor(() => {
      const betaRow = [...document.querySelectorAll(".row")].find((r) =>
        (r.textContent ?? "").includes("beta.png"),
      );
      expect(betaRow?.classList.contains("selected")).toBe(true);
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

describe("command palette (CPE-602/605)", () => {
  it("opens on Ctrl+Shift+P and filters to a matching command", async () => {
    mockBackend([file("alpha.md", "md")]);
    await enterDrive();
    await waitFor(() => expect(screen.getByText("alpha.md")).toBeTruthy());

    await fireEvent.keyDown(window, { key: "P", ctrlKey: true, shiftKey: true });

    const input = await screen.findByPlaceholderText(/Type a command/);
    await fireEvent.input(input, { target: { value: "terminal" } });

    // "Open terminal here" (added in CPE-605) is the only command matching "terminal".
    await waitFor(() => expect(screen.getByText("Open terminal here")).toBeTruthy());
  });
});

describe("find files by name (CPE-603)", () => {
  it("opens on Ctrl+P and searches the current folder recursively", async () => {
    mockBackend([file("alpha.md", "md")]);
    await enterDrive();
    await waitFor(() => expect(screen.getByText("alpha.md")).toBeTruthy());

    await fireEvent.keyDown(window, { key: "p", ctrlKey: true });

    const box = await screen.findByPlaceholderText(/Name to find/);
    await fireEvent.input(box, { target: { value: "*.md" } });
    await fireEvent.click(screen.getByText("Search"));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "find_files_by_name_stream",
        expect.objectContaining({ root: "C:\\d", query: "*.md" }),
      ),
    );
    // The streamed hit renders in the dialog's results.
    await waitFor(() => expect(screen.getAllByText("alpha.md").length).toBeGreaterThan(1));
  });
});
