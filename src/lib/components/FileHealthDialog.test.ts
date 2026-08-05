import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import FileHealthDialog from "./FileHealthDialog.svelte";

// The dialog streams dangling/cyclic link batches (CPE-1315) over an `onLink` Channel, then returns the
// terminal `{ scanned, truncated }` — mirroring SimilarImagesDialog (CPE-1202) exactly, incl. the
// EMPTY-result-sends-no-batch edge case. `find_dangling_links_stream` is the FIRST frontend consumer of
// this `_stream` shape with a frontend-supplied `streamId` + a paired `cancel_dangling_links_stream`, so
// this suite also proves the cancel-on-rescan wiring (mirrors ExplorerPane's list_dir_stream/
// cancel_dir_stream convention, CPE-665/CPE-1299).
//
// This mock gives the test full control: `find_dangling_links_stream` captures the channel + streamId
// and returns a promise the test resolves by hand, so batch emission and stream completion can be driven
// independently (needed to exercise the generation-token supersede + the cancel call).
type Pending = {
  channel: { onmessage: ((b: unknown) => void) | null };
  streamId: number;
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
};
let pending: Pending[] = [];
let cancelCalls: number[] = [];

const invoke = vi.fn(async (cmd: string, args?: any) => {
  if (cmd === "find_dangling_links_stream") {
    return await new Promise((resolve, reject) =>
      pending.push({ channel: args.onLink, streamId: args.streamId, resolve, reject }),
    );
  }
  if (cmd === "cancel_dangling_links_stream") {
    cancelCalls.push(args.streamId);
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

/** Emit one streamed batch of links on the Nth stream call's channel. */
function emit(callIndex: number, links: Array<{ path: string; reason: "Missing" | "Cyclic" }>) {
  pending[callIndex].channel.onmessage?.(links);
}
/** Resolve the Nth stream call with its terminal walk stats. */
async function finish(callIndex: number, scanned: number, truncated = false) {
  pending[callIndex].resolve({ links: [], scanned, truncated });
  await Promise.resolve();
}
/** Reject the Nth stream call — a plain (non-Error) rejection, mirroring a Tauri command's Err(String). */
async function fail(callIndex: number, err: unknown) {
  pending[callIndex].reject(err);
  await Promise.resolve();
}

beforeEach(() => {
  invoke.mockClear();
  pending = [];
  cancelCalls = [];
});

describe("FileHealthDialog — Dangling links tab (CPE-1315)", () => {
  it("does not scan until Scan is clicked, then calls find_dangling_links_stream with root+excludes+streamId", async () => {
    render(FileHealthDialog, { root: "/repo" });
    expect(invoke).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    expect(invoke).toHaveBeenCalledWith(
      "find_dangling_links_stream",
      expect.objectContaining({ root: "/repo", excludes: [], streamId: 1 }),
    );
  });

  it("flips loading off after the first batch and APPENDS (not replaces) subsequent batches", async () => {
    render(FileHealthDialog, { root: "/repo" });
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));

    // Still loading — no rows yet.
    expect(screen.queryByTestId("fh-row")).toBeNull();

    emit(0, [{ path: "/repo/a-link.txt", reason: "Missing" }]);
    await waitFor(() => expect(screen.getAllByTestId("fh-row").length).toBe(1));
    expect(screen.getByText("a-link.txt")).toBeTruthy();

    // A second appended batch grows the list rather than replacing it.
    emit(0, [{ path: "/repo/b-link.txt", reason: "Cyclic" }]);
    await waitFor(() => expect(screen.getAllByTestId("fh-row").length).toBe(2));
    expect(screen.getByText("a-link.txt")).toBeTruthy(); // first batch's row survived
    expect(screen.getByText("b-link.txt")).toBeTruthy();

    await finish(0, 20);
  });

  it("renders the reason badge per link (Missing vs Cyclic)", async () => {
    render(FileHealthDialog, { root: "/repo" });
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    emit(0, [
      { path: "/repo/missing-target.txt", reason: "Missing" },
      { path: "/repo/loop.txt", reason: "Cyclic" },
    ]);
    await finish(0, 2);

    await waitFor(() => expect(screen.getAllByTestId("fh-reason").length).toBe(2));
    const badges = screen.getAllByTestId("fh-reason").map((b) => b.textContent);
    expect(badges).toContain("Missing target");
    expect(badges).toContain("Cyclic link");
  });

  it("clears loading on an EMPTY result even though no batch is streamed", async () => {
    render(FileHealthDialog, { root: "/repo" });
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    // No emit() at all — the empty case sends no batch. Only the terminal resolution can clear loading.
    await finish(0, 9);
    await waitFor(() => expect(screen.getByTestId("fh-none")).toBeTruthy());
    expect(screen.getByText(/No dangling links found/)).toBeTruthy();
  });

  it("surfaces a backend error instead of hanging in the loading state", async () => {
    render(FileHealthDialog, { root: "/repo" });
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    await fail(0, "not a folder");
    await waitFor(() => expect(screen.getByText("not a folder")).toBeTruthy());
    expect(screen.getByTestId("fh-rescan-btn")).toBeTruthy();
    expect(screen.queryByText(/Scanning/)).toBeNull(); // no stuck spinner
  });

  it("row click dispatches navigate with the link's path and closes", async () => {
    const { component } = render(FileHealthDialog, { root: "/repo" });
    const navigated: string[] = [];
    let closed = false;
    component.$on("navigate", (e: CustomEvent<string>) => navigated.push(e.detail));
    component.$on("close", () => (closed = true));

    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    emit(0, [{ path: "/repo/dead-link.txt", reason: "Missing" }]);
    await waitFor(() => expect(screen.getByTestId("fh-row")).toBeTruthy());
    await fireEvent.click(screen.getByTestId("fh-row"));

    expect(navigated).toEqual(["/repo/dead-link.txt"]);
    expect(closed).toBe(true);
  });

  it("rescanning cancels the PRIOR stream (by its streamId) and supersedes its late batches", async () => {
    render(FileHealthDialog, { root: "/repo" });
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    emit(0, [{ path: "/repo/old.txt", reason: "Missing" }]);
    await finish(0, 1);
    await waitFor(() => expect(screen.getByText("old.txt")).toBeTruthy());
    expect(cancelCalls).toEqual([]); // no cancel on the very first scan

    // Rescan: generation 2 starts, and the PRIOR generation's streamId (1) is cancelled.
    await fireEvent.click(screen.getByTestId("fh-rescan-btn"));
    expect(cancelCalls).toEqual([1]);
    expect(invoke).toHaveBeenCalledWith(
      "find_dangling_links_stream",
      expect.objectContaining({ streamId: 2 }),
    );

    // A LATE batch from the superseded first (cancelled) stream must be ignored.
    emit(0, [{ path: "/repo/old.txt", reason: "Missing" }]);
    // The fresh scan's batch is what renders.
    emit(1, [{ path: "/repo/new.txt", reason: "Cyclic" }]);
    await finish(1, 1);

    await waitFor(() => expect(screen.getByText("new.txt")).toBeTruthy());
    expect(screen.queryByText("old.txt")).toBeNull(); // stale batch never rendered
    expect(screen.getAllByTestId("fh-row").length).toBe(1);
  });
});

// CPE-1323: the exclude-glob configuration UI, shared across all four tabs. The backend already
// accepts `excludes: string[]` on every scan command (CPE-1302) — this suite proves the UI actually
// configures it: typed/added patterns render as removable pills, a quick-add suggestion only applies
// on click (never pre-applied), and the CONFIGURED array — not a hardcoded `[]` — is what reaches the
// scan command, only as of the NEXT Scan/Rescan click.
describe("FileHealthDialog — exclude-glob configuration (CPE-1323)", () => {
  it("typing a pattern and pressing Enter adds a removable pill, with no scan triggered", async () => {
    render(FileHealthDialog, { root: "/repo" });
    const input = screen.getByTestId("fh-exclude-input");

    await fireEvent.input(input, { target: { value: "node_modules" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    expect(screen.getByText("node_modules")).toBeTruthy();
    expect(screen.getAllByTestId("fh-exclude-remove").length).toBe(1);
    expect(invoke).not.toHaveBeenCalled(); // editing excludes never runs a scan by itself
  });

  it("a configured exclude pattern is passed to the scan call, replacing the old hardcoded []", async () => {
    render(FileHealthDialog, { root: "/repo" });
    const input = screen.getByTestId("fh-exclude-input");
    await fireEvent.input(input, { target: { value: ".git" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    expect(invoke).toHaveBeenCalledWith(
      "find_dangling_links_stream",
      expect.objectContaining({ root: "/repo", excludes: [".git"], streamId: 1 }),
    );
  });

  it("quick-add suggestion chips add on click only — NOT pre-applied to a scan run without touching them", async () => {
    render(FileHealthDialog, { root: "/repo" });

    // A scan run without ever touching the exclude UI still gets an empty excludes array.
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    expect(invoke).toHaveBeenCalledWith(
      "find_dangling_links_stream",
      expect.objectContaining({ excludes: [] }),
    );
  });

  it("clicking a quick-add suggestion chip adds that pattern as a pill", async () => {
    render(FileHealthDialog, { root: "/repo" });
    const suggestions = screen.getAllByTestId("fh-exclude-suggest");
    const nodeModules = suggestions.find((b) => b.textContent?.includes("node_modules"));
    expect(nodeModules).toBeTruthy();

    await fireEvent.click(nodeModules!);
    expect(screen.getByText("node_modules")).toBeTruthy();

    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    expect(invoke).toHaveBeenCalledWith(
      "find_dangling_links_stream",
      expect.objectContaining({ excludes: ["node_modules"] }),
    );
  });

  it("removing a pill drops it from the NEXT scan's excludes array", async () => {
    render(FileHealthDialog, { root: "/repo" });
    const input = screen.getByTestId("fh-exclude-input");
    await fireEvent.input(input, { target: { value: "target" } });
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(screen.getByText("target")).toBeTruthy();

    await fireEvent.click(screen.getByTestId("fh-exclude-remove"));
    expect(screen.queryByText("target")).toBeNull();

    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    expect(invoke).toHaveBeenCalledWith(
      "find_dangling_links_stream",
      expect.objectContaining({ excludes: [] }),
    );
  });

  it("does not add a duplicate or blank pattern", async () => {
    render(FileHealthDialog, { root: "/repo" });
    const input = screen.getByTestId("fh-exclude-input");

    await fireEvent.input(input, { target: { value: "dist" } });
    await fireEvent.keyDown(input, { key: "Enter" });
    await fireEvent.input(input, { target: { value: "dist" } });
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(screen.getAllByTestId("fh-exclude-remove").length).toBe(1);

    await fireEvent.keyDown(input, { key: "Enter" }); // blank draft — no-op
    expect(screen.getAllByTestId("fh-exclude-remove").length).toBe(1);
  });
});
