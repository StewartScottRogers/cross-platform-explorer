// CPE-495: per-repo two-way-mirror sync policy — persistence + safe defaults + action labels.
import { describe, it, expect, beforeEach } from "vitest";
import { loadSyncPolicy, saveSyncPolicy, syncActionLabel } from "./syncPolicy";

beforeEach(() => localStorage.clear());

describe("sync policy persistence (CPE-495)", () => {
  it("defaults to the safe 'merge' policy for an unknown repo", () => {
    expect(loadSyncPolicy("/repo/a")).toBe("merge");
  });

  it("round-trips a saved policy per repo path", () => {
    saveSyncPolicy("/repo/a", "rebase");
    saveSyncPolicy("/repo/b", "manual");
    expect(loadSyncPolicy("/repo/a")).toBe("rebase");
    expect(loadSyncPolicy("/repo/b")).toBe("manual");
    expect(loadSyncPolicy("/repo/c")).toBe("merge"); // untouched repo still defaults
  });

  it("overwrites a repo's prior choice", () => {
    saveSyncPolicy("/repo/a", "rebase");
    saveSyncPolicy("/repo/a", "merge");
    expect(loadSyncPolicy("/repo/a")).toBe("merge");
  });

  it("falls back to 'merge' when the stored value is corrupt/unknown", () => {
    localStorage.setItem("cpe.syncPolicy", JSON.stringify({ "/repo/a": "force-push" }));
    expect(loadSyncPolicy("/repo/a")).toBe("merge");
  });

  it("survives non-JSON garbage in storage", () => {
    localStorage.setItem("cpe.syncPolicy", "not json");
    expect(loadSyncPolicy("/repo/a")).toBe("merge");
    expect(() => saveSyncPolicy("/repo/a", "rebase")).not.toThrow();
  });
});

describe("syncActionLabel (CPE-495)", () => {
  it("humanizes the backend plan action names", () => {
    expect(syncActionLabel("pull-ff")).toBe("Fast-forward pull");
    expect(syncActionLabel("pull-merge")).toBe("Pull (merge)");
    expect(syncActionLabel("pull-rebase")).toBe("Pull (rebase)");
    expect(syncActionLabel("push")).toBe("Push");
  });

  it("passes through an unrecognized action verbatim", () => {
    expect(syncActionLabel("something-new")).toBe("something-new");
  });
});
