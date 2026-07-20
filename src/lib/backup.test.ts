import { describe, it, expect } from "vitest";
import { planBackup, addJob, removeJob, updateJob, parseJobs, serializeJobs, type BackupJob } from "./backup";
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
