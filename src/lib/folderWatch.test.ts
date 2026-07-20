import { describe, it, expect, vi } from "vitest";
import { OscillationGuard, handleFolderBatch, type FolderWatchEvent } from "./folderWatch";
import { addRule, type WatchRule } from "./watchRules";

describe("OscillationGuard (CPE-794)", () => {
  it("guards a path within the window and expires after it", () => {
    const g = new OscillationGuard(1000);
    g.guard("/a.txt", 0);
    expect(g.isGuarded("/a.txt", 500)).toBe(true);
    expect(g.isGuarded("/a.txt", 1000)).toBe(false); // expired (prunes)
    expect(g.isGuarded("/a.txt", 1200)).toBe(false);
    expect(g.isGuarded("/never.txt", 0)).toBe(false);
  });
});

describe("handleFolderBatch (CPE-794)", () => {
  const pdfRule: WatchRule = addRule([], "Archive PDFs", { kind: "ext", exts: ["pdf"] }, [
    { kind: "move", dest: "/archive" },
  ])[0];

  const deps = (over: Partial<Parameters<typeof handleFolderBatch>[3]> = {}) => ({
    now: () => 100,
    stat: vi.fn(async (path: string) => ({ name: path.split("/").pop()!, is_dir: false, size: 10, modified: 100 })),
    run: vi.fn(async () => []),
    guard: new OscillationGuard(3000),
    ...over,
  });

  it("runs the matching rule's fs actions on a created file", async () => {
    const d = deps();
    const fired: string[] = [];
    await handleFolderBatch([{ path: "/dl/invoice.pdf", kind: "created" }], [pdfRule], (m) => fired.push(m), d);
    expect(d.run).toHaveBeenCalledWith("/dl/invoice.pdf", [{ kind: "move", resolved: "/archive" }]);
    expect(fired).toEqual(["Archive PDFs: invoice.pdf → /archive"]);
  });

  it("ignores non-create/modify events, folders, and non-matching files", async () => {
    const d = deps({ stat: vi.fn(async () => ({ name: "x", is_dir: true, size: 0, modified: 0 })) });
    const fired: string[] = [];
    await handleFolderBatch(
      [
        { path: "/dl/a.pdf", kind: "removed" }, // wrong kind
        { path: "/dl/folder", kind: "created" }, // is_dir
      ],
      [pdfRule],
      (m) => fired.push(m),
      d,
    );
    // removed → never stats; folder → stats but skipped as dir
    expect(d.run).not.toHaveBeenCalled();
    expect(fired).toEqual([]);
  });

  it("suppresses the executor's own echo via the oscillation guard", async () => {
    const g = new OscillationGuard(3000);
    const d = deps({ guard: g });
    const fired: string[] = [];
    const ev: FolderWatchEvent[] = [{ path: "/dl/report.pdf", kind: "created" }];
    await handleFolderBatch(ev, [pdfRule], (m) => fired.push(m), d);
    // same path fires again (as the move's echo) → guarded, not re-run
    await handleFolderBatch(ev, [pdfRule], (m) => fired.push(m), d);
    expect(d.run).toHaveBeenCalledTimes(1);
    expect(fired).toHaveLength(1);
  });

  it("skips a rule whose only actions are non-fs (tag)", async () => {
    const tagRule: WatchRule = addRule([], "Tag it", { kind: "ext", exts: ["pdf"] }, [
      { kind: "tag", tag: "inbox" },
    ])[0];
    const d = deps();
    await handleFolderBatch([{ path: "/dl/x.pdf", kind: "created" }], [tagRule], () => {}, d);
    expect(d.run).not.toHaveBeenCalled(); // no fs actions to run
  });
});
