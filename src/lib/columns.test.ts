import { describe, it, expect } from "vitest";
import {
  columnsTemplate,
  resizeColumnTo,
  boundaryOffsets,
  COLUMN_MINS,
  NAME_COL_MIN,
  MID_MIN,
  fullMins,
  addMetaColumn,
  removeMetaColumn,
  moveMetaColumn,
  clampMetaWidths,
  appliesToAllFiles,
  columnAppliesTo,
  META_COL_MIN,
  META_COL_DEFAULT_WIDTH,
  type ActiveMetaColumn,
} from "./columns";

describe("columnsTemplate (CPE-350)", () => {
  it("renders px widths plus a trailing 1fr spacer", () => {
    expect(columnsTemplate([320, 150, 120, 90])).toBe("320px 150px 120px 90px 1fr");
  });
  it("rounds fractional widths", () => {
    expect(columnsTemplate([100.4, 99.6])).toBe("100px 100px 1fr");
  });
});

describe("resizeColumnTo (CPE-350)", () => {
  const w = [320, 150, 120, 90];

  it("sets a column width and does not mutate the input", () => {
    const out = resizeColumnTo(w, 0, 400);
    expect(out[0]).toBe(400);
    expect(w[0]).toBe(320);
  });

  it("clamps below the per-column minimum (never collapses to zero)", () => {
    expect(resizeColumnTo(w, 3, 5)[3]).toBe(COLUMN_MINS[3]); // Size min
    expect(resizeColumnTo(w, 0, -100)[0]).toBe(COLUMN_MINS[0]);
  });

  it("clamps above the maximum", () => {
    expect(resizeColumnTo(w, 0, 99999)[0]).toBe(1200);
  });

  it("ignores an out-of-range index", () => {
    expect(resizeColumnTo(w, 9, 500)).toEqual(w);
    expect(resizeColumnTo(w, -1, 500)).toEqual(w);
  });
});

describe("boundaryOffsets (CPE-350)", () => {
  it("accumulates from the left padding to each column's right edge", () => {
    expect(boundaryOffsets([320, 150, 120, 90], 10)).toEqual([330, 480, 600, 690]);
  });
});

// CPE-1140: the middle (file-list) pane's derived minimum must never be smaller than the
// leftmost Name column's own minimum — the middle pane can never be narrower than that column.
describe("MID_MIN (CPE-1140)", () => {
  it("equals NAME_COL_MIN", () => {
    expect(NAME_COL_MIN).toBe(COLUMN_MINS[0]);
  });
  it("is at least the Name column's own minimum", () => {
    expect(MID_MIN).toBeGreaterThanOrEqual(NAME_COL_MIN);
  });
  it("is at least the sum of every visible column's minimum width", () => {
    const sum = COLUMN_MINS.reduce((a, b) => a + b, 0);
    expect(MID_MIN).toBeGreaterThanOrEqual(sum);
  });
});

// CPE-1146 (epic CPE-707): the active-metadata-column model — add/remove/reorder + width clamping,
// which the column picker + FileList's dynamic header/resize logic build on.
describe("fullMins (CPE-1146)", () => {
  it("appends one META_COL_MIN per active metadata column after the 4 built-in minimums", () => {
    expect(fullMins(0)).toEqual(COLUMN_MINS);
    expect(fullMins(2)).toEqual([...COLUMN_MINS, META_COL_MIN, META_COL_MIN]);
  });
});

describe("addMetaColumn / removeMetaColumn / moveMetaColumn (CPE-1146)", () => {
  it("appends a new column at the default width, in order", () => {
    const a = addMetaColumn([], "dimensions");
    expect(a).toEqual([{ id: "dimensions", width: META_COL_DEFAULT_WIDTH }]);
    const b = addMetaColumn(a, "duration");
    expect(b.map((c) => c.id)).toEqual(["dimensions", "duration"]);
  });

  it("adding an already-active column is a no-op (never duplicates)", () => {
    const a: ActiveMetaColumn[] = [{ id: "dimensions", width: 120 }];
    expect(addMetaColumn(a, "dimensions")).toBe(a); // same reference: genuinely untouched
  });

  it("does not mutate the input array", () => {
    const a: ActiveMetaColumn[] = [];
    addMetaColumn(a, "dimensions");
    expect(a).toEqual([]);
  });

  it("removes a column by id, leaving the rest in order", () => {
    const a: ActiveMetaColumn[] = [
      { id: "dimensions", width: 100 },
      { id: "duration", width: 90 },
      { id: "pages", width: 80 },
    ];
    expect(removeMetaColumn(a, "duration").map((c) => c.id)).toEqual(["dimensions", "pages"]);
  });

  it("removing an id that isn't active is a no-op", () => {
    const a: ActiveMetaColumn[] = [{ id: "dimensions", width: 100 }];
    expect(removeMetaColumn(a, "nope")).toEqual(a);
  });

  it("moves a column earlier/later in display order", () => {
    const a: ActiveMetaColumn[] = [
      { id: "dimensions", width: 100 },
      { id: "duration", width: 90 },
      { id: "pages", width: 80 },
    ];
    expect(moveMetaColumn(a, "pages", -1).map((c) => c.id)).toEqual(["dimensions", "pages", "duration"]);
    expect(moveMetaColumn(a, "dimensions", 1).map((c) => c.id)).toEqual(["duration", "dimensions", "pages"]);
  });

  it("moving past either end is a no-op", () => {
    const a: ActiveMetaColumn[] = [{ id: "dimensions", width: 100 }, { id: "duration", width: 90 }];
    expect(moveMetaColumn(a, "dimensions", -1)).toEqual(a);
    expect(moveMetaColumn(a, "duration", 1)).toEqual(a);
    expect(moveMetaColumn(a, "nope", 1)).toEqual(a);
  });
});

describe("clampMetaWidths (CPE-1146, CPE-1140-style load guard)", () => {
  it("floors a too-narrow persisted width at META_COL_MIN", () => {
    expect(clampMetaWidths([{ id: "dimensions", width: 5 }])[0].width).toBe(META_COL_MIN);
  });

  it("caps an absurd persisted width at the shared COLUMN_MAX", () => {
    expect(clampMetaWidths([{ id: "dimensions", width: 999999 }])[0].width).toBe(1200);
  });

  it("leaves an already-valid width untouched (rounded)", () => {
    expect(clampMetaWidths([{ id: "dimensions", width: 110.4 }])[0].width).toBe(110);
  });
});

// CPE-1166: the "applies to all files" sentinel — an empty `extensions` list means the column applies
// to every file (the magic-byte detectors), so it must never be greyed out by the extension gate.
describe("appliesToAllFiles / columnAppliesTo (CPE-1166)", () => {
  it("treats an empty extensions list as applies-to-all", () => {
    expect(appliesToAllFiles([])).toBe(true);
    expect(appliesToAllFiles(["png", "jpg"])).toBe(false);
  });

  it("an applies-to-all column matches every extension (never greyed out)", () => {
    expect(columnAppliesTo([], "png")).toBe(true);
    expect(columnAppliesTo([], "exe")).toBe(true);
    expect(columnAppliesTo([], "")).toBe(true);
  });

  it("an extension-scoped column matches only its listed extensions", () => {
    expect(columnAppliesTo(["png", "jpg"], "png")).toBe(true);
    expect(columnAppliesTo(["png", "jpg"], "pdf")).toBe(false);
  });
});
