import { describe, it, expect, vi } from "vitest";
import { OscillationGuard, handleFolderBatch, undoPlan, type FolderWatchEvent, type WatchFire } from "./folderWatch";
import { addRule, type WatchRule } from "./watchRules";

describe("OscillationGuard (CPE-794)", () => {
  it("guards a path within the window and expires after it", () => {
    const g = new OscillationGuard(1000);
    g.guard("/a.txt", 0);
    expect(g.isGuarded("/a.txt", 500)).toBe(true);
    expect(g.isGuarded("/a.txt", 1000)).toBe(false);
    expect(g.isGuarded("/a.txt", 1200)).toBe(false);
    expect(g.isGuarded("/never.txt", 0)).toBe(false);
  });
});

describe("undoPlan (CPE-794)", () => {
  const fire = (over: Partial<WatchFire>): WatchFire => ({
    id: "f", rule: "R", source: "/dl/a.pdf", finalPath: "/dl/a.pdf", copies: [], summary: "", ...over,
  });
  it("moves the file back when it was relocated", () => {
    expect(undoPlan(fire({ finalPath: "/archive/a.pdf" }))).toEqual({
      moveBack: { from: "/archive/a.pdf", to: "/dl/a.pdf" },
      deletes: [],
    });
  });
  it("deletes copies and doesn't move when the file only was copied", () => {
    expect(undoPlan(fire({ copies: ["/backup/a.pdf"] }))).toEqual({ moveBack: null, deletes: ["/backup/a.pdf"] });
  });
});

describe("handleFolderBatch (CPE-794)", () => {
  const pdfRule: WatchRule = addRule([], "Archive PDFs", { kind: "ext", exts: ["pdf"] }, [
    { kind: "move", dest: "/archive" },
  ])[0];

  const deps = (over: Partial<Parameters<typeof handleFolderBatch>[3]> = {}) => ({
    now: () => 100,
    stat: vi.fn(async (path: string) => ({ name: path.split("/").pop()!, is_dir: false, size: 10, modified: 100 })),
    run: vi.fn(async (_p: string, actions: { kind: string; resolved: string }[]) =>
      actions.map((a) => ({ path: `${a.resolved}/moved.pdf`, ok: true, error: "" })),
    ),
    guard: new OscillationGuard(3000),
    ...over,
  });

  it("runs the matching rule and reports a reversible fire", async () => {
    const d = deps();
    const fires: WatchFire[] = [];
    await handleFolderBatch([{ path: "/dl/invoice.pdf", kind: "created" }], [pdfRule], (f) => fires.push(f), d);
    expect(d.run).toHaveBeenCalledWith("/dl/invoice.pdf", [{ kind: "move", resolved: "/archive" }]);
    expect(fires).toHaveLength(1);
    expect(fires[0].source).toBe("/dl/invoice.pdf");
    expect(fires[0].finalPath).toBe("/archive/moved.pdf"); // from the OpResult path
    expect(fires[0].copies).toEqual([]);
    expect(fires[0].summary).toBe("Archive PDFs: invoice.pdf → /archive");
  });

  it("records copies (not a move) for a copy rule", async () => {
    const copyRule = addRule([], "Backup", { kind: "ext", exts: ["pdf"] }, [{ kind: "copy", dest: "/backup" }])[0];
    const d = deps();
    const fires: WatchFire[] = [];
    await handleFolderBatch([{ path: "/dl/a.pdf", kind: "created" }], [copyRule], (f) => fires.push(f), d);
    expect(fires[0].finalPath).toBe("/dl/a.pdf"); // not moved
    expect(fires[0].copies).toEqual(["/backup/moved.pdf"]);
  });

  it("ignores non-create/modify events, folders, and non-matching files", async () => {
    const d = deps({ stat: vi.fn(async () => ({ name: "x", is_dir: true, size: 0, modified: 0 })) });
    const fires: WatchFire[] = [];
    await handleFolderBatch(
      [{ path: "/dl/a.pdf", kind: "removed" }, { path: "/dl/folder", kind: "created" }],
      [pdfRule], (f) => fires.push(f), d,
    );
    expect(d.run).not.toHaveBeenCalled();
    expect(fires).toEqual([]);
  });

  it("suppresses the executor's own echo via the oscillation guard", async () => {
    const g = new OscillationGuard(3000);
    const d = deps({ guard: g });
    const fires: WatchFire[] = [];
    const ev: FolderWatchEvent[] = [{ path: "/dl/report.pdf", kind: "created" }];
    await handleFolderBatch(ev, [pdfRule], (f) => fires.push(f), d);
    await handleFolderBatch(ev, [pdfRule], (f) => fires.push(f), d);
    expect(d.run).toHaveBeenCalledTimes(1);
    expect(fires).toHaveLength(1);
  });

  it("skips a rule whose only actions are non-fs (tag)", async () => {
    const tagRule = addRule([], "Tag it", { kind: "ext", exts: ["pdf"] }, [{ kind: "tag", tag: "inbox" }])[0];
    const d = deps();
    await handleFolderBatch([{ path: "/dl/x.pdf", kind: "created" }], [tagRule], () => {}, d);
    expect(d.run).not.toHaveBeenCalled();
  });
});
