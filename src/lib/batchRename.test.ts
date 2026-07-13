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
