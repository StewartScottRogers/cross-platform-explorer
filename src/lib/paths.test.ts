import { describe, it, expect } from "vitest";
import { canonicalPath, samePath } from "./paths";

describe("canonicalPath (CPE-1737 round 2)", () => {
  it("strips exactly one trailing slash", () => {
    expect(canonicalPath("sftp://h/srv/sub/")).toBe("sftp://h/srv/sub");
    expect(canonicalPath("sftp://h/srv/sub")).toBe("sftp://h/srv/sub");
  });

  it("collapses multiple trailing slashes to none", () => {
    expect(canonicalPath("sftp://h/srv/sub///")).toBe("sftp://h/srv/sub");
  });

  it("normalises backslashes to forward slashes", () => {
    expect(canonicalPath("C:\\Users\\me\\docs")).toBe("C:/Users/me/docs");
  });

  it("leaves a bare root ('/') alone rather than stripping it to an empty string", () => {
    expect(canonicalPath("/")).toBe("/");
  });

  it("keeps a Windows drive root's trailing separator — bare 'C:' means something different (cwd on that drive)", () => {
    expect(canonicalPath("C:\\")).toBe("C:/");
    expect(canonicalPath("C:/")).toBe("C:/");
  });

  it("a scheme root ('sftp://host/') is stripped like any other trailing slash — safe because location::parse treats an empty path as '/' either way", () => {
    expect(canonicalPath("sftp://host/")).toBe("sftp://host");
  });

  it("CPE-1737 round 3: a BARE drive letter with no separator at all is left untouched, never promoted into a root", () => {
    // Regression: an earlier version of this function fired the drive-root guard unconditionally once
    // `stripped` matched /^[A-Za-z]:$/ — which also matches a bare "C:" that never had a trailing
    // separator to strip in the first place, rewriting it into "C:/" (the root). That directly
    // contradicts this file's own doc comment: a bare "C:" and its root are supposed to mean two
    // different things, not canonicalise into the same value.
    expect(canonicalPath("C:")).toBe("C:");
  });
});

describe("samePath (CPE-1737 round 2)", () => {
  it("treats a slashed and un-slashed spelling of the same folder as equal", () => {
    expect(samePath("sftp://h/srv/sub", "sftp://h/srv/sub/")).toBe(true);
  });

  it("treats a backslash and forward-slash spelling as equal", () => {
    expect(samePath("C:\\Users\\me", "C:/Users/me")).toBe(true);
  });

  it("returns false for genuinely different paths", () => {
    expect(samePath("sftp://h/srv/sub", "sftp://h/srv/other")).toBe(false);
  });
});
