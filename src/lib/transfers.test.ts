import { describe, it, expect } from "vitest";
import { upsertProgress, markFinished, dismiss, percent, type TransferState, type TransferProgress, type TransferReport } from "./transfers";

const prog = (id: number, done: number, total: number): TransferProgress => ({
  id, total_bytes: total, done_bytes: done, total_items: 1, done_items: 0, current: "x",
});

describe("transfers reducer (CPE-622)", () => {
  it("appends a new transfer then updates it in place", () => {
    let l: TransferState[] = [];
    l = upsertProgress(l, prog(1, 0, 100));
    expect(l).toHaveLength(1);
    l = upsertProgress(l, prog(1, 50, 100));
    expect(l).toHaveLength(1);
    expect(l[0].done_bytes).toBe(50);
    l = upsertProgress(l, prog(2, 0, 200));
    expect(l.map((t) => t.id)).toEqual([1, 2]);
  });

  it("marks a transfer finished with its report and clears its current file", () => {
    let l = upsertProgress([], prog(1, 50, 100));
    const r: TransferReport = { id: 1, transferred: 1, skipped: 0, failed: 0, cancelled: false, errors: [] };
    l = markFinished(l, r);
    expect(l[0].finished).toBe(true);
    expect(l[0].current).toBe("");
    expect(l[0].report).toEqual(r);
  });

  it("keeps the report across a late progress event and drops on dismiss", () => {
    const r: TransferReport = { id: 1, transferred: 1, skipped: 0, failed: 0, cancelled: false, errors: [] };
    let l = markFinished(upsertProgress([], prog(1, 100, 100)), r);
    l = upsertProgress(l, prog(1, 100, 100)); // a stray late event must not wipe the report
    expect(l[0].report).toEqual(r);
    expect(dismiss(l, 1)).toHaveLength(0);
  });

  it("computes percent by bytes, falling back to items, and 100 when finished", () => {
    expect(percent({ ...prog(1, 25, 100), finished: false })).toBe(25);
    expect(percent({ ...prog(1, 0, 0), total_items: 4, done_items: 1, finished: false })).toBe(25);
    expect(percent({ ...prog(1, 3, 100), finished: true })).toBe(100);
    expect(percent({ ...prog(1, 0, 0), finished: false })).toBe(0);
  });
});
