/**
 * Component render tests for FileList.
 *
 * These exist because v0.5.0 shipped with the file list rendering ZERO rows
 * while the status bar happily reported "18 items". Every pure module was unit
 * tested; nothing actually rendered a component. A single test like the one
 * below would have caught it before release.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import FileList from "./FileList.svelte";
import { emptySelection, selectOnly } from "../selection";
import type { DirEntry } from "../types";

// The component tree imports Tauri APIs transitively; stub them so jsdom can
// render without a Tauri runtime.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const entry = (over: Partial<DirEntry> = {}): DirEntry => ({
  name: "readme.md",
  path: "/x/readme.md",
  is_dir: false,
  size: 1024,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension: "md",
  hidden: false,
  ...over,
});

const base = {
  selection: emptySelection(),
  sortKey: "name" as const,
  sortDir: "asc" as const,
  view: "details" as const,
  error: "",
  loading: false,
  searching: false,
  cutPaths: [],
  renamingPath: "",
  renameValue: "",
  rowEls: [],
  draggedPaths: [],
};

describe("FileList Agent Watch annotations (CPE-399)", () => {
  it("badges + accents a row the agent just touched, and leaves untouched rows plain", () => {
    const entries = [
      entry({ name: "changed.rs", path: "/x/changed.rs", extension: "rs" }),
      entry({ name: "quiet.md", path: "/x/quiet.md" }),
    ];
    const { container } = render(FileList, {
      ...base,
      entries,
      activity: { "/x/changed.rs": { kind: "modified" as const, at: Date.now() } },
    });

    expect(screen.getByText("edited")).toBeTruthy(); // the kind badge
    const active = container.querySelectorAll(".row.agent-active");
    expect(active.length).toBe(1);
    expect(active[0].getAttribute("data-agent-kind")).toBe("modified");
  });

  it("shows no annotations when the activity map is empty (off means off)", () => {
    const { container } = render(FileList, { ...base, entries: [entry()], activity: {} });
    expect(container.querySelector(".agent-active")).toBeNull();
    expect(container.querySelector(".agent-badge")).toBeNull();
  });
});

describe("FileList rendering", () => {
  it("renders a row for every entry", () => {
    const entries = [
      entry({ name: "alpha.md", path: "/x/alpha.md" }),
      entry({ name: "beta.png", path: "/x/beta.png", extension: "png" }),
      entry({ name: "docs", path: "/x/docs", is_dir: true, extension: "" }),
    ];

    render(FileList, { ...base, entries });

    // The regression: rows silently disappeared while the count stayed right.
    expect(screen.getByText("alpha.md")).toBeTruthy();
    expect(screen.getByText("beta.png")).toBeTruthy();
    expect(screen.getByText("docs")).toBeTruthy();
  });

  it("renders the executable icon and 'Application' type for a .exe (CPE-047)", () => {
    const { container } = render(FileList, {
      ...base,
      entries: [entry({ name: "setup.exe", path: "/x/setup.exe", extension: "exe" })],
    });

    expect(screen.getByText("setup.exe")).toBeTruthy();
    expect(screen.getByText("Application")).toBeTruthy(); // Type column

    // The executable glyph uses a stroke colour no other category uses.
    const glyph = container.querySelector('svg [stroke="#5b3fd6"]');
    expect(glyph).toBeTruthy();
  });

  it("renders the details columns and the cell values", () => {
    render(FileList, { ...base, entries: [entry()] });

    expect(screen.getByText("Name")).toBeTruthy();
    expect(screen.getByText("Date modified")).toBeTruthy();
    expect(screen.getByText("Type")).toBeTruthy();
    expect(screen.getByText("Size")).toBeTruthy();

    expect(screen.getByText("Markdown file")).toBeTruthy();
    expect(screen.getByText("1.0 KB")).toBeTruthy();
  });

  it("renders rows with a live selection (the real-world case)", () => {
    const entries = [
      entry({ name: "one.txt", path: "/x/one.txt" }),
      entry({ name: "two.txt", path: "/x/two.txt" }),
    ];
    render(FileList, { ...base, entries, selection: selectOnly(1) });

    expect(screen.getByText("one.txt")).toBeTruthy();
    expect(screen.getByText("two.txt")).toBeTruthy();
  });

  it("shows the empty state only when there really are no entries", () => {
    render(FileList, { ...base, entries: [] });
    expect(screen.getByText("This folder is empty")).toBeTruthy();
  });

  it("shows a distinct message when a search matches nothing", () => {
    render(FileList, { ...base, entries: [], searching: true });
    expect(screen.getByText("No items match your search")).toBeTruthy();
  });

  it("shows the error state instead of rows", () => {
    render(FileList, { ...base, entries: [], error: "Permission denied." });
    expect(screen.getByText("Permission denied.")).toBeTruthy();
  });

  it("still renders rows in icons view", () => {
    render(FileList, {
      ...base,
      view: "icons",
      entries: [entry({ name: "pic.png", path: "/x/pic.png", extension: "png" })],
    });
    expect(screen.getByText("pic.png")).toBeTruthy();
  });

  it("gives image tiles a thumbnail slot in icons view, but not other files (CPE-257)", () => {
    const { container } = render(FileList, {
      ...base,
      view: "icons",
      entries: [
        entry({ name: "pic.png", path: "/x/pic.png", extension: "png" }),
        entry({ name: "notes.txt", path: "/x/notes.txt", extension: "txt" }),
        entry({ name: "docs", path: "/x/docs", is_dir: true, extension: "" }),
      ],
    });
    // Exactly one thumbnail slot — the image. The .txt and the folder keep icons.
    expect(container.querySelectorAll(".thumb-slot")).toHaveLength(1);
    expect(screen.getByText("notes.txt")).toBeTruthy();
    expect(screen.getByText("docs")).toBeTruthy();
  });

  it("column dividers are labelled separators, resizable by keyboard (CPE-314 a11y)", async () => {
    const { container, component } = render(FileList, {
      ...base,
      view: "details",
      entries: [entry({ name: "a.md", path: "/x/a.md" })],
    });
    const handle = container.querySelector(".col-resize") as HTMLElement;
    expect(handle).toBeTruthy();
    expect(handle.getAttribute("role")).toBe("separator");
    expect(handle.getAttribute("aria-label")).toMatch(/Resize/);
    expect(handle.getAttribute("tabindex")).toBe("0");

    const resized = vi.fn();
    component.$on("resizeColumns", (e) => resized(e.detail));
    await fireEvent.keyDown(handle, { key: "ArrowRight" });
    expect(resized).toHaveBeenCalled();
    expect(resized.mock.calls[0][0][0]).toBeGreaterThan(320); // Name column widened from its default
  });

  it("does not use thumbnail slots in details view (CPE-257)", () => {
    const { container } = render(FileList, {
      ...base,
      view: "details",
      entries: [entry({ name: "pic.png", path: "/x/pic.png", extension: "png" })],
    });
    expect(container.querySelector(".thumb-slot")).toBeNull();
  });

  /**
   * CPE-045. `class="row {view}"` gave every row the bare class `details`, which
   * collided with the global `.details` DetailsPane rule and clipped every row
   * to nothing — a blank list under a correct item count.
   *
   * jsdom applies no CSS, so no rendering assertion can catch this. What CAN be
   * asserted is the cause: a row must never carry a bare class that a global
   * layout rule owns. View classes must be namespaced.
   */
  it("never gives rows a bare class that collides with a global layout rule", () => {
    // These are global class names owned by other components / layout regions.
    const RESERVED = ["details", "filelist-pane", "main", "navigation-pane", "rows", "columns"];

    for (const view of ["details", "list", "icons"] as const) {
      const { container, unmount } = render(FileList, {
        ...base,
        view,
        entries: [entry()],
      });

      const row = container.querySelector(".row");
      expect(row, `no row rendered in ${view} view`).toBeTruthy();

      for (const reserved of RESERVED) {
        expect(
          row!.classList.contains(reserved),
          `row must not carry the reserved global class "${reserved}" (view=${view})`,
        ).toBe(false);
      }

      // The view must still be expressed — namespaced.
      expect(row!.classList.contains(`view-${view}`)).toBe(true);
      unmount();
    }
  });
});
