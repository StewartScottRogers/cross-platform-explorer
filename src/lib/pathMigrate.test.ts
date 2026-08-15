/**
 * Unit tests for the shared frontend path-migration helper (CPE-1224): rename/move must carry
 * favourites, spotlight frecency, and recents/recent-folders entries to their new path — exact match
 * and subtree — mirroring the backend `tag_store_rename_subtree` boundary rule (CPE-1222). Each of the
 * three real stores' shapes (`Favorite`, `RecentFile`, `Visit`) is exercised through `migratePathList`
 * to prove the helper genuinely serves all of them, not just an abstract `{ path }` shape.
 */
import { describe, it, expect } from "vitest";
import { migratedPath, migratePathList } from "./pathMigrate";
import type { Favorite, RecentFile } from "./types";
import type { Visit } from "./bindings.gen";

describe("migratedPath — pure path rewrite (CPE-1224)", () => {
  it("re-keys an exact match", () => {
    expect(migratedPath("/a/b", "/a/b", "/a/renamed")).toBe("/a/renamed");
  });

  it("re-keys a subtree entry under a `/`-separated from path", () => {
    expect(migratedPath("/a/b/c.txt", "/a/b", "/a/renamed")).toBe("/a/renamed/c.txt");
    expect(migratedPath("/a/b/sub/d.txt", "/a/b", "/a/renamed")).toBe("/a/renamed/sub/d.txt");
  });

  it("re-keys a subtree entry under a `\\`-separated (Windows) from path", () => {
    expect(migratedPath(String.raw`C:\Users\me\Docs\notes.txt`, String.raw`C:\Users\me\Docs`, String.raw`C:\Users\me\Archive`))
      .toBe(String.raw`C:\Users\me\Archive\notes.txt`);
  });

  it("leaves a mere-prefix sibling untouched — the /a/b vs /a/bc boundary", () => {
    expect(migratedPath("/a/bc", "/a/b", "/a/renamed")).toBeNull();
    expect(migratedPath("/a/bc.txt", "/a/b", "/a/renamed")).toBeNull();
    expect(migratedPath(String.raw`C:\Users\me\Docsy`, String.raw`C:\Users\me\Docs`, String.raw`C:\Users\me\Archive`))
      .toBeNull();
  });

  it("leaves an unrelated path untouched", () => {
    expect(migratedPath("/x/y", "/a/b", "/a/renamed")).toBeNull();
  });

  it("is a no-op when from === to, or from is empty", () => {
    expect(migratedPath("/a/b", "/a/b", "/a/b")).toBeNull();
    expect(migratedPath("/a/b", "", "/new")).toBeNull();
  });

  describe("CPE-1737 round 2: from/to already carrying a trailing separator", () => {
    // A remote directory's own path can now legitimately arrive here already slashed (CPE-1737 round
    // 1). Before stripping it, `underSlash`/`underBack` built a DOUBLED separator ("…/sub//") that no
    // real descendant path ever starts with — so a slashed folder's rename/move migrated the folder
    // itself (the exact-match branch, unaffected) but silently left every descendant behind, un-rekeyed.
    it("still re-keys a subtree entry when `from` carries a trailing slash", () => {
      expect(migratedPath("sftp://h/srv/sub/child.txt", "sftp://h/srv/sub/", "sftp://h/srv/renamed"))
        .toBe("sftp://h/srv/renamed/child.txt");
    });

    it("does not build a doubled separator in the OUTPUT when `to` also carries a trailing slash", () => {
      expect(migratedPath("sftp://h/srv/sub/child.txt", "sftp://h/srv/sub", "sftp://h/srv/renamed/"))
        .toBe("sftp://h/srv/renamed/child.txt");
    });

    it("still re-keys the exact match itself when `from` is slashed (unaffected branch, kept green)", () => {
      expect(migratedPath("sftp://h/srv/sub/", "sftp://h/srv/sub/", "sftp://h/srv/renamed")).toBe("sftp://h/srv/renamed");
    });
  });

  describe("CPE-1737 round 3: the exact-match check must agree with the subtree check about the trailing slash", () => {
    it("re-keys a store entry for the folder ITSELF, saved un-slashed, even when `from` arrives slashed", () => {
      // Round 2 only fixed the subtree (descendant) branch; the exact-match check still compared `p`
      // only against the raw `from`, so an un-slashed store entry for the renamed folder itself was
      // skipped whenever `from` showed up slashed — inconsistent with the subtree check two lines below
      // it, which already tolerated this via `fromBase`.
      expect(migratedPath("sftp://h/srv/sub", "sftp://h/srv/sub/", "sftp://h/srv/renamed")).toBe("sftp://h/srv/renamed");
    });

    it("the reverse pairing (`p` slashed, `from` bare) was already correct — the subtree branch's `startsWith` matches an empty remainder", () => {
      // `p` equals `fromBase + '/'` here, so the SUBTREE check below (not the exact-match check this
      // fix touches) already handles it, with an empty remainder after the prefix — hence the trailing
      // '/' in the output. Kept as a no-regression pin alongside the actual fix above.
      expect(migratedPath("sftp://h/srv/sub/", "sftp://h/srv/sub", "sftp://h/srv/renamed")).toBe("sftp://h/srv/renamed/");
    });
  });
});

describe("migratePathList — array re-keying for the three real stores (CPE-1224)", () => {
  it("migrates a favourites list on an exact-path rename", () => {
    const favorites: Favorite[] = [
      { path: "/proj", name: "proj", is_dir: true },
      { path: "/other", name: "other", is_dir: true },
    ];
    const next = migratePathList(favorites, "/proj", "/archive");
    expect(next).toEqual([
      { path: "/archive", name: "proj", is_dir: true },
      { path: "/other", name: "other", is_dir: true },
    ]);
  });

  it("migrates a favourites list on a move (different parent, same leaf name)", () => {
    const favorites: Favorite[] = [{ path: "/home/proj", name: "proj", is_dir: true }];
    const next = migratePathList(favorites, "/home/proj", "/work/proj");
    expect(next).toEqual([{ path: "/work/proj", name: "proj", is_dir: true }]);
  });

  it("migrates a recents list's subtree entry (a file inside a renamed folder)", () => {
    const recents: RecentFile[] = [
      { path: "/proj/notes.txt", name: "notes.txt", opened: 1000 },
      { path: "/proj/sub/b.txt", name: "b.txt", opened: 2000 },
      { path: "/elsewhere.txt", name: "elsewhere.txt", opened: 3000 },
    ];
    const next = migratePathList(recents, "/proj", "/archive");
    expect(next).toEqual([
      { path: "/archive/notes.txt", name: "notes.txt", opened: 1000 },
      { path: "/archive/sub/b.txt", name: "b.txt", opened: 2000 },
      { path: "/elsewhere.txt", name: "elsewhere.txt", opened: 3000 },
    ]);
  });

  it("respects the /a/b vs /a/bc sibling boundary in a recent-folders list", () => {
    const recentFolders: RecentFile[] = [
      { path: "/a/b", name: "b", opened: 1 },
      { path: "/a/bc", name: "bc", opened: 2 },
    ];
    const next = migratePathList(recentFolders, "/a/b", "/a/renamed");
    expect(next).toEqual([
      { path: "/a/renamed", name: "b", opened: 1 },
      { path: "/a/bc", name: "bc", opened: 2 }, // untouched — mere prefix, not a real subtree member
    ]);
  });

  it("migrates a spotlight-frecency (Visit) list, exact + subtree, preserving count/last_used_s", () => {
    const visits: Visit[] = [
      { path: "/proj", count: 5, last_used_s: 100 },
      { path: "/proj/deep/file.txt", count: 2, last_used_s: 200 },
      { path: "/projector", count: 9, last_used_s: 300 }, // false-prefix sibling
    ];
    const next = migratePathList(visits, "/proj", "/archive");
    expect(next).toEqual([
      { path: "/archive", count: 5, last_used_s: 100 },
      { path: "/archive/deep/file.txt", count: 2, last_used_s: 200 },
      { path: "/projector", count: 9, last_used_s: 300 },
    ]);
  });

  it("returns the same array reference when nothing in the list is affected", () => {
    const recents: RecentFile[] = [{ path: "/unrelated", name: "unrelated", opened: 1 }];
    const next = migratePathList(recents, "/proj", "/archive");
    expect(next).toBe(recents);
  });

  it("handles an empty list", () => {
    expect(migratePathList([], "/a", "/b")).toEqual([]);
  });
});
