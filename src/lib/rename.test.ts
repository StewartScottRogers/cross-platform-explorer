import { describe, it, expect } from "vitest";
import {
  applyRecipe,
  validate,
  previewRename,
  type RenameRecipe,
  type RenameResult,
} from "./rename";

/** Small helper: apply a recipe and return just the `to` names. */
function names(input: string[], recipe: RenameRecipe): string[] {
  return applyRecipe(input, recipe).map((r) => r.to);
}

describe("applyRecipe — operations", () => {
  it("replace: literal, first vs all, case-insensitive, name-scoped by default", () => {
    expect(names(["a-b-c.txt"], [{ kind: "replace", find: "-", replace: "_" }])).toEqual([
      "a_b-c.txt", // default first-only
    ]);
    expect(names(["a-b-c.txt"], [{ kind: "replace", find: "-", replace: "_", all: true }])).toEqual([
      "a_b_c.txt",
    ]);
    expect(
      names(["Photo.JPG"], [{ kind: "replace", find: "photo", replace: "img", caseInsensitive: true }]),
    ).toEqual(["img.JPG"]); // ext untouched (name scope)
  });

  it("replace: literal replacement with $ is not treated as a regex reference", () => {
    expect(names(["price.txt"], [{ kind: "replace", find: "price", replace: "$5" }])).toEqual([
      "$5.txt",
    ]);
  });

  it("replace: regex mode with a backreference, and invalid regex is a no-op", () => {
    expect(
      names(["report2026.txt"], [{ kind: "replace", find: "(\\d+)", replace: "-$1-", regex: true }]),
    ).toEqual(["report-2026-.txt"]);
    // Unbalanced group → invalid regex → name unchanged, no throw.
    expect(names(["x.txt"], [{ kind: "replace", find: "(", replace: "y", regex: true }])).toEqual([
      "x.txt",
    ]);
  });

  it("case: lower/upper/title/sentence over the name", () => {
    expect(names(["My File.TXT"], [{ kind: "case", mode: "lower" }])).toEqual(["my file.TXT"]);
    expect(names(["my file.txt"], [{ kind: "case", mode: "upper" }])).toEqual(["MY FILE.txt"]);
    expect(names(["hello world.txt"], [{ kind: "case", mode: "title" }])).toEqual([
      "Hello World.txt",
    ]);
    expect(names(["hELLO wORLD.txt"], [{ kind: "case", mode: "sentence" }])).toEqual([
      "Hello world.txt",
    ]);
  });

  it("insert: prefix, suffix, and at an index", () => {
    expect(names(["file.txt"], [{ kind: "insert", text: "IMG_", position: "prefix" }])).toEqual([
      "IMG_file.txt",
    ]);
    expect(names(["file.txt"], [{ kind: "insert", text: "_v2", position: "suffix" }])).toEqual([
      "file_v2.txt",
    ]);
    expect(names(["file.txt"], [{ kind: "insert", text: "X", position: 2 }])).toEqual(["fiXle.txt"]);
  });

  it("remove: an index range within the name", () => {
    expect(names(["draft_report.txt"], [{ kind: "remove", from: 0, count: 6 }])).toEqual([
      "report.txt",
    ]);
    // count omitted → to end of the name part.
    expect(names(["report_final.txt"], [{ kind: "remove", from: 6 }])).toEqual(["report.txt"]);
  });

  it("trim: trims and collapses internal whitespace", () => {
    expect(names(["  a   b .txt"], [{ kind: "trim" }])).toEqual(["a b.txt"]);
  });

  it("number: start/step/padding/position with a separator", () => {
    expect(
      names(
        ["a.txt", "b.txt", "c.txt"],
        [{ kind: "number", start: 1, padding: 3, position: "prefix", separator: "_" }],
      ),
    ).toEqual(["001_a.txt", "002_b.txt", "003_c.txt"]);
    expect(
      names(
        ["a.txt", "b.txt"],
        [{ kind: "number", start: 10, step: 5, position: "suffix", separator: "-" }],
      ),
    ).toEqual(["a-10.txt", "b-15.txt"]);
  });

  it("extension: set replaces, add only when missing, strip removes", () => {
    expect(names(["photo.jpeg"], [{ kind: "extension", mode: "set", ext: "jpg" }])).toEqual([
      "photo.jpg",
    ]);
    expect(names(["photo.jpeg"], [{ kind: "extension", mode: "set", ext: ".png" }])).toEqual([
      "photo.png",
    ]); // tolerates a leading dot
    expect(names(["README"], [{ kind: "extension", mode: "add", ext: "md" }])).toEqual(["README.md"]);
    expect(names(["photo.jpg"], [{ kind: "extension", mode: "add", ext: "png" }])).toEqual([
      "photo.jpg",
    ]); // already has one → unchanged
    expect(names(["photo.jpg"], [{ kind: "extension", mode: "strip" }])).toEqual(["photo"]);
  });

  it("scope: ext and full change which part is transformed", () => {
    expect(names(["Photo.JPG"], [{ kind: "case", mode: "lower", scope: "ext" }])).toEqual([
      "Photo.jpg",
    ]);
    expect(names(["My.File.TXT"], [{ kind: "case", mode: "lower", scope: "full" }])).toEqual([
      "my.file.txt",
    ]);
  });

  it("composes operations in order and reports changed", () => {
    const res = applyRecipe(
      ["Draft Report.txt"],
      [
        { kind: "replace", find: " ", replace: "_", all: true },
        { kind: "case", mode: "lower" },
        { kind: "number", start: 1, padding: 2, position: "prefix", separator: "-" },
      ],
    );
    expect(res).toEqual<RenameResult[]>([
      { from: "Draft Report.txt", to: "01-draft_report.txt", changed: true },
    ]);
  });

  it("marks an unchanged name as not changed", () => {
    expect(applyRecipe(["keep.txt"], [{ kind: "replace", find: "zzz", replace: "q" }])).toEqual([
      { from: "keep.txt", to: "keep.txt", changed: false },
    ]);
  });
});

describe("validate", () => {
  it("flags duplicate targets as collisions", () => {
    const results = applyRecipe(
      ["a.txt", "b.txt"],
      [{ kind: "replace", find: "a", replace: "x" }], // a.txt→x.txt, b.txt unchanged — no dup here
    );
    // Force a duplicate: rename both to the same target.
    const dup: RenameResult[] = [
      { from: "a.txt", to: "same.txt", changed: true },
      { from: "b.txt", to: "same.txt", changed: true },
    ];
    const v = validate(dup);
    expect(v[0].collision).toBe(true);
    expect(v[1].collision).toBe(true);
    // sanity: the non-colliding real recipe above doesn't false-positive
    expect(validate(results).some((r) => r.collision)).toBe(false);
  });

  it("flags a collision with an untouched sibling passed in `existing`", () => {
    const results: RenameResult[] = [{ from: "a.txt", to: "b.txt", changed: true }];
    expect(validate(results, { existing: ["b.txt"] })[0].collision).toBe(true);
    // Renaming a file to its own existing name is not a collision.
    const noop: RenameResult[] = [{ from: "b.txt", to: "b.txt", changed: false }];
    expect(validate(noop, { existing: ["b.txt"] })[0].collision).toBe(false);
  });

  it("collision matching is case-insensitive on win, case-sensitive on posix", () => {
    const results: RenameResult[] = [
      { from: "a.txt", to: "File.txt", changed: true },
      { from: "b.txt", to: "file.txt", changed: true },
    ];
    expect(validate(results, { platform: "win" }).every((r) => r.collision)).toBe(true);
    expect(validate(results, { platform: "posix" }).some((r) => r.collision)).toBe(false);
  });

  it("flags no-ops", () => {
    const results: RenameResult[] = [{ from: "x.txt", to: "x.txt", changed: false }];
    expect(validate(results)[0].noop).toBe(true);
  });

  it("flags invalid names: empty, illegal chars, and Windows-reserved", () => {
    const bad: RenameResult[] = [
      { from: "a", to: "   ", changed: true },
      { from: "b", to: "a<b>.txt", changed: true },
      { from: "c", to: "NUL", changed: true },
      { from: "d", to: "CON.txt", changed: true },
    ];
    const v = validate(bad, { platform: "win" });
    expect(v.map((x) => x.invalid)).toEqual([true, true, true, true]);
    // A slash is illegal on posix too.
    expect(validate([{ from: "e", to: "a/b", changed: true }], { platform: "posix" })[0].invalid).toBe(
      true,
    );
    // A colon is fine on posix (only illegal on Windows).
    expect(validate([{ from: "f", to: "a:b", changed: true }], { platform: "posix" })[0].invalid).toBe(
      false,
    );
  });
});

describe("previewRename", () => {
  it("zips results with validation flags", () => {
    const rows = previewRename(
      ["a.txt", "b.txt"],
      [{ kind: "replace", find: /[ab]/.source, replace: "same", regex: true }],
      { platform: "win" },
    );
    expect(rows[0].to).toBe("same.txt");
    expect(rows[1].to).toBe("same.txt");
    expect(rows.every((r) => r.collision)).toBe(true);
  });
});
