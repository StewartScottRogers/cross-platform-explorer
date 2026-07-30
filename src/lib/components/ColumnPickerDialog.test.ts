/**
 * CPE-1146 (epic CPE-707): the column picker — add/remove/reorder a folder's active metadata columns.
 * The dialog is pure props-in/events-out (no backend call of its own — the caller owns fetch + persist),
 * so these are straightforward component-render tests, no Tauri mock needed.
 */
import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ColumnPickerDialog from "./ColumnPickerDialog.svelte";
import type { AvailableColumn } from "../bindings.gen";
import type { ActiveMetaColumn } from "../columns";

const AVAILABLE: AvailableColumn[] = [
  { id: "dimensions", label: "Dimensions", column: "ImageDimensions", extensions: ["png", "jpg"] },
  { id: "duration", label: "Duration", column: "VideoDuration", extensions: ["mp4", "mov"] },
  { id: "pages", label: "Pages", column: "DocPages", extensions: ["pdf"] },
];

describe("ColumnPickerDialog add/remove/reorder (CPE-1146)", () => {
  it("lists every available column and shows 'no metadata columns' when none are active", () => {
    render(ColumnPickerDialog, { available: AVAILABLE, active: [] });
    expect(screen.getByTestId("active-empty")).toBeTruthy();
    expect(screen.getByTestId("available-list").textContent).toContain("Dimensions");
    expect(screen.getByTestId("available-list").textContent).toContain("Duration");
    expect(screen.getByTestId("available-list").textContent).toContain("Pages");
  });

  it("adding a column dispatches the new full active array, appended at the default width", async () => {
    const { component } = render(ColumnPickerDialog, { available: AVAILABLE, active: [] });
    const onChange = (e: CustomEvent<ActiveMetaColumn[]>) => changed.push(e.detail);
    const changed: ActiveMetaColumn[][] = [];
    component.$on("change", onChange);

    await fireEvent.click(screen.getByTestId("add-dimensions"));

    expect(changed).toHaveLength(1);
    expect(changed[0]).toEqual([{ id: "dimensions", width: 110 }]);
  });

  it("the Add button for an already-active column is disabled (never duplicates)", () => {
    render(ColumnPickerDialog, {
      available: AVAILABLE,
      active: [{ id: "dimensions", width: 120 }],
    });
    expect((screen.getByTestId("add-dimensions") as HTMLButtonElement).disabled).toBe(true);
  });

  it("removing an active column dispatches the set without it", async () => {
    const active: ActiveMetaColumn[] = [{ id: "dimensions", width: 120 }, { id: "duration", width: 90 }];
    const { component } = render(ColumnPickerDialog, { available: AVAILABLE, active });
    const changed: ActiveMetaColumn[][] = [];
    component.$on("change", (e: CustomEvent<ActiveMetaColumn[]>) => changed.push(e.detail));

    await fireEvent.click(screen.getByTestId("remove-dimensions"));

    expect(changed[0]).toEqual([{ id: "duration", width: 90 }]);
  });

  it("reordering with up/down moves the column and disables at either end", async () => {
    const active: ActiveMetaColumn[] = [
      { id: "dimensions", width: 100 },
      { id: "duration", width: 90 },
      { id: "pages", width: 80 },
    ];
    const { component } = render(ColumnPickerDialog, { available: AVAILABLE, active });
    const changed: ActiveMetaColumn[][] = [];
    component.$on("change", (e: CustomEvent<ActiveMetaColumn[]>) => changed.push(e.detail));

    // First row (dimensions) can't move up; last row (pages) can't move down.
    expect((screen.getByTestId("up-dimensions") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId("down-pages") as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.click(screen.getByTestId("down-dimensions"));
    expect(changed[0].map((c) => c.id)).toEqual(["duration", "dimensions", "pages"]);
  });

  it("close (X or the bottom button) dispatches close without changing anything", async () => {
    const { component } = render(ColumnPickerDialog, { available: AVAILABLE, active: [] });
    let closed = 0;
    component.$on("close", () => closed++);

    await fireEvent.click(screen.getByTestId("done-btn"));
    expect(closed).toBe(1);

    await fireEvent.click(screen.getByTestId("close-x"));
    expect(closed).toBe(2);
  });

  it("Escape closes the dialog", async () => {
    const { component } = render(ColumnPickerDialog, { available: AVAILABLE, active: [] });
    let closed = 0;
    component.$on("close", () => closed++);
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(closed).toBe(1);
  });

  it("a stale active id no longer in the catalog is dropped from the ACTIVE list but not crashed on", () => {
    render(ColumnPickerDialog, {
      available: AVAILABLE,
      active: [{ id: "long-removed-column", width: 100 }],
    });
    expect(screen.getByTestId("active-empty")).toBeTruthy();
  });
});
