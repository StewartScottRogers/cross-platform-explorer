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
  const plan = { copy: ["a.txt"], update: ["b.txt"], delete: ["stale.txt"] };
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
      verify: true,
      confirmed: true,
    });
  });
});
