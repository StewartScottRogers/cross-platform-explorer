import { describe, it, expect } from "vitest";
import { driveRoot, newlyConnected, jobsForConnect, anyAutoRun } from "./driveScheduler";
import type { BackupJob } from "./backup";

const job = (over: Partial<BackupJob>): BackupJob => ({
  id: "j", name: "J", source: "C:\\src", dest: "D:\\backup", ...over,
});

describe("driveRoot (CPE-797)", () => {
  it("extracts the Windows drive-letter root, upper-cased with a trailing backslash", () => {
    expect(driveRoot("D:\\Backups\\2026")).toBe("D:\\");
    expect(driveRoot("d:/backups")).toBe("D:\\");
    expect(driveRoot("E:\\")).toBe("E:\\");
  });
  it("falls back to / for POSIX and unrecognised paths", () => {
    expect(driveRoot("/mnt/backup")).toBe("/");
    expect(driveRoot("\\\\server\\share")).toBe("/");
    expect(driveRoot("relative\\path")).toBe("/");
  });
});

describe("newlyConnected (CPE-797)", () => {
  it("returns roots present now but not before, deduped and case-insensitive", () => {
    expect(newlyConnected(["C:\\"], ["C:\\", "D:\\"])).toEqual(["D:\\"]);
    expect(newlyConnected(["c:\\"], ["C:\\"])).toEqual([]); // same drive, different case
    expect(newlyConnected(["C:\\", "D:\\"], ["C:\\"])).toEqual([]); // removal is not a connect
  });
});

describe("jobsForConnect (CPE-797)", () => {
  it("runs only auto-run jobs whose dest drive just appeared", () => {
    const jobs = [
      job({ id: "a", dest: "D:\\backup", autoRun: true }),
      job({ id: "b", dest: "E:\\backup", autoRun: true }), // drive not connected
      job({ id: "c", dest: "D:\\other", autoRun: false }), // opted out
    ];
    const due = jobsForConnect(["C:\\"], ["C:\\", "D:\\"], jobs);
    expect(due.map((j) => j.id)).toEqual(["a"]);
  });

  it("does not re-run a job whose drive was already connected", () => {
    const jobs = [job({ id: "a", dest: "D:\\backup", autoRun: true })];
    expect(jobsForConnect(["C:\\", "D:\\"], ["C:\\", "D:\\"], jobs)).toEqual([]);
  });

  it("returns nothing when no drive appeared", () => {
    const jobs = [job({ autoRun: true })];
    expect(jobsForConnect(["C:\\"], ["C:\\"], jobs)).toEqual([]);
  });
});

describe("anyAutoRun (CPE-797)", () => {
  it("is true only when at least one job opts in", () => {
    expect(anyAutoRun([job({ autoRun: false }), job({ autoRun: true })])).toBe(true);
    expect(anyAutoRun([job({}), job({ autoRun: false })])).toBe(false);
    expect(anyAutoRun([])).toBe(false);
  });
});
