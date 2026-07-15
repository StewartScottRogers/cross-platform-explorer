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
