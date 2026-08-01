/**
 * CPE-1146 (epic CPE-707): ExplorerPane's metadata-column orchestration — the lazy visible-row fetch
 * FileList's `needMetaCells` triggers (streamed `metadata_column_cells`, merged into the shared cache),
 * and fetch-on-sort (the collect-to-vec `metadata_column_cells_collect`) driving a type-aware re-sort.
 * A separate file from ExplorerPane.test.ts: this one needs a real `Channel` stub in its
 * `@tauri-apps/api/core` mock (for the streamed fetch), which that file's simpler `{ invoke: vi.fn() }`
 * mock doesn't provide.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import ExplorerPane from "./ExplorerPane.svelte";
import { emptySelection } from "../selection";
import { __resetMetaColumnCatalogForTests } from "../metaColumnCatalog";
import type { DirEntry } from "../types";
import type { AvailableColumn, MetadataCell } from "../bindings.gen";

const DIMS: AvailableColumn = {
  id: "dimensions",
  label: "Dimensions",
  column: "ImageDimensions",
  extensions: ["png"],
};

const CELLS: Record<string, MetadataCell> = {
  "/x/big.png": { path: "/x/big.png", cell: { Dimensions: { w: 1920, h: 1080 } }, display: "1920×1080" },
  "/x/small.png": { path: "/x/small.png", cell: { Dimensions: { w: 100, h: 100 } }, display: "100×100" },
};

const invoke = vi.fn(async (cmd: string, args: any) => {
  if (cmd === "metadata_columns_available") return [DIMS];
  if (cmd === "metadata_column_cells") {
    const batch = (args.paths as string[]).map((p) => CELLS[p]).filter(Boolean);
    args.onCell?.onmessage?.(batch);
    return batch.length;
  }
  if (cmd === "metadata_column_cells_collect") {
    return (args.paths as string[]).map((p) => CELLS[p]).filter(Boolean);
  }
  return null;
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (...a: unknown[]) => invoke(...(a as [string, any])), Channel };
});

const entry = (over: Partial<DirEntry> = {}): DirEntry => ({
  name: "x",
  path: "/x",
  is_dir: false,
  size: 0,
  modified: 0,
  extension: "png",
  hidden: false,
  is_symlink: false,
  ...over,
});

beforeEach(() => {
  invoke.mockClear();
  __resetMetaColumnCatalogForTests();
});

describe("ExplorerPane lazy metadata-cell fetch (CPE-1146)", () => {
  it("streams the visible row's cell and renders its formatted display once it lands", async () => {
    render(ExplorerPane, {
      selection: emptySelection(),
      entries: [entry({ path: "/x/small.png", name: "small.png" })],
      activeMetaColumns: [{ id: "dimensions", width: 110 }],
    });

    // The catalog loads async (onMount → ensureMetaColumnCatalog), which is what resolves the id into a
    // header FileList can render + a column FileList can request cells for.
    await waitFor(() => expect(screen.getByText("Dimensions")).toBeTruthy());
    await waitFor(() => expect(screen.getByText("100×100")).toBeTruthy());
  });

  it("a failed cell fetch leaves the cell blank rather than blocking the row (never crashes)", async () => {
    invoke.mockImplementationOnce(async (cmd: string) => {
      if (cmd === "metadata_columns_available") return [DIMS];
      throw new Error("boom");
    });
    invoke.mockImplementationOnce(async () => {
      throw new Error("boom"); // the metadata_column_cells call itself
    });

    const { container } = render(ExplorerPane, {
      selection: emptySelection(),
      entries: [entry({ path: "/x/small.png", name: "small.png" })],
      activeMetaColumns: [{ id: "dimensions", width: 110 }],
    });

    await waitFor(() => expect(screen.getByText("Dimensions")).toBeTruthy());
    // The row still renders (name cell present); the meta cell is simply empty, no thrown error surfaced.
    expect(screen.getByText("small.png")).toBeTruthy();
    expect(container.querySelector(".cell.meta")?.textContent?.trim()).toBe("");
  });
});

describe("ExplorerPane fetch-on-sort by a metadata column (CPE-1146)", () => {
  it("clicking the metadata column header fetches every row and re-sorts numerically", async () => {
    const { container } = render(ExplorerPane, {
      selection: emptySelection(),
      entries: [
        entry({ path: "/x/big.png", name: "big.png" }),
        entry({ path: "/x/small.png", name: "small.png" }),
      ],
      activeMetaColumns: [{ id: "dimensions", width: 110 }],
    });

    await waitFor(() => expect(screen.getByText("Dimensions")).toBeTruthy());
    await fireEvent.click(screen.getByText("Dimensions"));

    // Ascending by pixel AREA (100×100 = 10,000 before 1920×1080 = 2,073,600) — a lexical sort on the
    // formatted "w×h" strings would get this wrong.
    await waitFor(() => {
      const names = Array.from(container.querySelectorAll(".cell.name .ellip")).map((el) => el.textContent);
      expect(names).toEqual(["small.png", "big.png"]);
    });
  });
});
