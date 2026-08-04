import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import FileHealthDialog from "./FileHealthDialog.svelte";

// The Orphan-sidecars tab (CPE-1316, slice 2) streams plain path-string batches over an `onOrphan`
// Channel, then returns the terminal `{ scanned, truncated }` — same shape as the other two tabs, minus
// any per-row metadata (an orphan row is just a path). Mirrors FileHealthDialog.test.ts's dangling-tab
// coverage (batch-append, loading-flip, cancel-on-rescan, late-batch-drop, navigate+close, error,
// empty-clears-loading), and proves `recursive: true` is always passed plus that this scan's
// generation/cancel wiring (`orphanGen` / `cancel_orphan_sidecars_stream`) is independent of the other
// two tabs' (CPE-1316's cross-scan-bug requirement).
type Pending = {
  channel: { onmessage: ((b: unknown) => void) | null };
  streamId: number;
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
};
let pending: Pending[] = [];
let cancelCalls: number[] = [];
let otherCancelCalls: string[] = [];

const invoke = vi.fn(async (cmd: string, args?: any) => {
  if (cmd === "find_orphan_sidecars_stream") {
    return await new Promise((resolve, reject) =>
      pending.push({ channel: args.onOrphan, streamId: args.streamId, resolve, reject }),
    );
  }
  if (cmd === "cancel_orphan_sidecars_stream") {
    cancelCalls.push(args.streamId);
    return null;
  }
  // Neither of the other two scans' commands should ever be touched by orphan-tab activity.
  if (cmd === "find_dangling_links_stream" || cmd === "find_type_mismatches_stream") {
    return await new Promise(() => {}); // never resolves in this suite
  }
  if (cmd === "cancel_dangling_links_stream" || cmd === "cancel_type_mismatches_stream") {
    otherCancelCalls.push(cmd);
    return null;
  }
  return null;
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

/** Emit one streamed batch of orphan paths on the Nth stream call's channel. */
function emit(callIndex: number, paths: string[]) {
  pending[callIndex].channel.onmessage?.(paths);
}
/** Resolve the Nth stream call with its terminal walk stats. */
async function finish(callIndex: number, scanned: number, truncated = false) {
  pending[callIndex].resolve({ orphans: [], scanned, truncated });
  await Promise.resolve();
}
/** Reject the Nth stream call — a plain (non-Error) rejection, mirroring a Tauri command's Err(String). */
async function fail(callIndex: number, err: unknown) {
  pending[callIndex].reject(err);
  await Promise.resolve();
}

async function openOrphanTab() {
  const result = render(FileHealthDialog, { root: "/repo" });
  await fireEvent.click(screen.getByTestId("fh-tab-orphan"));
  return result;
}

beforeEach(() => {
  invoke.mockClear();
  pending = [];
  cancelCalls = [];
  otherCancelCalls = [];
});

describe("FileHealthDialog — Orphan sidecars tab (CPE-1316)", () => {
  it("does not scan until Scan is clicked, then calls find_orphan_sidecars_stream with root+recursive:true+excludes+streamId", async () => {
    await openOrphanTab();
    expect(invoke).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    expect(invoke).toHaveBeenCalledWith(
      "find_orphan_sidecars_stream",
      expect.objectContaining({ root: "/repo", recursive: true, excludes: [], streamId: 1 }),
    );
  });

  it("flips loading off after the first batch and APPENDS (not replaces) subsequent batches", async () => {
    await openOrphanTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));

    expect(screen.queryByTestId("fh-row")).toBeNull();

    emit(0, ["/repo/movie.srt"]);
    await waitFor(() => expect(screen.getAllByTestId("fh-row").length).toBe(1));
    expect(screen.getByText("movie.srt")).toBeTruthy();

    emit(0, ["/repo/photo.xmp"]);
    await waitFor(() => expect(screen.getAllByTestId("fh-row").length).toBe(2));
    expect(screen.getByText("movie.srt")).toBeTruthy(); // first batch's row survived
    expect(screen.getByText("photo.xmp")).toBeTruthy();

    await finish(0, 20);
  });

  it("clears loading on an EMPTY result even though no batch is streamed", async () => {
    await openOrphanTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    await finish(0, 9);
    await waitFor(() => expect(screen.getByTestId("fh-none")).toBeTruthy());
    expect(screen.getByText(/No orphan sidecars found/)).toBeTruthy();
  });

  it("surfaces a backend error instead of hanging in the loading state", async () => {
    await openOrphanTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    await fail(0, "not a folder");
    await waitFor(() => expect(screen.getByText("not a folder")).toBeTruthy());
    expect(screen.getByTestId("fh-rescan-btn")).toBeTruthy();
    expect(screen.queryByText(/Scanning/)).toBeNull();
  });

  it("row click dispatches navigate with the orphan's path and closes", async () => {
    const { component } = await openOrphanTab();
    const navigated: string[] = [];
    let closed = false;
    component.$on("navigate", (e: CustomEvent<string>) => navigated.push(e.detail));
    component.$on("close", () => (closed = true));

    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    emit(0, ["/repo/orphan.srt"]);
    await waitFor(() => expect(screen.getByTestId("fh-row")).toBeTruthy());
    await fireEvent.click(screen.getByTestId("fh-row"));

    expect(navigated).toEqual(["/repo/orphan.srt"]);
    expect(closed).toBe(true);
  });

  it("rescanning cancels the PRIOR orphan stream (by its streamId) and supersedes its late batches — never touches the other tabs' cancel commands", async () => {
    await openOrphanTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    emit(0, ["/repo/old.srt"]);
    await finish(0, 1);
    await waitFor(() => expect(screen.getByText("old.srt")).toBeTruthy());
    expect(cancelCalls).toEqual([]); // no cancel on the very first scan

    await fireEvent.click(screen.getByTestId("fh-rescan-btn"));
    expect(cancelCalls).toEqual([1]);
    expect(invoke).toHaveBeenCalledWith(
      "find_orphan_sidecars_stream",
      expect.objectContaining({ streamId: 2 }),
    );

    emit(0, ["/repo/old.srt"]);
    emit(1, ["/repo/new.srt"]);
    await finish(1, 1);

    await waitFor(() => expect(screen.getByText("new.srt")).toBeTruthy());
    expect(screen.queryByText("old.srt")).toBeNull(); // stale batch never rendered
    expect(screen.getAllByTestId("fh-row").length).toBe(1);
    expect(otherCancelCalls).toEqual([]);
  });
});
