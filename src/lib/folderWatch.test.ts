import { describe, it, expect, vi, afterEach } from "vitest";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { OscillationGuard, handleFolderBatch, undoFire, undoPlan, type FolderWatchEvent, type WatchFire } from "./folderWatch";
import { addRule, type WatchRule } from "./watchRules";

// CPE-1666 — `undoFire` re-stats each recorded delete right before acting, on a fake `commands` module
// whose `entryInfo`/`deletePermanent` are backed by the REAL filesystem (mirroring
// `delete_permanent_impl`'s own `is_dir()` -> `remove_dir_all` dispatch in src-tauri/src/lib.rs). That
// lets the test drive `undoFire` exactly as production code does and then check survival by listing the
// real directory back off disk — asserting a return value would miss the exact bug this ticket exists to
// catch (nothing about `deletePermanent`'s resolved value says whether a whole tree got wiped).
vi.mock("./bindings.gen", () => ({
  commands: {
    entryInfo: vi.fn(async (p: string) => {
      try {
        const st = fs.statSync(p);
        return { status: "ok", data: { name: path.basename(p), path: p, is_dir: st.isDirectory(), size: st.size, modified: null, created: null, readonly: false, hidden: false } };
      } catch (e) {
        return { status: "error", error: String(e) };
      }
    }),
    deletePermanent: vi.fn(async (paths: string[], _confirmed: boolean) => {
      const data = paths.map((p) => {
        try {
          // Mirrors delete_permanent_impl's own dispatch: is_dir() -> remove_dir_all (recursive),
          // else remove_file — including the recursive branch that CPE-1666 is about not reaching
          // for a swapped-in directory.
          if (fs.existsSync(p) && fs.statSync(p).isDirectory()) fs.rmSync(p, { recursive: true, force: true });
          else fs.rmSync(p, { force: true });
          return { path: p, ok: true, error: "" };
        } catch (e) {
          return { path: p, ok: false, error: String(e) };
        }
      });
      return { status: "ok", data };
    }),
    moveExact: vi.fn(async () => [{ path: "", ok: true, error: "" }]),
  },
}));

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

  // CPE-1312: `runWatchActions` returns one `OpResult` per action, and a real op can fail (disk full,
  // permission denied, ...). The executor must only treat a fired/undoable op as the ones where
  // `OpResult.ok === true` — a failed op must never be recorded as fired, must not appear in the fire's
  // `finalPath`/`copies`, and undo must not touch its (never-written) result path.
  describe("failed OpResult handling (CPE-1312)", () => {
    const moveCopyRule = addRule([], "Archive + Backup", { kind: "ext", exts: ["pdf"] }, [
      { kind: "move", dest: "/archive" },
      { kind: "copy", dest: "/backup" },
    ])[0];

    it("only records the successful action when one action in the plan fails", async () => {
      const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
      const d = deps({
        // Mixed result: the move lands, the copy fails (e.g. disk full).
        run: vi.fn(async () => [
          { path: "/archive/invoice.pdf", ok: true, error: "" },
          { path: "/backup/invoice.pdf", ok: false, error: "disk full" },
        ]),
      });
      const fires: WatchFire[] = [];
      await handleFolderBatch([{ path: "/dl/invoice.pdf", kind: "created" }], [moveCopyRule], (f) => fires.push(f), d);

      expect(fires).toHaveLength(1);
      // The successful move is recorded and undoable...
      expect(fires[0].finalPath).toBe("/archive/invoice.pdf");
      // ...but the failed copy must NOT be recorded as a copy to clean up on undo.
      expect(fires[0].copies).toEqual([]);
      // The failure must be surfaced, not swallowed.
      expect(warn).toHaveBeenCalled();
      expect(warn.mock.calls[0][0]).toContain("disk full");

      // Undo must only act on what actually landed: move the file back, no deletes (nothing was copied).
      const plan = undoPlan(fires[0]);
      expect(plan).toEqual({ moveBack: { from: "/archive/invoice.pdf", to: "/dl/invoice.pdf" }, deletes: [] });

      warn.mockRestore();
    });

    it("fires nothing and guards nothing when every action fails", async () => {
      const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
      const g = new OscillationGuard(3000);
      const d = deps({
        guard: g,
        run: vi.fn(async () => [
          { path: "/archive/invoice.pdf", ok: false, error: "permission denied" },
          { path: "/backup/invoice.pdf", ok: false, error: "permission denied" },
        ]),
      });
      const fires: WatchFire[] = [];
      await handleFolderBatch([{ path: "/dl/invoice.pdf", kind: "created" }], [moveCopyRule], (f) => fires.push(f), d);

      expect(fires).toEqual([]);
      expect(warn).toHaveBeenCalledTimes(2);
      // Nothing landed at the (failed) result paths, so they must not be guarded as the executor's echo.
      expect(g.isGuarded("/archive/invoice.pdf", 100)).toBe(false);
      expect(g.isGuarded("/backup/invoice.pdf", 100)).toBe(false);

      warn.mockRestore();
    });
  });
});

describe("undoFire (CPE-1666) — re-stats a recorded delete before acting", () => {
  const tmpRoots: string[] = [];
  function makeTmp(): string {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "cpe-undo-"));
    tmpRoots.push(dir);
    return dir;
  }
  afterEach(() => {
    for (const dir of tmpRoots.splice(0)) {
      try {
        fs.rmSync(dir, { recursive: true, force: true });
      } catch {
        // best-effort cleanup
      }
    }
    vi.clearAllMocks();
  });

  const fire = (over: Partial<WatchFire>): WatchFire => ({
    id: "f", rule: "R", source: "/dl/a.pdf", finalPath: "/dl/a.pdf", copies: [], summary: "", ...over,
  });

  it(
    "skips a copy path swapped for a real directory since fire time, and the swapped-in tree survives " +
      "— verified by listing it back off disk",
    async () => {
      const root = makeTmp();
      const copyPath = path.join(root, "backup", "invoice.pdf");
      fs.mkdirSync(path.dirname(copyPath), { recursive: true });
      fs.writeFileSync(copyPath, "app-made copy"); // the fire's actual copy, as recorded at fire time

      // Attacker (or anything else with write access), sometime between fire and Undo: delete the
      // recorded copy and swap a real directory tree into its exact path — same attack the PR #844
      // auditor used to reach delete_permanent_impl's remove_dir_all branch.
      fs.rmSync(copyPath);
      fs.mkdirSync(copyPath);
      fs.writeFileSync(path.join(copyPath, "keep-me.txt"), "important data");
      fs.mkdirSync(path.join(copyPath, "nested"));
      fs.writeFileSync(path.join(copyPath, "nested", "also-keep-me.txt"), "more important data");

      const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
      await undoFire(fire({ copies: [copyPath] }));
      expect(warn).toHaveBeenCalled();
      warn.mockRestore();

      // The assertion CPE-1666 requires: list the directory back OFF DISK — not a return value — and
      // confirm the swapped-in tree is completely intact.
      expect(fs.existsSync(copyPath)).toBe(true);
      expect(fs.statSync(copyPath).isDirectory()).toBe(true);
      expect(fs.readdirSync(copyPath).sort()).toEqual(["keep-me.txt", "nested"]);
      expect(fs.readFileSync(path.join(copyPath, "keep-me.txt"), "utf8")).toBe("important data");
      expect(fs.existsSync(path.join(copyPath, "nested", "also-keep-me.txt"))).toBe(true);
      expect(fs.readFileSync(path.join(copyPath, "nested", "also-keep-me.txt"), "utf8")).toBe(
        "more important data",
      );
    },
  );

  it("a normal undo still deletes the app-created copy — verified by listing the directory off disk", async () => {
    const root = makeTmp();
    const copyPath = path.join(root, "backup", "invoice.pdf");
    fs.mkdirSync(path.dirname(copyPath), { recursive: true });
    fs.writeFileSync(copyPath, "app-made copy");

    await undoFire(fire({ copies: [copyPath] }));

    expect(fs.existsSync(copyPath)).toBe(false);
    expect(fs.readdirSync(path.dirname(copyPath))).toEqual([]);
  });

  it("a normal undo still moves the source back when the fire relocated it", async () => {
    const root = makeTmp();
    const finalPath = path.join(root, "archive", "invoice.pdf");
    fs.mkdirSync(path.dirname(finalPath), { recursive: true });
    fs.writeFileSync(finalPath, "the moved file");

    await undoFire(fire({ source: path.join(root, "dl", "invoice.pdf"), finalPath, copies: [] }));

    // moveExact itself is mocked (no real backend to move it for real in this unit test) — what's under
    // test here is that undoFire still calls it for the move leg, unaffected by the CPE-1666 re-stat
    // gate, which only applies to `plan.deletes`.
    const bindings = await import("./bindings.gen");
    expect(bindings.commands.moveExact).toHaveBeenCalledWith([[finalPath, path.join(root, "dl", "invoice.pdf")]]);
  });
});
