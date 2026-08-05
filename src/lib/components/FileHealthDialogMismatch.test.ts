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

// CPE-1322's per-row "rename to correct extension" fix-it action reuses the existing `move_exact`
// backend command (the same one BatchRenameDialog applies its plan through) — `moveExactImpl` resolves
// each call from a queue so a test can control ok/error per invocation instead of hard-coding one
// canned response.
let moveExactImpl: ((pairs: [string, string][]) => unknown) | null = null;
let moveExactCalls: [string, string][][] = [];

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
  if (cmd === "move_exact") {
    moveExactCalls.push(args.pairs);
    if (!moveExactImpl) throw new Error("unexpected move_exact call");
    return moveExactImpl(args.pairs);
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
  moveExactImpl = null;
  moveExactCalls = [];
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

  // CPE-1319 (Visual Critic defect 1): the full sentence used to render as a right-side PILL, which
  // squeezed the filename column down to an unreadable "f…" because pills don't shrink. It must now
  // render as a dim SUBTITLE line under the filename — the full sentence stays visible and the name
  // gets its own line, structurally separate from the reason text (not sharing a flex line with it).
  it("renders the mismatch reason as a subtitle UNDER the filename, not a right-competing pill", async () => {
    await openMismatchTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    emit(0, [
      { path: "/repo/photo.jpg", claimed_ext: "jpg", detected_label: "Windows executable/library", detected_ext: "exe" },
    ]);
    await finish(0, 1);

    const reason = await screen.findByTestId("fh-reason");
    // The full sentence is visible in full — never truncated to a short label.
    expect(reason.textContent).toContain("claims jpg");
    expect(reason.textContent).toContain("looks like Windows executable/library");
    // It's the dim ".subtitle" treatment, not the old right-competing ".reason" pill.
    expect(reason.className).toContain("subtitle");
    expect(reason.className).not.toContain("reason");

    // The filename lives on its own line — the subtitle isn't sharing a parent with it, and the
    // subtitle span itself doesn't wrap (contain) the filename.
    const name = screen.getByText("photo.jpg");
    expect(name.parentElement).not.toBe(reason.parentElement);
    expect(reason.contains(name)).toBe(false);
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

  // CPE-1322: per-row "Rename to .{detected extension}" fix-it action, reusing the existing `move_exact`
  // command (never a new backend command) — refuses to overwrite, so a name collision surfaces loudly.
  describe("rename-to-detected-extension fix-it action", () => {
    async function scanOneHit(hit: { path: string; claimed_ext: string; detected_label: string; detected_ext: string }) {
      const result = await openMismatchTab();
      await fireEvent.click(screen.getByTestId("fh-scan-btn"));
      emit(0, [hit]);
      await finish(0, 1);
      await waitFor(() => expect(screen.getByTestId("fh-row")).toBeTruthy());
      return result;
    }

    it("shows the fix button only when detected_ext is present and differs from claimed_ext", async () => {
      await scanOneHit({ path: "/repo/photo.jpg", claimed_ext: "jpg", detected_label: "Windows executable/library", detected_ext: "exe" });
      expect(screen.getByTestId("fh-fix-btn")).toBeTruthy();
    });

    it("hides the fix button when detected_ext is empty or matches claimed_ext (nothing to rename to)", async () => {
      await scanOneHit({ path: "/repo/photo.jpg", claimed_ext: "jpg", detected_label: "unrecognised", detected_ext: "" });
      expect(screen.queryByTestId("fh-fix-btn")).toBeNull();
    });

    it("calls move_exact with the source path and a target of the same dir + stem + detected extension, then removes the row on success", async () => {
      moveExactImpl = (pairs) => pairs.map(([, to]) => ({ path: to, ok: true, error: "" }));
      await scanOneHit({ path: "/repo/photo.jpg", claimed_ext: "jpg", detected_label: "Windows executable/library", detected_ext: "exe" });

      await fireEvent.click(screen.getByTestId("fh-fix-btn"));

      expect(moveExactCalls).toEqual([[["/repo/photo.jpg", "/repo/photo.exe"]]]);
      await waitFor(() => expect(screen.queryByTestId("fh-row")).toBeNull());
    });

    it("computes the target from the DETECTED extension even when the original name has multiple dots", async () => {
      moveExactImpl = (pairs) => pairs.map(([, to]) => ({ path: to, ok: true, error: "" }));
      await scanOneHit({ path: "/repo/archive.tar.gz", claimed_ext: "gz", detected_label: "PDF document", detected_ext: "pdf" });

      await fireEvent.click(screen.getByTestId("fh-fix-btn"));

      // Only the LAST extension segment is replaced — the stem keeps the rest of the name intact.
      expect(moveExactCalls).toEqual([[["/repo/archive.tar.gz", "/repo/archive.tar.pdf"]]]);
    });

    it("does NOT navigate/close the dialog when the fix button is clicked (stopPropagation from the row's reveal click)", async () => {
      moveExactImpl = (pairs) => pairs.map(([, to]) => ({ path: to, ok: true, error: "" }));
      const { component } = await scanOneHit({ path: "/repo/photo.jpg", claimed_ext: "jpg", detected_label: "Windows executable/library", detected_ext: "exe" });
      const navigated: string[] = [];
      let closed = false;
      component.$on("navigate", (e: CustomEvent<string>) => navigated.push(e.detail));
      component.$on("close", () => (closed = true));

      await fireEvent.click(screen.getByTestId("fh-fix-btn"));

      expect(navigated).toEqual([]);
      expect(closed).toBe(false);
    });

    it("surfaces a backend refusal (e.g. target already exists) inline and does NOT remove the row", async () => {
      moveExactImpl = (pairs) => pairs.map(([, to]) => ({ path: to, ok: false, error: `"${to.split("/").pop()}" already exists` }));
      await scanOneHit({ path: "/repo/photo.jpg", claimed_ext: "jpg", detected_label: "Windows executable/library", detected_ext: "exe" });

      await fireEvent.click(screen.getByTestId("fh-fix-btn"));

      await waitFor(() => expect(screen.getByTestId("fh-fix-error")).toBeTruthy());
      expect(screen.getByTestId("fh-fix-error").textContent).toContain("already exists");
      // The row is NOT removed on failure — never a silent no-op.
      expect(screen.getByTestId("fh-row")).toBeTruthy();
      expect(screen.getByText("photo.jpg")).toBeTruthy();
    });

    it("surfaces a rejected/thrown move_exact call inline and does NOT remove the row", async () => {
      moveExactImpl = () => {
        throw new Error("permission denied");
      };
      await scanOneHit({ path: "/repo/photo.jpg", claimed_ext: "jpg", detected_label: "Windows executable/library", detected_ext: "exe" });

      await fireEvent.click(screen.getByTestId("fh-fix-btn"));

      await waitFor(() => expect(screen.getByTestId("fh-fix-error")).toBeTruthy());
      expect(screen.getByTestId("fh-fix-error").textContent).toContain("permission denied");
      expect(screen.getByTestId("fh-row")).toBeTruthy();
    });
  });
});
