import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { canonicalPath, samePath, treePrefixPath } from "./paths";

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

/**
 * CPE-1950 — the provenance claim, derived instead of asserted.
 *
 * `canonicalPath`'s doc comment used to say it "mirrors `Sidebar.svelte`'s pre-existing local
 * `norm()` … so every path-keyed consumer in the app agrees with the sidebar's own notion of 'the
 * same folder'". That was false the day it was written: `norm` strips trailing separators
 * unconditionally, so `norm("/") === ""` and `norm("C:/") === "C:"`, while `canonicalPath` preserves
 * both by explicit design. Nothing reddened, because the claim lived in prose and both functions had
 * green tests of their own.
 *
 * There is now one definition (`treePrefixPath`, which `Sidebar.svelte` imports and aliases to
 * `norm`), and the relationship between the two is *derived from the functions themselves* below —
 * agreement on every non-root input, and the exact, deliberate divergence at the roots. Change either
 * function and this reds.
 *
 * **Red-proofed, not assumed.** Replacing `treePrefixPath`'s body with `return canonicalPath(p)` — the
 * change the old comment claimed was already true — fails 2 of these 4 tests:
 * `/: expected '/' not to be '/'`, and `expected false to be true` on the ancestor leg. Reverted.
 */
describe("canonicalPath vs treePrefixPath — derived, not claimed (CPE-1950)", () => {
  /** Every shape either function is fed in this app: local, UNC, drive-relative, scheme'd remote. */
  const NON_ROOT_INPUTS = [
    "sftp://h/srv/sub",
    "sftp://h/srv/sub/",
    "sftp://h/srv/sub///",
    "C:\\Users\\me\\docs",
    "C:/Users/me/docs/",
    "/home/me",
    "/home/me/",
    "//server/share/dir",
    "davs://h:8443/dav/photos/",
  ];

  it("the two agree on every NON-root input — which is the part the old claim got right", () => {
    for (const p of NON_ROOT_INPUTS) {
      expect(canonicalPath(p), p).toBe(treePrefixPath(p));
    }
  });

  it("...and disagree at exactly the roots, which is the part it got wrong", () => {
    // Derived, not restated: for each root, assert the two functions return DIFFERENT values and
    // spell out which is which. If someone "fixes" the divergence, this reds here rather than the
    // sidebar's reveal quietly breaking.
    for (const root of ["/", "C:/", "C:\\"]) {
      expect(canonicalPath(root), root).not.toBe(treePrefixPath(root));
    }
    expect(canonicalPath("/")).toBe("/");
    expect(treePrefixPath("/")).toBe("");
    expect(canonicalPath("C:/")).toBe("C:/");
    expect(treePrefixPath("C:/")).toBe("C:");
  });

  it("the divergence is what makes the sidebar's ancestor test work at the root", () => {
    // `Sidebar.svelte`'s isAncestorOrSelf, verbatim in shape: b === a || b.startsWith(a + "/").
    const isAncestorOrSelf = (norm: (p: string) => string, anc: string, p: string) => {
      const a = norm(anc);
      const b = norm(p);
      return b === a || b.startsWith(`${a}/`);
    };
    expect(isAncestorOrSelf(treePrefixPath, "/", "/home/me")).toBe(true);
    // The counterfactual, executed rather than described: swapping in canonicalPath breaks the POSIX
    // root, because "/home/me" does not start with "//". This is why the two must differ.
    expect(isAncestorOrSelf(canonicalPath, "/", "/home/me")).toBe(false);
  });

  it("Sidebar.svelte no longer carries its own copy of the normaliser", () => {
    // Code-anchored, not prose-anchored: the old `const norm = (p: string) => p.replace(...)` body is
    // gone from the component, so there is nothing left to drift from `treePrefixPath`.
    const sidebar = readFileSync(
      join(process.cwd(), "src", "lib", "components", "Sidebar.svelte"),
      "utf8",
    );
    expect(sidebar).toContain('import { treePrefixPath } from "../paths"');
    expect(
      sidebar.includes("const norm = (p: string) =>"),
      "Sidebar.svelte re-declared its own `norm` — that is the duplication CPE-1950 removed; import " +
        "`treePrefixPath` from $lib/paths instead so there stays exactly one definition.",
    ).toBe(false);
  });
});
