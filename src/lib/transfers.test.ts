import { describe, it, expect, vi, beforeEach } from "vitest";
import { upsertProgress, markFinished, dismiss, percent, collidingNames, startTransfer, type TransferState, type TransferProgress, type TransferReport } from "./transfers";

const { startTransferMock } = vi.hoisted(() => ({ startTransferMock: vi.fn() }));
vi.mock("./bindings.gen", () => ({ commands: { startTransfer: startTransferMock } }));

const prog = (id: number, done: number, total: number): TransferProgress => ({
  id, op: "copy", total_bytes: total, done_bytes: done, total_items: 1, done_items: 0, current: "x",
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
    const r: TransferReport = { id: 1, op: "copy", transferred: 1, skipped: 0, failed: 0, cancelled: false, errors: [] };
    l = markFinished(l, r);
    expect(l[0].finished).toBe(true);
    expect(l[0].current).toBe("");
    expect(l[0].report).toEqual(r);
  });

  it("keeps the report across a late progress event and drops on dismiss", () => {
    const r: TransferReport = { id: 1, op: "copy", transferred: 1, skipped: 0, failed: 0, cancelled: false, errors: [] };
    let l = markFinished(upsertProgress([], prog(1, 100, 100)), r);
    l = upsertProgress(l, prog(1, 100, 100)); // a stray late event must not wipe the report
    expect(l[0].report).toEqual(r);
    expect(dismiss(l, 1)).toHaveLength(0);
  });

  it("carries an archive compress/extract op through the reducer alongside copy/move (CPE-1184)", () => {
    const compressProg: TransferProgress = { id: 5, op: "compress", total_bytes: 100, done_bytes: 40, total_items: 4, done_items: 2, current: "sub/b.txt" };
    let l = upsertProgress([], compressProg);
    expect(l[0].op).toBe("compress");
    expect(percent(l[0])).toBe(40);

    // A later progress tick for the same id keeps its op (the reducer never overwrites it from the
    // incoming payload's own `op`, but the backend always resends the same op for a given id anyway).
    l = upsertProgress(l, { ...compressProg, done_bytes: 80, done_items: 3 });
    expect(l[0].op).toBe("compress");
    expect(percent(l[0])).toBe(80);

    const extractReport: TransferReport = { id: 6, op: "extract", transferred: 3, skipped: 0, failed: 0, cancelled: false, errors: [] };
    l = upsertProgress(l, { id: 6, op: "extract", total_bytes: 0, done_bytes: 0, total_items: 3, done_items: 1, current: "c.txt" });
    l = markFinished(l, extractReport);
    const extractRow = l.find((t) => t.id === 6)!;
    expect(extractRow.finished).toBe(true);
    expect(extractRow.report?.op).toBe("extract");

    // A cancelled compress/extract is a normal report shape too — nothing archive-specific breaks it.
    const cancelled: TransferReport = { id: 5, op: "compress", transferred: 3, skipped: 0, failed: 0, cancelled: true, errors: [] };
    l = markFinished(l, cancelled);
    expect(l.find((t) => t.id === 5)!.report?.cancelled).toBe(true);
  });

  it("finds base-name collisions against the destination (CPE-624)", () => {
    const existing = ["a.txt", "sub", "keep.md"];
    expect(collidingNames(["C:\\x\\a.txt", "C:\\x\\new.txt", "/y/sub"], existing)).toEqual(["a.txt", "sub"]);
    expect(collidingNames(["/y/none.txt"], existing)).toEqual([]);
    // Trailing slash on a folder source is stripped before matching.
    expect(collidingNames(["/y/sub/"], existing)).toEqual(["sub"]);
  });

  it("computes percent by bytes, falling back to items, and 100 when finished", () => {
    expect(percent({ ...prog(1, 25, 100), finished: false })).toBe(25);
    expect(percent({ ...prog(1, 0, 0), total_items: 4, done_items: 1, finished: false })).toBe(25);
    expect(percent({ ...prog(1, 3, 100), finished: true })).toBe(100);
    expect(percent({ ...prog(1, 0, 0), finished: false })).toBe(0);
  });
});

describe("startTransfer overwrite consent (CPE-1662)", () => {
  beforeEach(() => {
    startTransferMock.mockReset();
    startTransferMock.mockResolvedValue({ status: "ok", data: 7 });
  });

  it("defaults `confirmed` to false, so intent (the policy) is never its own consent", async () => {
    await startTransfer(["/a"], "/dest", "copy", "overwrite");
    expect(startTransferMock).toHaveBeenCalledWith(["/a"], "/dest", "copy", "overwrite", false);
  });

  it("forwards consent as its own argument when the conflict dialog gave it", async () => {
    await expect(startTransfer(["/a"], "/dest", "copy", "overwrite", true)).resolves.toBe(7);
    expect(startTransferMock).toHaveBeenCalledWith(["/a"], "/dest", "copy", "overwrite", true);
  });

  it("leaves the non-destructive policies alone — they pass false and are unaffected", async () => {
    await startTransfer(["/a"], "/dest", "copy", "keepboth");
    await startTransfer(["/a"], "/dest", "move", "skip");
    expect(startTransferMock).toHaveBeenNthCalledWith(1, ["/a"], "/dest", "copy", "keepboth", false);
    expect(startTransferMock).toHaveBeenNthCalledWith(2, ["/a"], "/dest", "move", "skip", false);
  });

  it("surfaces the backend refusal as a rejection, so the caller can show it in a notice", async () => {
    startTransferMock.mockResolvedValue({ status: "error", error: "`confirmed` was not set" });
    await expect(startTransfer(["/a"], "/dest", "copy", "overwrite")).rejects.toBeTruthy();
  });
});
