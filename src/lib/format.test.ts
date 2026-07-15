import { describe, it, expect } from "vitest";
import { formatSize, formatDiskFree, friendlyError, splitPath, formatPathsForClipboard } from "./format";

describe("formatSize", () => {
  it("returns an empty string for zero bytes (directories)", () => {
    expect(formatSize(0)).toBe("");
  });

  it("formats bytes with no decimal place", () => {
    expect(formatSize(1)).toBe("1 B");
    expect(formatSize(999)).toBe("999 B");
  });

  it("rolls over to the next unit at exactly 1024", () => {
    expect(formatSize(1024)).toBe("1.0 KB");
    expect(formatSize(1023)).toBe("1023 B");
  });

  it("formats larger units with one decimal place", () => {
    expect(formatSize(1536)).toBe("1.5 KB");
    expect(formatSize(1024 * 1024)).toBe("1.0 MB");
    expect(formatSize(1024 * 1024 * 1024)).toBe("1.0 GB");
  });

  it("clamps at the largest known unit rather than inventing one", () => {
    const petabyte = 1024 ** 5;
    expect(formatSize(petabyte)).toContain("TB");
  });
});

describe("friendlyError", () => {
  it("maps Windows access-denied to a permission message", () => {
    expect(friendlyError("C:\\x: Access is denied. (os error 5)")).toBe(
      "Can't open this folder — permission denied.",
    );
  });

  it("maps Unix permission-denied to a permission message", () => {
    expect(friendlyError("/root: Permission denied (os error 13)")).toBe(
      "Can't open this folder — permission denied.",
    );
  });

  it("maps a missing path to a not-found message", () => {
    expect(friendlyError("/gone: No such file or directory (os error 2)")).toBe(
      "This folder no longer exists.",
    );
  });

  it("falls back to a generic message for unknown errors", () => {
    expect(friendlyError("something bizarre happened")).toBe(
      "Can't open this folder.",
    );
  });

  it("never leaks the raw error text", () => {
    const raw = "C:\\secret\\path: Access is denied. (os error 5)";
    expect(friendlyError(raw)).not.toContain("secret");
  });
});

describe("splitPath", () => {
  it("returns nothing for an empty path", () => {
    expect(splitPath("")).toEqual([]);
  });

  it("splits a POSIX path into cumulative segments", () => {
    expect(splitPath("/home/stewart/docs")).toEqual([
      { name: "/", path: "/" },
      { name: "home", path: "/home" },
      { name: "stewart", path: "/home/stewart" },
      { name: "docs", path: "/home/stewart/docs" },
    ]);
  });

  it("handles the POSIX root on its own", () => {
    expect(splitPath("/")).toEqual([{ name: "/", path: "/" }]);
  });

  it("splits a Windows path into cumulative segments", () => {
    expect(splitPath("C:\\Users\\Stewart\\Docs")).toEqual([
      { name: "C:", path: "C:\\" },
      { name: "Users", path: "C:\\Users" },
      { name: "Stewart", path: "C:\\Users\\Stewart" },
      { name: "Docs", path: "C:\\Users\\Stewart\\Docs" },
    ]);
  });

  it("handles a bare Windows drive root", () => {
    expect(splitPath("C:\\")).toEqual([{ name: "C:", path: "C:\\" }]);
  });

  it("normalises forward slashes on Windows paths", () => {
    expect(splitPath("Z:/repos/app")).toEqual([
      { name: "Z:", path: "Z:\\" },
      { name: "repos", path: "Z:\\repos" },
      { name: "app", path: "Z:\\repos\\app" },
    ]);
  });

  it("produces a last segment whose path equals the input (round-trip)", () => {
    const p = "/home/stewart/docs";
    const segs = splitPath(p);
    expect(segs[segs.length - 1].path).toBe(p);
  });

  it("splits a Windows UNC path rooted at \\\\server\\share", () => {
    expect(splitPath("\\\\server\\share\\folder\\file")).toEqual([
      { name: "\\\\server\\share", path: "\\\\server\\share" },
      { name: "folder", path: "\\\\server\\share\\folder" },
      { name: "file", path: "\\\\server\\share\\folder\\file" },
    ]);
  });

  it("handles forward-slash UNC input equivalently", () => {
    expect(splitPath("//server/share/folder")).toEqual([
      { name: "\\\\server\\share", path: "\\\\server\\share" },
      { name: "folder", path: "\\\\server\\share\\folder" },
    ]);
  });

  it("handles a bare UNC share root", () => {
    expect(splitPath("\\\\server\\share")).toEqual([
      { name: "\\\\server\\share", path: "\\\\server\\share" },
    ]);
  });
});

describe("formatPathsForClipboard", () => {
  it("wraps a single path in double quotes", () => {
    expect(formatPathsForClipboard(["C:\\Users\\me\\a.txt"])).toBe('"C:\\Users\\me\\a.txt"');
  });

  it("wraps and newline-joins multiple paths", () => {
    expect(formatPathsForClipboard(["C:\\a", "C:\\b"])).toBe('"C:\\a"\n"C:\\b"');
  });

  it("returns an empty string for no paths", () => {
    expect(formatPathsForClipboard([])).toBe("");
  });
});

describe("formatDiskFree (CPE-403)", () => {
  it("formats free of total", () => {
    expect(formatDiskFree(12.3 * 1024 ** 3, 500 * 1024 ** 3)).toBe("12.3 GB free of 500.0 GB");
  });
  it("shows 0 B for a full drive rather than blank", () => {
    expect(formatDiskFree(0, 256 * 1024 ** 3)).toBe("0 B free of 256.0 GB");
  });
  it("returns empty when total is unknown/zero", () => {
    expect(formatDiskFree(0, 0)).toBe("");
    expect(formatDiskFree(100, -1)).toBe("");
  });
});
