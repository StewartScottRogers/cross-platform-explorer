import { describe, it, expect, vi } from "vitest";
import {
  watchPathsForScope,
  changedPathInScope,
  batchTouchesScope,
  TrailingDebounce,
  type SmartFolderScope,
} from "./smartFolderLiveRefresh";
import type { FolderWatchEvent } from "./folderWatch";

describe("watchPathsForScope (CPE-1230)", () => {
  it("is empty when nothing is open", () => {
    expect(watchPathsForScope(null)).toEqual([]);
  });

  it("watches the exact matched paths for a tag smart folder", () => {
    const scope: SmartFolderScope = { kind: "paths", paths: ["C:/a/one.pdf", "C:/b/two.pdf"] };
    expect(watchPathsForScope(scope)).toEqual(["C:/a/one.pdf", "C:/b/two.pdf"]);
  });

  it("watches the single captured root for a structured search", () => {
    const scope: SmartFolderScope = { kind: "root", root: "C:/Projects" };
    expect(watchPathsForScope(scope)).toEqual(["C:/Projects"]);
  });
});

describe("changedPathInScope (CPE-1230)", () => {
  it("null scope never matches", () => {
    expect(changedPathInScope("C:/a/one.pdf", null)).toBe(false);
  });

  describe("tag smart folder (paths scope)", () => {
    const scope: SmartFolderScope = { kind: "paths", paths: ["C:/a/One.pdf", "C:/b/two.pdf"] };

    it("matches one of the tracked paths exactly", () => {
      expect(changedPathInScope("C:/b/two.pdf", scope)).toBe(true);
    });

    it("matches case-insensitively and across separator style (Windows)", () => {
      expect(changedPathInScope("c:\\a\\one.pdf", scope)).toBe(true);
    });

    it("does not match an untracked path", () => {
      expect(changedPathInScope("C:/c/three.pdf", scope)).toBe(false);
    });

    it("does not match a path merely inside the same folder as a tracked file", () => {
      // Tag smart folders scope to individual tagged paths, not their containing folders.
      expect(changedPathInScope("C:/a/unrelated.pdf", scope)).toBe(false);
    });
  });

  describe("structured search (root scope)", () => {
    const scope: SmartFolderScope = { kind: "root", root: "C:/Projects" };

    it("matches the root itself", () => {
      expect(changedPathInScope("C:/Projects", scope)).toBe(true);
    });

    it("matches anything nested under the root, any depth", () => {
      expect(changedPathInScope("C:/Projects/sub/deep/file.txt", scope)).toBe(true);
    });

    it("matches case-insensitively and across separator style (Windows)", () => {
      expect(changedPathInScope("c:\\projects\\file.txt", scope)).toBe(true);
    });

    it("does not match a sibling folder that merely shares a prefix", () => {
      expect(changedPathInScope("C:/Projects2/file.txt", scope)).toBe(false);
    });

    it("does not match an unrelated path", () => {
      expect(changedPathInScope("C:/Other/file.txt", scope)).toBe(false);
    });
  });
});

describe("batchTouchesScope (CPE-1230)", () => {
  const scope: SmartFolderScope = { kind: "root", root: "C:/Projects" };

  it("true when any event in the batch is under scope", () => {
    const batch: FolderWatchEvent[] = [
      { path: "C:/Other/a.txt", kind: "modified" },
      { path: "C:/Projects/new.txt", kind: "created" },
    ];
    expect(batchTouchesScope(batch, scope)).toBe(true);
  });

  it("false when no event in the batch is relevant", () => {
    const batch: FolderWatchEvent[] = [{ path: "C:/Other/a.txt", kind: "modified" }];
    expect(batchTouchesScope(batch, scope)).toBe(false);
  });

  it("false for an empty batch or no open scope", () => {
    expect(batchTouchesScope([], scope)).toBe(false);
    expect(batchTouchesScope([{ path: "C:/Projects/new.txt", kind: "created" }], null)).toBe(false);
  });
});

describe("TrailingDebounce (CPE-1230)", () => {
  it("collapses a burst of schedule() calls into one run, after the quiet window", () => {
    vi.useFakeTimers();
    try {
      const run = vi.fn();
      const d = new TrailingDebounce(300);
      d.schedule(run);
      vi.advanceTimersByTime(100);
      d.schedule(run); // re-arms — the first timer must NOT fire
      vi.advanceTimersByTime(100);
      d.schedule(run); // re-arms again
      expect(run).not.toHaveBeenCalled();
      vi.advanceTimersByTime(300);
      expect(run).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("cancel() drops a pending timer without firing it", () => {
    vi.useFakeTimers();
    try {
      const run = vi.fn();
      const d = new TrailingDebounce(300);
      d.schedule(run);
      d.cancel();
      vi.advanceTimersByTime(1000);
      expect(run).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("cancel() on an already-idle debounce is a no-op", () => {
    const d = new TrailingDebounce(300);
    expect(() => d.cancel()).not.toThrow();
  });

  it("independent schedule() calls each run once their own window elapses", () => {
    vi.useFakeTimers();
    try {
      const run = vi.fn();
      const d = new TrailingDebounce(50);
      d.schedule(run);
      vi.advanceTimersByTime(50);
      expect(run).toHaveBeenCalledTimes(1);
      d.schedule(run);
      vi.advanceTimersByTime(50);
      expect(run).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });
});
