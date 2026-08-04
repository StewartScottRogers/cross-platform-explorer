import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import FileHealthDialog from "./FileHealthDialog.svelte";

// The Type-mismatch tab (CPE-1316, slice 2) streams `MismatchHit` batches over an `onHit` Channel, then
// returns the terminal `{ scanned, truncated }` — the EXACT same shape as the dangling-links tab
// (CPE-1315), just with its own command names (`find_type_mismatches_stream` /
// `cancel_type_mismatches_stream`) and its own generation counter (`mismatchGen`), so this suite mirrors
// FileHealthDialog.test.ts's dangling-tab coverage 1:1 (batch-append, loading-flip, cancel-on-rescan,
// late-batch-drop, navigate+close, error, empty-clears-loading) while also proving the mismatch scan's
// generation/cancel wiring is independent of the dangling tab's (CPE-1316's cross-scan-bug requirement).
type Pending = {
  channel: { onmessage: ((b: unknown) => void) | null };
  streamId: number;
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
};
let pending: Pending[] = [];
let cancelCalls: number[] = [];
let danglingCancelCalls: number[] = [];

const invoke = vi.fn(async (cmd: string, args?: any) => {
  if (cmd === "find_type_mismatches_stream") {
    return await new Promise((resolve, reject) =>
      pending.push({ channel: args.onHit, streamId: args.streamId, resolve, reject }),
    );
  }
  if (cmd === "cancel_type_mismatches_stream") {
    cancelCalls.push(args.streamId);
    return null;
  }
  // The dangling tab's stream/cancel commands must never be touched by mismatch-tab activity — captured
  // separately so a cross-scan bug (wrong cancel command, shared counter) shows up as a failed assertion.
  if (cmd === "find_dangling_links_stream") {
    return await new Promise(() => {}); // never resolves — dangling tab is untouched in this suite
  }
  if (cmd === "cancel_dangling_links_stream") {
    danglingCancelCalls.push(args.streamId);
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

/** Emit one streamed batch of mismatch hits on the Nth stream call's channel. */
function emit(
  callIndex: number,
  hits: Array<{ path: string; claimed_ext: string; detected_label: string; detected_ext: string }>,
) {
  pending[callIndex].channel.onmessage?.(hits);
}
/** Resolve the Nth stream call with its terminal walk stats. */
async function finish(callIndex: number, scanned: number, truncated = false) {
  pending[callIndex].resolve({ hits: [], scanned, truncated });
  await Promise.resolve();
}
/** Reject the Nth stream call — a plain (non-Error) rejection, mirroring a Tauri command's Err(String). */
async function fail(callIndex: number, err: unknown) {
  pending[callIndex].reject(err);
  await Promise.resolve();
}

async function openMismatchTab() {
  const result = render(FileHealthDialog, { root: "/repo" });
  await fireEvent.click(screen.getByTestId("fh-tab-mismatch"));
  return result;
}

beforeEach(() => {
  invoke.mockClear();
  pending = [];
  cancelCalls = [];
  danglingCancelCalls = [];
});

describe("FileHealthDialog — Type mismatch tab (CPE-1316)", () => {
  it("does not scan until Scan is clicked, then calls find_type_mismatches_stream with root+excludes+streamId", async () => {
    await openMismatchTab();
    expect(invoke).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    expect(invoke).toHaveBeenCalledWith(
      "find_type_mismatches_stream",
      expect.objectContaining({ root: "/repo", excludes: [], streamId: 1 }),
    );
  });

  it("flips loading off after the first batch and APPENDS (not replaces) subsequent batches", async () => {
    await openMismatchTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));

    expect(screen.queryByTestId("fh-row")).toBeNull();

    emit(0, [{ path: "/repo/photo.jpg", claimed_ext: "jpg", detected_label: "Windows executable/library", detected_ext: "exe" }]);
    await waitFor(() => expect(screen.getAllByTestId("fh-row").length).toBe(1));
    expect(screen.getByText("photo.jpg")).toBeTruthy();

    emit(0, [{ path: "/repo/notes.txt", claimed_ext: "txt", detected_label: "PDF document", detected_ext: "pdf" }]);
    await waitFor(() => expect(screen.getAllByTestId("fh-row").length).toBe(2));
    expect(screen.getByText("photo.jpg")).toBeTruthy(); // first batch's row survived
    expect(screen.getByText("notes.txt")).toBeTruthy();

    await finish(0, 20);
  });

  it("renders the claims-X-looks-like-Y badge per hit", async () => {
    await openMismatchTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    emit(0, [
      { path: "/repo/photo.jpg", claimed_ext: "jpg", detected_label: "Windows executable/library", detected_ext: "exe" },
    ]);
    await finish(0, 1);

    await waitFor(() => expect(screen.getAllByTestId("fh-reason").length).toBe(1));
    expect(screen.getByTestId("fh-reason").textContent).toContain("jpg");
    expect(screen.getByTestId("fh-reason").textContent).toContain("Windows executable/library");
  });

  it("clears loading on an EMPTY result even though no batch is streamed", async () => {
    await openMismatchTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    await finish(0, 9);
    await waitFor(() => expect(screen.getByTestId("fh-none")).toBeTruthy());
    expect(screen.getByText(/No type mismatches found/)).toBeTruthy();
  });

  it("surfaces a backend error instead of hanging in the loading state", async () => {
    await openMismatchTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    await fail(0, "not a folder");
    await waitFor(() => expect(screen.getByText("not a folder")).toBeTruthy());
    expect(screen.getByTestId("fh-rescan-btn")).toBeTruthy();
    expect(screen.queryByText(/Scanning/)).toBeNull();
  });

  it("row click dispatches navigate with the hit's path and closes", async () => {
    const { component } = await openMismatchTab();
    const navigated: string[] = [];
    let closed = false;
    component.$on("navigate", (e: CustomEvent<string>) => navigated.push(e.detail));
    component.$on("close", () => (closed = true));

    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    emit(0, [{ path: "/repo/disguised.jpg", claimed_ext: "jpg", detected_label: "Windows executable/library", detected_ext: "exe" }]);
    await waitFor(() => expect(screen.getByTestId("fh-row")).toBeTruthy());
    await fireEvent.click(screen.getByTestId("fh-row"));

    expect(navigated).toEqual(["/repo/disguised.jpg"]);
    expect(closed).toBe(true);
  });

  it("rescanning cancels the PRIOR mismatch stream (by its streamId) and supersedes its late batches — never touches the dangling tab's cancel command", async () => {
    await openMismatchTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    emit(0, [{ path: "/repo/old.jpg", claimed_ext: "jpg", detected_label: "old-type", detected_ext: "bin" }]);
    await finish(0, 1);
    await waitFor(() => expect(screen.getByText("old.jpg")).toBeTruthy());
    expect(cancelCalls).toEqual([]); // no cancel on the very first scan

    await fireEvent.click(screen.getByTestId("fh-rescan-btn"));
    expect(cancelCalls).toEqual([1]);
    expect(invoke).toHaveBeenCalledWith(
      "find_type_mismatches_stream",
      expect.objectContaining({ streamId: 2 }),
    );

    emit(0, [{ path: "/repo/old.jpg", claimed_ext: "jpg", detected_label: "old-type", detected_ext: "bin" }]);
    emit(1, [{ path: "/repo/new.jpg", claimed_ext: "jpg", detected_label: "new-type", detected_ext: "bin" }]);
    await finish(1, 1);

    await waitFor(() => expect(screen.getByText("new.jpg")).toBeTruthy());
    expect(screen.queryByText("old.jpg")).toBeNull(); // stale batch never rendered
    expect(screen.getAllByTestId("fh-row").length).toBe(1);
    // The mismatch scan's own cancel command was used with the mismatch generation counter — the
    // dangling tab's cancel command/counter must never be invoked by mismatch-tab rescans.
    expect(danglingCancelCalls).toEqual([]);
  });
});
