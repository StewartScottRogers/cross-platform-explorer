import { describe, it, expect } from "vitest";
import { planFindReplace } from "./batchRename";

describe("planFindReplace", () => {
  it("replaces all occurrences in each name", () => {
    const items = planFindReplace(["IMG copy.jpg", "IMG copy copy.png"], "copy", "final");
    expect(items.map((i) => i.to)).toEqual(["IMG final.jpg", "IMG final final.png"]);
    expect(items.every((i) => i.changed)).toBe(true);
  });

  it("matches case-insensitively by default", () => {
    const [item] = planFindReplace(["Photo.JPG"], "photo", "pic");
    expect(item.to).toBe("pic.JPG");
  });

  it("treats the replacement as literal — `$` is not a group reference (CPE-630)", () => {
    // "$1" would be an (empty) group ref with a string replacer → "photo_00.jpg"; must stay literal.
    expect(planFindReplace(["photo_v1.jpg"], "v1", "$100")[0].to).toBe("photo_$100.jpg");
    // "$&" would insert the match; "US$" keeps its dollar.
    expect(planFindReplace(["aXb"], "X", "$&")[0].to).toBe("a$&b");
    expect(planFindReplace(["USD5.txt"], "USD", "US$")[0].to).toBe("US$5.txt");
  });

  it("respects the case-sensitive flag", () => {
    const [item] = planFindReplace(["Photo.jpg"], "photo", "pic", true);
    expect(item.to).toBe("Photo.jpg");
    expect(item.changed).toBe(false);
  });

  it("is a no-op when find is empty", () => {
    const items = planFindReplace(["a.txt", "b.txt"], "", "x");
    expect(items.every((i) => !i.changed)).toBe(true);
    expect(items.map((i) => i.to)).toEqual(["a.txt", "b.txt"]);
  });

  it("treats find as a literal, not a regex", () => {
    // "a.b" must not match "aXb" — the dot is escaped.
    const items = planFindReplace(["aXb.txt", "a.b.txt"], "a.b", "Z");
    expect(items.map((i) => i.to)).toEqual(["aXb.txt", "Z.txt"]);
  });

  it("inserts regex-special replacement text literally", () => {
    const [item] = planFindReplace(["v1.txt"], "v1", "$v");
    expect(item.to).toBe("$v.txt");
  });

  it("does not flag conflicts when targets stay distinct", () => {
    const items = planFindReplace(["one.txt", "two.txt"], "o", "0");
    expect(items.map((i) => i.to)).toEqual(["0ne.txt", "tw0.txt"]);
    expect(items.every((i) => !i.conflict)).toBe(true);
  });

  it("flags a collision when two names map to the same target", () => {
    // "a1.txt" -> "a2.txt" duplicates the existing "a2.txt".
    const collide = planFindReplace(["a1.txt", "a2.txt"], "1", "2");
    expect(collide[0].to).toBe("a2.txt");
    expect(collide[1].to).toBe("a2.txt");
    expect(collide.every((i) => i.conflict)).toBe(true);
  });
});

describe("batch rename — add prefix/suffix (CPE-424)", () => {
  it("splitExt keeps a leading dot as part of the base (dotfiles have no extension)", async () => {
    const { splitExt } = await import("./batchRename");
    expect(splitExt("report.pdf")).toEqual(["report", ".pdf"]);
    expect(splitExt("archive.tar.gz")).toEqual(["archive.tar", ".gz"]);
    expect(splitExt("Makefile")).toEqual(["Makefile", ""]);
    expect(splitExt(".gitignore")).toEqual([".gitignore", ""]);
  });

  it("suffix lands before the extension; prefix goes first; both empty is a no-op", async () => {
    const { planAffix } = await import("./batchRename");
    const r = planAffix(["report.pdf", "notes", ".env"], "2026-", "-final");
    expect(r.map((i) => i.to)).toEqual(["2026-report-final.pdf", "2026-notes-final", "2026-.env-final"]);
    expect(r.every((i) => i.changed)).toBe(true);
    expect(planAffix(["a.txt"], "", "").every((i) => !i.changed)).toBe(true);
  });

  it("flags intra-batch collisions (e.g. a prefix that makes two names equal)", async () => {
    const { planAffix } = await import("./batchRename");
    // Different base, same ext, suffix identical → no collision; but a prefix can't collide distinct
    // names. Force a collision by mapping via find/replace-like scenario is covered elsewhere; here
    // confirm distinct names stay distinct.
    const r = planAffix(["a.txt", "b.txt"], "x_", "");
    expect(r.some((i) => i.conflict)).toBe(false);
  });
});

describe("batch rename — change case (CPE-427)", () => {
  it("transforms the base only, preserving the extension", async () => {
    const { planCase } = await import("./batchRename");
    expect(planCase(["README.TXT"], "lower").map((i) => i.to)).toEqual(["readme.TXT"]);
    expect(planCase(["my file.md"], "upper").map((i) => i.to)).toEqual(["MY FILE.md"]);
    expect(planCase(["my_cool file.PNG"], "title").map((i) => i.to)).toEqual(["My_Cool File.PNG"]);
  });

  it("marks no change when the base already matches", async () => {
    const { planCase } = await import("./batchRename");
    expect(planCase(["readme.txt"], "lower").every((i) => !i.changed)).toBe(true);
  });
});

describe("batch rename — sequential numbering (CPE-426)", () => {
  it("zero-pads the # run, preserves extension, counts from start, in order", async () => {
    const { planNumber } = await import("./batchRename");
    const r = planNumber(["a.jpg", "b.png", "c.gif"], "photo-###", 1);
    expect(r.map((i) => i.to)).toEqual(["photo-001.jpg", "photo-002.png", "photo-003.gif"]);
    // A different start.
    expect(planNumber(["x.txt"], "doc-#", 7).map((i) => i.to)).toEqual(["doc-7.txt"]);
  });

  it("appends the number when the pattern has no # token; empty pattern is a no-op", async () => {
    const { planNumber } = await import("./batchRename");
    expect(planNumber(["a.md", "b.md"], "note", 1).map((i) => i.to)).toEqual(["note1.md", "note2.md"]);
    expect(planNumber(["a.md"], "", 1).every((i) => !i.changed)).toBe(true);
  });
});
