import { describe, it, expect } from "vitest";
import {
  emptyClipboard,
  stage,
  isEmpty,
  isCut,
  canPaste,
  isSelfOrDescendant,
} from "./clipboard";

describe("clipboard", () => {
  it("starts empty", () => {
    expect(isEmpty(emptyClipboard())).toBe(true);
  });

  it("stages paths with a mode", () => {
    const c = stage(["/a/x.txt"], "cut");
    expect(isEmpty(c)).toBe(false);
    expect(c.mode).toBe("cut");
  });

  it("marks cut items (and only cut items) as cut", () => {
    expect(isCut(stage(["/a/x.txt"], "cut"), "/a/x.txt")).toBe(true);
    expect(isCut(stage(["/a/x.txt"], "copy"), "/a/x.txt")).toBe(false);
    expect(isCut(stage(["/a/x.txt"], "cut"), "/a/y.txt")).toBe(false);
  });
});

describe("isSelfOrDescendant", () => {
  it("detects the same folder", () => {
    expect(isSelfOrDescendant("/a/b", "/a/b")).toBe(true);
  });

  it("detects a descendant", () => {
    expect(isSelfOrDescendant("/a/b", "/a/b/c/d")).toBe(true);
  });

  it("does not treat a sibling as a descendant", () => {
    expect(isSelfOrDescendant("/a/b", "/a/bc")).toBe(false);
  });

  it("does not treat a parent as a descendant", () => {
    expect(isSelfOrDescendant("/a/b", "/a")).toBe(false);
  });

  it("handles Windows separators", () => {
    expect(isSelfOrDescendant("C:\\a\\b", "C:\\a\\b\\c")).toBe(true);
    expect(isSelfOrDescendant("C:\\a\\b", "C:\\a\\bc")).toBe(false);
  });
});

describe("canPaste", () => {
  it("refuses an empty clipboard", () => {
    expect(canPaste(emptyClipboard(), "/dest").allowed).toBe(false);
  });

  it("allows a normal copy into another folder", () => {
    expect(canPaste(stage(["/a/x.txt"], "copy"), "/b").allowed).toBe(true);
  });

  it("allows copying into the same folder (it auto-renames)", () => {
    expect(canPaste(stage(["/a/x.txt"], "copy"), "/a").allowed).toBe(true);
  });

  it("refuses moving a folder into itself", () => {
    const r = canPaste(stage(["/a/b"], "cut"), "/a/b");
    expect(r.allowed).toBe(false);
    expect(r.reason).toContain("itself");
  });

  it("refuses moving a folder into its own descendant", () => {
    const r = canPaste(stage(["/a/b"], "cut"), "/a/b/c");
    expect(r.allowed).toBe(false);
    expect(r.reason).toContain("itself");
  });

  it("refuses copying a folder into its own descendant", () => {
    const r = canPaste(stage(["/a/b"], "copy"), "/a/b/c");
    expect(r.allowed).toBe(false);
  });

  it("refuses a cut+paste back into the source folder as a no-op", () => {
    const r = canPaste(stage(["/a/x.txt", "/a/y.txt"], "cut"), "/a");
    expect(r.allowed).toBe(false);
    expect(r.reason).toContain("already");
  });

  it("allows a cut when only some items come from the destination", () => {
    expect(canPaste(stage(["/a/x.txt", "/b/y.txt"], "cut"), "/a").allowed).toBe(
      true,
    );
  });
});
