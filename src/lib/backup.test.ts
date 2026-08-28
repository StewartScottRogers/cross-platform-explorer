import { describe, it, expect } from "vitest";
import { planBackup, addJob, removeJob, updateJob, parseJobs, serializeJobs, unattendedBackupConsent, unattendedBackupArgs, type BackupJob } from "./backup";
import type { CompareNode } from "./treeDiff";

const f = (name: string, size = 0, modified = 0): CompareNode => ({ name, isDir: false, size, modified });
const d = (name: string, children: CompareNode[] = []): CompareNode => ({ name, isDir: true, children });

describe("planBackup (CPE-796)", () => {
  it("copies new, updates changed, skips identical", () => {
    const source = [f("same.txt", 1, 1), f("edited.txt", 2, 2), f("new.txt", 3, 3)];
    const dest = [f("same.txt", 1, 1), f("edited.txt", 1, 1)];
    const plan = planBackup(source, dest);
    expect(plan.copy).toEqual(["new.txt"]);
    expect(plan.update).toEqual(["edited.txt"]);
    expect(plan.unchanged).toBe(1);
    expect(plan.delete).toEqual([]); // not mirror
  });

  it("deletes dest-only files only in mirror mode", () => {
    const source = [f("keep.txt", 1, 1)];
    const dest = [f("keep.txt", 1, 1), f("stale.txt", 9, 9)];
    expect(planBackup(source, dest, false).delete).toEqual([]);
    expect(planBackup(source, dest, true).delete).toEqual(["stale.txt"]);
  });

  it("recurses into subdirectories with relative paths", () => {
    const source = [d("sub", [f("a.txt", 1, 1), f("b.txt", 2, 2)])];
    const dest = [d("sub", [f("a.txt", 1, 1)])];
    const plan = planBackup(source, dest);
    expect(plan.copy).toEqual(["sub/b.txt"]);
    expect(plan.unchanged).toBe(1);
  });

  it("copies an entire new subtree", () => {
    const plan = planBackup([d("newdir", [f("x", 1, 1), f("y", 2, 2)])], []);
    expect(plan.copy.sort()).toEqual(["newdir/x", "newdir/y"]);
  });
});

describe("planBackup carries empty directories (CPE-1925)", () => {
  // Before this ticket the plan model had no directory entry at all: a source folder with no files
  // under it produced NOTHING, the run reported a clean `ok` for every file it did carry, and the
  // folder was simply absent afterwards. Measured end to end on `main` over this exact shape — real
  // `scan_tree`, real `planBackup`, real `apply_backup_plan` — `ok=3 fail=0`, 5 of 5 folders missing
  // on disk, in the backup direction AND the restore direction.

  it("plans an empty source directory that no file copy would create", () => {
    const plan = planBackup([f("top.txt", 1, 1), d("logs")], []);
    expect(plan.createDirs).toEqual(["logs"]);
    expect(plan.copy).toEqual(["top.txt"]);
  });

  it("plans the deepest path only — creating a/b/c creates a and a/b on the way", () => {
    const plan = planBackup([d("a", [d("b", [d("c")])])], []);
    expect(plan.createDirs).toEqual(["a/b/c"]);
  });

  it("handles the ticket's awkward case: a directory whose only content is another empty directory", () => {
    const plan = planBackup([d("outer", [d("inner"), f("side.txt", 1, 1)])], []);
    // `outer` is created by the copy of `outer/side.txt`; only `outer/inner` needs an entry.
    expect(plan.copy).toEqual(["outer/side.txt"]);
    expect(plan.createDirs).toEqual(["outer/inner"]);
  });

  it("does NOT plan a directory a file copy already creates", () => {
    const plan = planBackup([d("withfiles", [f("x", 1, 1)])], []);
    expect(plan.createDirs).toEqual([]);
    expect(plan.copy).toEqual(["withfiles/x"]);
  });

  it("does NOT plan a directory the destination already has", () => {
    const tree = [d("logs"), f("a.txt", 1, 1)];
    const plan = planBackup(tree, tree);
    expect(plan.createDirs).toEqual([]);
    expect(plan.unchanged).toBe(1);
  });

  it("plans an empty directory missing from a destination that has the rest", () => {
    const plan = planBackup([f("a.txt", 1, 1), d("logs")], [f("a.txt", 1, 1)]);
    expect(plan.createDirs).toEqual(["logs"]);
    expect(plan.copy).toEqual([]); // nothing to copy; the whole run is the folder
  });

  it("plans the directory side of a file-to-directory type change, rather than dropping the name", () => {
    // `diffTrees` emits this as `changed` with no children, so today the source subtree vanishes
    // silently. An entry the engine will refuse (a file is standing in the way) is at least reported.
    const plan = planBackup([d("x")], [f("x", 1, 1)]);
    expect(plan.createDirs).toEqual(["x"]);
  });
});

describe("planBackup tells an empty directory from an unseen one (CPE-1925)", () => {
  // `scan_tree` reports a directory's children short for SEVEN different reasons and only one of them
  // means "empty" — the enumeration lives on `TreeNode` in `crates/server/src/compare.rs`. Creating a
  // directory in the destination because the source one LOOKED childless would be asserting something
  // the scan never established. Note `unreadable` does not imply the children list is EMPTY: case 5 (an
  // entry that failed to read among others that did not) sets it on a PARTIAL list, covered below.
  const unreadable = (name: string, children: CompareNode[] = []): CompareNode => ({ name, isDir: true, children, unreadable: true });
  const truncated = (name: string, children: CompareNode[] = []): CompareNode => ({ name, isDir: true, children, truncated: true });

  it("creates the one it could read", () => {
    expect(planBackup([d("really-empty")], []).createDirs).toEqual(["really-empty"]);
  });

  it("refuses to create one it could not read, and says so", () => {
    const plan = planBackup([unreadable("locked")], []);
    expect(plan.createDirs).toEqual([]);
    expect(plan.skippedDirs).toEqual([{ path: "locked", reason: "unreadable" }]);
  });

  it("refuses to create one the depth cap stopped at, and says so", () => {
    const plan = planBackup([truncated("very/deep")], []);
    expect(plan.createDirs).toEqual([]);
    expect(plan.skippedDirs).toEqual([{ path: "very/deep", reason: "depth-limit" }]);
  });

  it("is silent about nothing — an ordinary plan reports an empty skip list", () => {
    expect(planBackup([f("a.txt", 1, 1)], []).skippedDirs).toEqual([]);
  });

  it("never mirror-deletes the destination's copies under a source directory it could not read", () => {
    // The destructive consequence of the same ambiguity, and the reason this is not cosmetic: an
    // unreadable source directory comes back with NO children, so every file the destination holds
    // under it diffs as "removed" and a mirror run would delete the very copies it exists to protect.
    const dest = [d("locked", [f("precious.txt", 5, 5), f("also.txt", 6, 6)])];
    const withAccess = planBackup([d("locked", [])], dest, true);
    expect(withAccess.delete.sort()).toEqual(["locked/also.txt", "locked/precious.txt"]); // genuinely gone
    const withoutAccess = planBackup([unreadable("locked")], dest, true);
    expect(withoutAccess.delete).toEqual([]); // unknown, so untouched
    expect(withoutAccess.skippedDirs).toEqual([{ path: "locked", reason: "unreadable" }]);
  });

  // CPE-1925 round 2, case 5 — the PARTIAL listing, and the worst-behaved of the seven because it is
  // the only one whose `children` is non-empty. `read_dir` succeeded, some entries were read and some
  // errored, and the result looks like an entirely ordinary directory that happens to hold two files.
  // The files that never made the list diff as "removed from the source", which in a mirror run means
  // *delete the backup's only copy of them*.
  //
  // There is no portable filesystem fixture for it — you cannot make `readdir` fail an entry on demand
  // — so it is pinned HERE, at the layer where the damage would happen, on the exact node shape
  // `scan_children` emits for it. The Rust side covers the flag-setting arm via case 4, which shares
  // the same three lines of code.
  //
  // Red-proof, run: putting the round-1 line back (`walk(...); if (inDest) materialised = true;`, which
  // discards whether a copy under the partial directory materialises it) reds the third test below with
  // `expected [ 'outer' ] to deeply equal []` — a `createDirs` entry for a folder a real file copy was
  // already going to create. It was the only red of the 28.
  describe("a PARTIAL listing (case 5) is treated as unknown, not as the whole truth", () => {
    const partly = (name: string, children: CompareNode[]): CompareNode =>
      ({ name, isDir: true, children, unreadable: true });

    it("copies the files it did see", () => {
      const plan = planBackup([partly("half", [f("seen.txt", 1, 1)])], []);
      expect(plan.copy).toEqual(["half/seen.txt"]);
    });

    it("does NOT delete the destination's copies of the files it did not see", () => {
      const dest = [d("half", [f("seen.txt", 1, 1), f("unseen.txt", 9, 9)])];
      const source = [partly("half", [f("seen.txt", 1, 1)])];
      const plan = planBackup(source, dest, true);
      expect(plan.delete).toEqual([]); // `half/unseen.txt` is unknown, not gone
      expect(plan.skippedDirs).toEqual([{ path: "half", reason: "unreadable" }]);
      // The control: with a listing the scan CAN vouch for, the same shape does delete it.
      expect(planBackup([d("half", [f("seen.txt", 1, 1)])], dest, true).delete).toEqual(["half/unseen.txt"]);
    });

    it("does not create the directory in its own right, even though a copy below it will", () => {
      // `half` reaches the destination as a side effect of copying `half/seen.txt` — which is honest,
      // because that is a real file. What it must not get is a `createDirs` entry asserting the folder
      // is deliberately as the scan found it. And the enclosing folder must not be double-listed.
      const plan = planBackup([d("outer", [partly("half", [f("seen.txt", 1, 1)])])], []);
      expect(plan.createDirs).toEqual([]);
      expect(plan.copy).toEqual(["outer/half/seen.txt"]);
      expect(plan.skippedDirs).toEqual([{ path: "outer/half", reason: "unreadable" }]);
    });
  });
});

describe("BackupJob CRUD + parse (CPE-796)", () => {
  it("adds/updates/removes immutably", () => {
    let list = addJob([], "Photos", "/pics", "E:/backup", true);
    expect(list[0].id).toMatch(/^bj_/);
    expect(list[0].mirror).toBe(true);
    const id = list[0].id;
    expect(updateJob(list, id, { name: "Pics" })[0].name).toBe("Pics");
    expect(list[0].name).toBe("Photos"); // original untouched
    expect(removeJob(list, id)).toEqual([]);
  });

  it("parse tolerates malformed input; serialize round-trips", () => {
    const list: BackupJob[] = [{ id: "a", name: "n", source: "/s", dest: "/d", mirror: false }];
    expect(parseJobs(serializeJobs(list))).toEqual(list);
    expect(parseJobs(null)).toEqual([]);
    expect(parseJobs("nope")).toEqual([]);
    expect(parseJobs(JSON.stringify([{ id: "x" }, list[0]]))).toEqual([list[0]]);
  });
});

describe("unattendedBackupConsent (CPE-1664)", () => {
  // The backend refuses a backup plan without consent, and a mirror plan deletes files under the
  // destination outright. For a run nobody is watching, the ticked "auto-run on connect" box is the
  // only honest source of that consent — so it must be READ, never assumed.
  it("grants consent only for a job the user actually ticked auto-run for", () => {
    expect(unattendedBackupConsent({ autoRun: true })).toBe(true);
  });

  it("withholds consent when auto-run is off or was never set", () => {
    expect(unattendedBackupConsent({ autoRun: false })).toBe(false);
    expect(unattendedBackupConsent({})).toBe(false);
    expect(unattendedBackupConsent({ autoRun: undefined })).toBe(false);
  });

  it("requires the flag to be exactly true — a truthy non-boolean is not consent", () => {
    // `parseJobs` never type-checks `autoRun`, so a hand-edited or migrated settings blob can carry a
    // string here. `!!job.autoRun` would read "no" as consent to delete.
    expect(unattendedBackupConsent({ autoRun: "no" as unknown as boolean })).toBe(false);
    expect(unattendedBackupConsent({ autoRun: 1 as unknown as boolean })).toBe(false);
  });

  it("is a real read of the flag, not a constant — the two answers differ", () => {
    // Guards against the decision being inlined back to a hard-coded `true`: if it were, both of these
    // would agree and this assertion would fail.
    expect(unattendedBackupConsent({ autoRun: true })).not.toBe(unattendedBackupConsent({ autoRun: false }));
  });
});

describe("unattendedBackupArgs (CPE-1664)", () => {
  // THE pin for the unattended consent decision. It lives here, not in an App-level test, because
  // `driveScheduler` only ever delivers `autoRun: true` jobs — so nothing driving the real scheduler can
  // distinguish the decision from a constant. This function can be called with an unticked job directly.
  const plan = { copy: ["a.txt"], update: ["b.txt"], delete: ["stale.txt"], createDirs: ["logs"] };
  const job = (autoRun?: boolean) => ({ source: "C:\\pics", dest: "D:\\backup", autoRun });

  it("carries consent for a job the user ticked auto-run for", () => {
    expect(unattendedBackupArgs(job(true), plan).confirmed).toBe(true);
  });

  it("WITHHOLDS consent for an unticked job — the case the scheduler can never reach", () => {
    // If `confirmed` were hard-coded `true`, this is the assertion that catches it.
    expect(unattendedBackupArgs(job(false), plan).confirmed).toBe(false);
    expect(unattendedBackupArgs(job(undefined), plan).confirmed).toBe(false);
  });

  it("passes the plan through unchanged, so consent is the only thing it decides", () => {
    const args = unattendedBackupArgs(job(true), plan);
    expect(args).toEqual({
      sourceRoot: "C:\\pics",
      destRoot: "D:\\backup",
      copy: ["a.txt"],
      update: ["b.txt"],
      deletePaths: ["stale.txt"], // renamed for the backend's argument name
      // CPE-1925: the unattended run is the one nobody is watching, so the directory entries have to
      // reach the backend from here too — a scheduled job that silently reshaped the tree would go
      // unnoticed for as long as the backup went unread.
      createDirs: ["logs"],
      verify: true,
      confirmed: true,
    });
  });
});
