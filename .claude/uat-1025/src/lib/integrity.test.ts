import { describe, it, expect } from "vitest";
import { verifyManifest, hasIssues, parseManifest, serializeManifest, type ChecksumEntry } from "./integrity";

const e = (path: string, sha256: string, modified: number | null = 100, size = 10): ChecksumEntry => ({
  path,
  sha256,
  size,
  modified,
});

describe("verifyManifest (CPE-790)", () => {
  it("classifies intact / missing / new", () => {
    const base = [e("/a", "h1"), e("/gone", "h2")];
    const cur = [e("/a", "h1"), e("/added", "h3")];
    const r = verifyManifest(base, cur);
    expect(r.intact).toEqual(["/a"]);
    expect(r.missing).toEqual(["/gone"]);
    expect(r.new).toEqual(["/added"]);
  });

  it("bitrot heuristic: hash changed + mtime unchanged → corrupted; both changed → edited", () => {
    const base = [e("/rot", "h1", 100), e("/edit", "h1", 100)];
    const cur = [e("/rot", "hX", 100), e("/edit", "hY", 200)]; // /rot: same mtime, /edit: newer mtime
    const r = verifyManifest(base, cur);
    expect(r.corrupted).toEqual(["/rot"]);
    expect(r.edited).toEqual(["/edit"]);
    expect(r.intact).toEqual([]);
  });

  it("hasIssues is true only for corruption or missing", () => {
    expect(hasIssues(verifyManifest([e("/a", "h1", 100)], [e("/a", "hX", 100)]))).toBe(true); // corrupted
    expect(hasIssues(verifyManifest([e("/a", "h1")], []))).toBe(true); // missing
    expect(hasIssues(verifyManifest([e("/a", "h1", 100)], [e("/a", "hX", 200)]))).toBe(false); // edited only
    expect(hasIssues(verifyManifest([e("/a", "h1")], [e("/a", "h1"), e("/b", "h2")]))).toBe(false); // new only
  });
});

describe("parse/serialize (CPE-790)", () => {
  it("round-trips and tolerates malformed input", () => {
    const list = [e("/a", "h1", null), e("/b", "h2", 5)];
    expect(parseManifest(serializeManifest(list))).toEqual(list);
    expect(parseManifest(null)).toEqual([]);
    expect(parseManifest("nope")).toEqual([]);
    expect(parseManifest('{"x":1}')).toEqual([]); // not an array
    expect(parseManifest(JSON.stringify([{ path: "/a" }, e("/ok", "h")]))).toEqual([e("/ok", "h")]); // drops invalid
  });
});
