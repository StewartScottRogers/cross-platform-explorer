import { describe, it, expect } from "vitest";
import { expandTemplate, expandForSelection } from "./cmdTemplate";
import type { DirEntry } from "./types";

const e = (path: string, name: string): DirEntry =>
  ({ name, path, is_dir: false, size: 0, modified: 0 }) as DirEntry;

const photo = e("C:\\pics\\holiday.jpg", "holiday.jpg");
const readme = e("/home/u/proj/README", "README");

describe("expandTemplate (CPE-781)", () => {
  it("substitutes every known token", () => {
    expect(expandTemplate("{path}", photo)).toBe("C:\\pics\\holiday.jpg");
    expect(expandTemplate("{name}", photo)).toBe("holiday.jpg");
    expect(expandTemplate("{dir}", photo)).toBe("C:\\pics");
    expect(expandTemplate("{ext}", photo)).toBe("jpg");
    expect(expandTemplate("{stem}", photo)).toBe("holiday");
  });

  it("handles a full command with repeated tokens", () => {
    expect(expandTemplate("convert {path} {stem}.png", photo)).toBe("convert C:\\pics\\holiday.jpg holiday.png");
  });

  it("leaves unknown tokens verbatim and unescapes {{ }}", () => {
    expect(expandTemplate("{bogus} {name}", photo)).toBe("{bogus} holiday.jpg");
    expect(expandTemplate("echo {{literal}} {name}", photo)).toBe("echo {literal} holiday.jpg");
  });

  it("extension-less names: ext empty, stem is the whole name; unix dir", () => {
    expect(expandTemplate("{ext}", readme)).toBe("");
    expect(expandTemplate("{stem}", readme)).toBe("README");
    expect(expandTemplate("{dir}", readme)).toBe("/home/u/proj");
  });
});

describe("expandForSelection (CPE-781)", () => {
  const sel = [e("/a/one.txt", "one.txt"), e("/a/two.txt", "two.txt")];

  it("each mode: one expansion per entry", () => {
    expect(expandForSelection("cat {path}", sel, "each")).toEqual(["cat /a/one.txt", "cat /a/two.txt"]);
  });

  it("joined mode: a single string with a quoted list per token", () => {
    expect(expandForSelection("zip out.zip {path}", sel, "joined")).toEqual(['zip out.zip "/a/one.txt" "/a/two.txt"']);
  });

  it("empty selection → []", () => {
    expect(expandForSelection("cat {path}", [], "each")).toEqual([]);
    expect(expandForSelection("cat {path}", [], "joined")).toEqual([]);
  });
});
