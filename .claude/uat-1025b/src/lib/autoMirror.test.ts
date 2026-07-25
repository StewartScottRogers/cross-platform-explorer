// CPE-497: scheduled auto-mirror — off-by-default persistence, due timing, and the unattended-safety
// filter (only ff-pull + push; a divergence/conflict pauses; never force-pushes).
import { describe, it, expect, beforeEach } from "vitest";
import {
  loadAutoMirror,
  saveAutoMirror,
  isDue,
  autoSyncActions,
  pausedReason,
  DEFAULT_INTERVAL_MIN,
} from "./autoMirror";

beforeEach(() => localStorage.clear());

describe("auto-mirror config (CPE-497)", () => {
  it("is OFF by default for an unconfigured repo", () => {
    const cfg = loadAutoMirror("/repo/a");
    expect(cfg.enabled).toBe(false);
    expect(cfg.intervalMinutes).toBe(DEFAULT_INTERVAL_MIN);
  });

  it("round-trips a per-repo toggle + interval", () => {
    saveAutoMirror("/repo/a", { enabled: true, intervalMinutes: 30 });
    expect(loadAutoMirror("/repo/a")).toEqual({ enabled: true, intervalMinutes: 30 });
    expect(loadAutoMirror("/repo/b").enabled).toBe(false); // other repos unaffected
  });

  it("repairs a bad stored interval to the default", () => {
    localStorage.setItem("cpe.autoMirror", JSON.stringify({ "/repo/a": { enabled: true, intervalMinutes: 0 } }));
    expect(loadAutoMirror("/repo/a").intervalMinutes).toBe(DEFAULT_INTERVAL_MIN);
  });
});

describe("isDue (CPE-497)", () => {
  it("is due immediately when never synced this session", () => {
    expect(isDue(null, 15, 1_000_000)).toBe(true);
  });
  it("is not due before the interval elapses, due after", () => {
    const now = 10_000_000;
    expect(isDue(now, 15, now + 14 * 60_000)).toBe(false);
    expect(isDue(now, 15, now + 15 * 60_000)).toBe(true);
  });
});

describe("autoSyncActions — unattended safety filter (CPE-497)", () => {
  it("runs a clean fast-forward pull + push", () => {
    expect(autoSyncActions({ actions: ["pull-ff", "push"] })).toEqual(["pull-ff", "push"]);
  });

  it("NEVER auto-reconciles a divergence (merge/rebase pull)", () => {
    expect(autoSyncActions({ actions: ["pull-merge", "push"] })).toEqual([]);
    expect(autoSyncActions({ actions: ["pull-rebase"] })).toEqual([]);
  });

  it("pauses when a conflict is possible or the plan is blocked", () => {
    expect(autoSyncActions({ actions: ["pull-ff"], conflicts_possible: true })).toEqual([]);
    expect(autoSyncActions({ actions: ["push"], blocked: "diverged" })).toEqual([]);
  });

  it("withholds a ff-pull into a dirty tree but still pushes committed work", () => {
    expect(autoSyncActions({ actions: ["pull-ff", "push"], dirty: true })).toEqual(["push"]);
  });

  it("only ever emits pull-ff / push — never a force or unknown action", () => {
    const out = autoSyncActions({ actions: ["pull-ff", "push", "force-push", "weird"] as string[] });
    expect(out).toEqual(["pull-ff", "push"]);
  });
});

describe("pausedReason (CPE-497)", () => {
  it("explains a blocked or diverged plan, and is null for a clean one", () => {
    expect(pausedReason({ blocked: "manual policy" })).toBe("manual policy");
    expect(pausedReason({ actions: ["pull-rebase"] })).toMatch(/diverged/i);
    expect(pausedReason({ actions: ["pull-ff", "push"] })).toBeNull();
  });
});
