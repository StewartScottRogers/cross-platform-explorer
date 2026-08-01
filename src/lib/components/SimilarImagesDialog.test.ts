import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import SimilarImagesDialog from "./SimilarImagesDialog.svelte";

// The dialog streams near-duplicate image groups (CPE-1202) over an `onGroup` Channel, then returns the
// terminal `{ files_scanned, truncated }` — mirroring DuplicatesDialog. Image clustering is a whole-set
// op, so on a NON-empty result the (single) batch arrives on the channel; on an EMPTY result NO batch is
// ever sent, and `loading` must clear on the awaited resolution instead (the reviewer's critical note).
//
// This mock gives the test full control: `find_similar_images_stream` captures the channel and returns a
// promise the test resolves by hand, so batch emission and stream completion can be driven independently
// (needed to exercise the generation-token supersede where a stale in-flight scan's late batch is dropped).
type Pending = { channel: { onmessage: ((b: unknown) => void) | null }; resolve: (v: unknown) => void };
let pending: Pending[] = [];
let trashCalls: string[][] = [];
let checkpointCalls: Array<[string, string]> = [];

const invoke = vi.fn(async (cmd: string, args?: any) => {
  if (cmd === "find_similar_images_stream") {
    return await new Promise((resolve) => pending.push({ channel: args.onGroup, resolve }));
  }
  if (cmd === "delete_to_trash") {
    trashCalls.push(args.paths);
    return [];
  }
  if (cmd === "checkpoint_create") {
    checkpointCalls.push([args.root, args.label]);
    return { status: "ok", data: { id: "cp1", label: args.label } };
  }
  if (cmd === "thumbnail") return ""; // ThumbnailImage falls back to its Icon — irrelevant here
  return null;
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

/** Emit one streamed batch of groups on the Nth stream call's channel. */
function emit(callIndex: number, groups: Array<{ paths: string[] }>) {
  pending[callIndex].channel.onmessage?.(groups);
}
/** Resolve the Nth stream call with its terminal walk stats. */
async function finish(callIndex: number, files_scanned: number, truncated = false) {
  pending[callIndex].resolve({ files_scanned, truncated });
  await Promise.resolve();
}

beforeEach(() => {
  invoke.mockClear();
  pending = [];
  trashCalls = [];
  checkpointCalls = [];
});

describe("SimilarImagesDialog (CPE-1202)", () => {
  it("streams a group of similar images and appends batches (no auto-scan)", async () => {
    render(SimilarImagesDialog, { root: "/repo" });
    expect(invoke).not.toHaveBeenCalled(); // explicit scan, never automatic

    await fireEvent.click(screen.getByText("Scan for similar images"));
    expect(invoke).toHaveBeenCalledWith("find_similar_images_stream", expect.objectContaining({ root: "/repo" }));

    // First batch: one group of two near-duplicate images → both render, loading clears.
    emit(0, [{ paths: ["/repo/a.png", "/repo/sub/b.png"] }]);
    await waitFor(() => expect(screen.getByText(/group of similar images/)).toBeTruthy());
    expect(screen.getByText("a.png")).toBeTruthy();
    expect(screen.getByText("b.png")).toBeTruthy();

    // A second appended batch grows the list (streaming append) rather than replacing it.
    emit(0, [{ paths: ["/repo/c.png", "/repo/d.png"] }]);
    await waitFor(() => expect(screen.getByText(/2 groups of similar images/)).toBeTruthy());
    await finish(0, 4);
  });

  it("clears loading on an EMPTY result even though no batch is streamed", async () => {
    render(SimilarImagesDialog, { root: "/repo" });
    await fireEvent.click(screen.getByText("Scan for similar images"));
    // No emit() at all — the empty case sends no batch. Only the terminal resolution can clear loading.
    await finish(0, 7);
    await waitFor(() => expect(screen.getByTestId("sim-none")).toBeTruthy());
    expect(screen.getByText(/No similar images found/)).toBeTruthy();
  });

  it("supersedes a stale in-flight scan: a late batch from the old generation is dropped", async () => {
    render(SimilarImagesDialog, { root: "/repo" });
    await fireEvent.click(screen.getByText("Scan for similar images"));
    emit(0, [{ paths: ["/repo/old1.png", "/repo/old2.png"] }]);
    await finish(0, 2);
    await waitFor(() => expect(screen.getByText("old1.png")).toBeTruthy());

    // Re-scan (generation 2 in flight): resets the view, first scan is now stale.
    await fireEvent.click(screen.getByTestId("sim-rescan-btn"));
    // A LATE batch from the superseded first scan must be ignored.
    emit(0, [{ paths: ["/repo/old1.png", "/repo/old2.png"] }]);
    // The fresh scan's batch is what renders.
    emit(1, [{ paths: ["/repo/new1.png", "/repo/new2.png"] }]);
    await finish(1, 2);

    await waitFor(() => expect(screen.getByText("new1.png")).toBeTruthy());
    expect(screen.queryByText("old1.png")).toBeNull(); // stale batch never rendered
    expect(screen.getByText(/1 group of similar images/)).toBeTruthy();
  });

  it("SAFETY: Move to Bin is disabled whenever the selection would delete every copy of a group", async () => {
    render(SimilarImagesDialog, { root: "/repo" });
    await fireEvent.click(screen.getByText("Scan for similar images"));
    emit(0, [{ paths: ["/repo/a.png", "/repo/b.png"] }]);
    await finish(0, 2);
    await waitFor(() => expect(screen.getByTestId("sim-group")).toBeTruthy());

    const moveBtn = screen.getByTestId("sim-move-btn") as HTMLButtonElement;
    // Default selects NOTHING → disabled.
    expect(moveBtn.disabled).toBe(true);

    const boxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    // Select the FIRST image only → one copy still survives → enabled.
    await fireEvent.click(boxes[0]);
    await waitFor(() => expect(moveBtn.disabled).toBe(false));
    // Select the SECOND too → now the whole group would be wiped → disabled by the keeper guard.
    await fireEvent.click(boxes[1]);
    await waitFor(() => expect(moveBtn.disabled).toBe(true));
  });

  it("SAFETY: cleanup trashes ONLY the redundant (non-kept) paths, after a checkpoint", async () => {
    render(SimilarImagesDialog, { root: "/repo" });
    await fireEvent.click(screen.getByText("Scan for similar images"));
    emit(0, [{ paths: ["/repo/a.png", "/repo/sub/b.png"] }]);
    await finish(0, 2);
    await waitFor(() => expect(screen.getByTestId("sim-group")).toBeTruthy());

    // "Select extras" keeps the first, marks only the second copy.
    await fireEvent.click(screen.getByText("Select extras"));
    await fireEvent.click(await screen.findByText("Move 1 to Recycle Bin"));

    await waitFor(() => expect(trashCalls).toEqual([["/repo/sub/b.png"]]));
    // Recoverable Bin only — never a permanent delete.
    expect(invoke).not.toHaveBeenCalledWith("delete_permanently", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("delete", expect.anything());
    // A checkpoint was taken first, scoped to the folder.
    expect(checkpointCalls).toEqual([["/repo", "Before removing similar images"]]);
    // The group dropped to one copy → no longer a near-duplicate → the list empties.
    await waitFor(() => expect(screen.getByTestId("sim-none")).toBeTruthy());
  });
});
