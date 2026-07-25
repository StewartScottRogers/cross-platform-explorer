// CPE-526: unified-diff parser — files/hunks/typed lines, stats, labels, tolerance.
import { describe, it, expect } from "vitest";
import { parseDiff, diffStats, fileStats, fileLabel, inlineDiff, annotateInline, toPatch } from "./diff";

const SAMPLE = `diff --git a/src/app.ts b/src/app.ts
index 1111..2222 100644
--- a/src/app.ts
+++ b/src/app.ts
@@ -1,3 +1,4 @@
 const x = 1;
-const y = 2;
+const y = 3;
+const z = 4;
 export { x };
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
-old title
+new title`;

describe("unified-diff parser (CPE-526)", () => {
  it("splits into files with cleaned paths", () => {
    const files = parseDiff(SAMPLE);
    expect(files.map((f) => f.newPath)).toEqual(["src/app.ts", "README.md"]);
    expect(files[0].oldPath).toBe("src/app.ts");
  });

  it("parses hunks with typed add/del/context lines", () => {
    const [app] = parseDiff(SAMPLE);
    expect(app.hunks.length).toBe(1);
    const kinds = app.hunks[0].lines.map((l) => l.kind);
    expect(kinds).toEqual(["context", "del", "add", "add", "context"]);
    expect(app.hunks[0].lines[2].text).toBe("const y = 3;");
    expect(app.hunks[0].header).toContain("@@");
  });

  it("tracks old/new line numbers per line from the @@ header (CPE-566)", () => {
    const [app] = parseDiff(SAMPLE);
    const ls = app.hunks[0].lines;
    expect(ls[0]).toMatchObject({ kind: "context", oldLine: 1, newLine: 1 });
    expect(ls[1]).toMatchObject({ kind: "del", oldLine: 2 });
    expect(ls[1].newLine).toBeUndefined();
    expect(ls[2]).toMatchObject({ kind: "add", newLine: 2 });
    expect(ls[2].oldLine).toBeUndefined();
    expect(ls[3]).toMatchObject({ kind: "add", newLine: 3 });
    expect(ls[4]).toMatchObject({ kind: "context", oldLine: 3, newLine: 4 });
  });

  it("computes add/remove/file stats", () => {
    expect(diffStats(parseDiff(SAMPLE))).toEqual({ added: 3, removed: 2, files: 2 });
  });

  it("computes an intra-line diff: common prefix/suffix unchanged, middle changed (CPE-570)", () => {
    const d = inlineDiff("const y = 2;", "const y = 3;");
    expect(d.old).toEqual([
      { text: "const y = ", changed: false },
      { text: "2", changed: true },
      { text: ";", changed: false },
    ]);
    expect(d.new).toEqual([
      { text: "const y = ", changed: false },
      { text: "3", changed: true },
      { text: ";", changed: false },
    ]);
  });

  it("intra-line diff handles identical + fully-different lines", () => {
    expect(inlineDiff("same", "same")).toEqual({ old: [{ text: "same", changed: false }], new: [{ text: "same", changed: false }] });
    expect(inlineDiff("abc", "xyz")).toEqual({ old: [{ text: "abc", changed: true }], new: [{ text: "xyz", changed: true }] });
    // pure insertion: old is all common, new has an added middle
    expect(inlineDiff("ac", "abc").new).toEqual([{ text: "a", changed: false }, { text: "b", changed: true }, { text: "c", changed: false }]);
  });

  it("annotates a modified del→add pair with inline segments, leaves others plain (CPE-570)", () => {
    const [app] = parseDiff(SAMPLE); // context, del(y=2), add(y=3), add(z=4), context
    const rows = annotateInline(app.hunks[0].lines);
    expect(rows[0].segs).toBeUndefined(); // context — plain
    expect(rows[1].segs).toBeDefined(); // del of the modified pair
    expect(rows[2].segs).toBeDefined(); // add of the modified pair
    expect(rows[2].segs!.some((s) => s.changed && s.text === "3")).toBe(true);
    expect(rows[3].segs).toBeUndefined(); // a pure add (z=4), not paired
  });

  it("reconstructs a file's unified-diff text (CPE-572)", () => {
    const [app] = parseDiff(SAMPLE);
    expect(toPatch(app)).toBe(
      "diff --git a/src/app.ts b/src/app.ts\n" +
        "--- a/src/app.ts\n" +
        "+++ b/src/app.ts\n" +
        "@@ -1,3 +1,4 @@\n" +
        " const x = 1;\n" +
        "-const y = 2;\n" +
        "+const y = 3;\n" +
        "+const z = 4;\n" +
        " export { x };\n",
    );
    // round-trips: re-parsing the reconstructed patch yields the same hunks.
    expect(parseDiff(toPatch(app))[0].hunks[0].lines.map((l) => l.kind)).toEqual(app.hunks[0].lines.map((l) => l.kind));
  });

  it("computes per-file add/remove stats (CPE-567)", () => {
    const [app, readme] = parseDiff(SAMPLE);
    expect(fileStats(app)).toEqual({ added: 2, removed: 1 });
    expect(fileStats(readme)).toEqual({ added: 1, removed: 1 });
  });

  it("labels new / deleted / renamed / modified files", () => {
    expect(fileLabel({ oldPath: "a.ts", newPath: "a.ts", binary: false, hunks: [] })).toBe("a.ts");
    expect(fileLabel({ oldPath: "/dev/null", newPath: "new.ts", binary: false, hunks: [] })).toBe("new.ts (new)");
    expect(fileLabel({ oldPath: "old.ts", newPath: "/dev/null", binary: false, hunks: [] })).toBe("old.ts (deleted)");
    expect(fileLabel({ oldPath: "a.ts", newPath: "b.ts", binary: false, hunks: [] })).toBe("a.ts → b.ts");
  });

  it("flags a binary file and parses no hunks for it", () => {
    const bin = parseDiff(`diff --git a/logo.png b/logo.png\nBinary files a/logo.png and b/logo.png differ`);
    expect(bin[0].binary).toBe(true);
    expect(bin[0].hunks).toEqual([]);
  });

  it("is tolerant of empty / malformed input", () => {
    expect(parseDiff("")).toEqual([]);
    expect(parseDiff("not a diff at all")).toEqual([]);
    // Hunk lines before any file header are ignored, not a crash.
    expect(parseDiff("@@ -1 +1 @@\n+orphan")).toEqual([]);
  });
});
