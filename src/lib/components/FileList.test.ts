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
import type { AvailableColumn, MetadataCell } from "../bindings.gen";

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
  is_symlink: false,
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

  it("flags a folder whose subtree the agent is changing, but not an unrelated folder (CPE-402)", () => {
    const entries = [
      entry({ name: "src", path: "/x/src", is_dir: true, extension: "" }),
      entry({ name: "docs", path: "/x/docs", is_dir: true, extension: "" }),
    ];
    const { container } = render(FileList, {
      ...base,
      entries,
      activity: { "/x/src/lib/main.rs": { kind: "modified" as const, at: Date.now() } },
    });
    const inside = container.querySelectorAll(".row.agent-inside");
    expect(inside.length).toBe(1); // only src
    expect(screen.getByTitle(/agent is changing files in here/i)).toBeTruthy();
  });
});

describe("Owner-coloured heat-map + legend (CPE-1116)", () => {
  it("tints an active row's accent bar with its actor's colour, not a per-kind literal", () => {
    const entries = [entry({ name: "changed.rs", path: "/x/changed.rs", extension: "rs" })];
    const { container } = render(FileList, {
      ...base,
      entries,
      activity: { "/x/changed.rs": { kind: "modified" as const, at: Date.now(), actor: "sess-a" } },
    });
    const row = container.querySelector(".row.agent-active") as HTMLElement;
    expect(row).toBeTruthy();
    // No session list resolves "sess-a" here, so it falls back to a deterministic hashed slot —
    // the point under test is that SOME --agent-N var drives the accent, not a hard-coded hex.
    expect(row.style.getPropertyValue("--agent-accent")).toMatch(/^var\(--agent-[1-6]\)$/);
  });

  it("tints an inside folder row with its subtree's owning actor (most-touches wins)", () => {
    const entries = [entry({ name: "src", path: "/x/src", is_dir: true, extension: "" })];
    const now = Date.now();
    const { container } = render(FileList, {
      ...base,
      entries,
      activity: {
        "/x/src/a.rs": { kind: "modified" as const, at: now, actor: "sess-a" },
        "/x/src/b.rs": { kind: "modified" as const, at: now, actor: "sess-a" },
        "/x/src/c.rs": { kind: "modified" as const, at: now, actor: "sess-b" },
      },
    });
    const row = container.querySelector(".row.agent-inside") as HTMLElement;
    expect(row).toBeTruthy();
    expect(row.style.getPropertyValue("--agent-accent")).toMatch(/^var\(--agent-[1-6]\)$/);
  });

  it("colours the user's own edits with the dedicated --agent-user var", () => {
    const entries = [entry({ name: "mine.txt", path: "/x/mine.txt" })];
    const { container } = render(FileList, {
      ...base,
      entries,
      activity: { "/x/mine.txt": { kind: "created" as const, at: Date.now(), actor: "user" } },
    });
    const row = container.querySelector(".row.agent-active") as HTMLElement;
    expect(row.style.getPropertyValue("--agent-accent")).toBe("var(--agent-user)");
  });

  it("shows a legend pill per distinct actor only while the activity map is non-empty", async () => {
    const entries = [entry({ name: "a.rs", path: "/x/a.rs" }), entry({ name: "b.rs", path: "/x/b.rs" })];
    const { container, rerender } = render(FileList, {
      ...base,
      entries,
      activity: {
        "/x/a.rs": { kind: "created" as const, at: Date.now(), actor: "user" },
        "/x/b.rs": { kind: "modified" as const, at: Date.now(), actor: "sess-a" },
      },
    });
    const legend = container.querySelector(".agent-legend");
    expect(legend).toBeTruthy();
    expect(legend?.querySelectorAll(".agent-legend-pill").length).toBe(2);
    expect(screen.getByText("You")).toBeTruthy(); // friendlyActor("user", ...)

    // Off means off: an empty activity map hides the legend entirely.
    await rerender({ ...base, entries, activity: {} });
    expect(container.querySelector(".agent-legend")).toBeNull();
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

  it("tints + labels a row matching a colour rule, leaving non-matching rows plain (CPE-775)", () => {
    const entries = [
      entry({ name: "photo.png", path: "/x/photo.png", extension: "png" }),
      entry({ name: "notes.txt", path: "/x/notes.txt", extension: "txt" }),
    ];
    const { container } = render(FileList, {
      ...base,
      entries,
      colorRules: [{ id: "r1", when: { kind: "ext", exts: ["png"] }, color: "#e03b3b", label: "Image" }],
    });
    const tinted = container.querySelectorAll(".row.rule-tinted");
    expect(tinted.length).toBe(1); // only the .png row
    expect((tinted[0] as HTMLElement).style.getPropertyValue("--rule-color")).toBe("#e03b3b");
    expect(screen.getByText("Image")).toBeTruthy(); // the rule label chip
  });

  it("leaves every row plain when the rule set is empty — off means off (CPE-775)", () => {
    const { container } = render(FileList, {
      ...base,
      entries: [entry({ name: "photo.png", path: "/x/photo.png", extension: "png" })],
      colorRules: [],
    });
    expect(container.querySelectorAll(".row.rule-tinted").length).toBe(0);
  });

  it("gives image tiles a thumbnail in icons view, but not other files (CPE-257/CPE-643)", () => {
    const { container } = render(FileList, {
      ...base,
      view: "icons",
      entries: [
        entry({ name: "pic.png", path: "/x/pic.png", extension: "png" }),
        entry({ name: "notes.txt", path: "/x/notes.txt", extension: "txt" }),
        entry({ name: "docs", path: "/x/docs", is_dir: true, extension: "" }),
      ],
    });
    // Exactly one ThumbnailImage — the image. The .txt and the folder keep plain icons.
    expect(container.querySelectorAll(".thumb")).toHaveLength(1);
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

  it("does not use thumbnails in details view (CPE-257/CPE-643)", () => {
    const { container } = render(FileList, {
      ...base,
      view: "details",
      entries: [entry({ name: "pic.png", path: "/x/pic.png", extension: "png" })],
    });
    expect(container.querySelector(".thumb")).toBeNull();
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

// CPE-1146 (epic CPE-707): dynamic metadata columns — a header + real value per active column,
// requesting cells for visible rows lazily, and rendering the dim empty-cell placeholder.
describe("FileList dynamic metadata columns (CPE-1146)", () => {
  const dims: AvailableColumn = { id: "dimensions", label: "Dimensions", column: "ImageDimensions", extensions: ["png"] };

  it("renders a header for each active metadata column, after the 4 built-ins", () => {
    render(FileList, {
      ...base,
      entries: [entry({ path: "/x/a.png", name: "a.png" })],
      activeMetaColumns: [{ col: dims, width: 110 }],
    });
    expect(screen.getByText("Dimensions")).toBeTruthy();
  });

  it("renders a cached cell's pre-formatted display string", () => {
    const cell: MetadataCell = { path: "/x/a.png", cell: { Dimensions: { w: 1920, h: 1080 } }, display: "1920×1080" };
    const metaCells = new Map([["dimensions", new Map([["/x/a.png", cell]])]]);
    render(FileList, {
      ...base,
      entries: [entry({ path: "/x/a.png", name: "a.png" })],
      activeMetaColumns: [{ col: dims, width: 110 }],
      metaCells,
    });
    expect(screen.getByText("1920×1080")).toBeTruthy();
  });

  it("renders a dim '—' for a cached-but-Empty cell, and nothing for a not-yet-fetched one", () => {
    const emptyCell: MetadataCell = { path: "/x/nope.png", cell: "Empty", display: "" };
    const metaCells = new Map([["dimensions", new Map([["/x/nope.png", emptyCell]])]]);
    const { container } = render(FileList, {
      ...base,
      entries: [
        entry({ path: "/x/nope.png", name: "nope.png" }),
        entry({ path: "/x/pending.png", name: "pending.png" }),
      ],
      activeMetaColumns: [{ col: dims, width: 110 }],
      metaCells,
    });
    expect(container.querySelector(".meta-empty")?.textContent).toBe("—");
    // The not-yet-fetched row's meta cell exists but is blank (never a crash, never a stray placeholder).
    const metaCellsEls = container.querySelectorAll(".cell.meta");
    expect(metaCellsEls.length).toBe(2);
    expect(metaCellsEls[1].textContent?.trim()).toBe("");
  });

  it("asks the caller for visible-row cells an active column doesn't have cached yet", async () => {
    // Mount with no active columns so nothing fires at mount time (a dispatch during the initial render
    // would land before a listener attached after `render()` returns); activate the column AFTERWARD so
    // the resulting reactive request is observed by a listener already wired up.
    const { component } = render(FileList, {
      ...base,
      entries: [entry({ path: "/x/a.png", name: "a.png" })],
    });
    const needMetaCells = vi.fn();
    component.$on("needMetaCells", (e: CustomEvent<{ columnId: string; paths: string[] }[]>) => needMetaCells(e.detail));

    await component.$set({ activeMetaColumns: [{ col: dims, width: 110 }] });

    expect(needMetaCells).toHaveBeenCalledWith([{ columnId: "dimensions", paths: ["/x/a.png"] }]);
  });

  it("clicking a metadata column header dispatches a meta:<id> sort key, toggling direction", async () => {
    const { component } = render(FileList, {
      ...base,
      entries: [entry({ path: "/x/a.png", name: "a.png" })],
      activeMetaColumns: [{ col: dims, width: 110 }],
    });
    const sorted = vi.fn();
    component.$on("sort", (e: CustomEvent<{ key: string; dir: string }>) => sorted(e.detail));

    await fireEvent.click(screen.getByText("Dimensions"));
    expect(sorted).toHaveBeenCalledWith({ key: "meta:dimensions", dir: "asc" });
  });

  it("clicking the header 'Columns…' affordance dispatches openColumnPicker", async () => {
    const { component } = render(FileList, { ...base, entries: [entry()] });
    const opened = vi.fn();
    component.$on("openColumnPicker", opened);

    await fireEvent.click(screen.getByTestId("open-column-picker"));
    expect(opened).toHaveBeenCalledOnce();
  });

  it("no active metadata columns renders exactly the pre-CPE-1146 4-column list (off means off)", () => {
    const { container } = render(FileList, { ...base, entries: [entry()] });
    expect(container.querySelectorAll(".cell.meta").length).toBe(0);
  });
});
