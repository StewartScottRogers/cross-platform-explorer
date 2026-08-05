import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import FileHealthDialog from "./FileHealthDialog.svelte";

// CPE-1316 cross-scan integration coverage: the panel now runs THREE independent streaming scans
// (dangling / mismatch / orphan), each with its OWN generation counter that doubles as its OWN
// frontend-supplied streamId, paired with its OWN `cancel_*_stream` command. This suite proves that
// running all three (in any tab order) never lets one scan's generation counter or cancel call bleed
// into another's — the exact bug class CPE-1316 called out ("don't let the 3 scans share one searchGen
// incorrectly"). Also covers the `initialTab` prop that lets a Tools-menu / palette entry open the panel
// straight to a specific tab.
type Pending = {
  cmd: string;
  channel: { onmessage: ((b: unknown) => void) | null };
  streamId: number;
  resolve: (v: unknown) => void;
};
let pending: Pending[] = [];
let cancels: Array<{ cmd: string; streamId: number }> = [];

const CHANNEL_ARG: Record<string, string> = {
  find_dangling_links_stream: "onLink",
  find_type_mismatches_stream: "onHit",
  find_orphan_sidecars_stream: "onOrphan",
};
const RESULT_KEY: Record<string, string> = {
  find_dangling_links_stream: "links",
  find_type_mismatches_stream: "hits",
  find_orphan_sidecars_stream: "orphans",
};

const invoke = vi.fn(async (cmd: string, args?: any) => {
  if (cmd in CHANNEL_ARG) {
    return await new Promise((resolve) =>
      pending.push({ cmd, channel: args[CHANNEL_ARG[cmd]], streamId: args.streamId, resolve }),
    );
  }
  if (cmd.startsWith("cancel_")) {
    cancels.push({ cmd, streamId: args.streamId });
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

function callsFor(cmd: string) {
  return pending.filter((p) => p.cmd === cmd);
}
async function finishCmd(cmd: string, index: number, scanned: number) {
  const calls = callsFor(cmd);
  calls[index].resolve({ [RESULT_KEY[cmd]]: [], scanned, truncated: false });
  await Promise.resolve();
}

beforeEach(() => {
  invoke.mockClear();
  pending = [];
  cancels = [];
});

describe("FileHealthDialog — cross-tab generation/cancel isolation (CPE-1316)", () => {
  it("each of the three tabs starts its OWN streamId at 1, independent of the others", async () => {
    render(FileHealthDialog, { root: "/repo" });

    // Run the dangling scan (default tab) — its streamId is 1.
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    expect(invoke).toHaveBeenCalledWith("find_dangling_links_stream", expect.objectContaining({ streamId: 1 }));
    await finishCmd("find_dangling_links_stream", 0, 5);

    // Switch to Type mismatches and run it — its streamId is ALSO 1 (own counter), not 2.
    await fireEvent.click(screen.getByTestId("fh-tab-mismatch"));
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    expect(invoke).toHaveBeenCalledWith("find_type_mismatches_stream", expect.objectContaining({ streamId: 1 }));
    await finishCmd("find_type_mismatches_stream", 0, 5);

    // Switch to Orphan sidecars and run it — its streamId is ALSO 1 (own counter).
    await fireEvent.click(screen.getByTestId("fh-tab-orphan"));
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    expect(invoke).toHaveBeenCalledWith(
      "find_orphan_sidecars_stream",
      expect.objectContaining({ streamId: 1, recursive: true }),
    );
    await finishCmd("find_orphan_sidecars_stream", 0, 5);

    // No cancel call was needed anywhere — each tab's FIRST scan, no rescans.
    expect(cancels).toEqual([]);
  });

  it("rescanning one tab cancels only that tab's PRIOR stream — the other two tabs' counters/cancels are untouched", async () => {
    render(FileHealthDialog, { root: "/repo" });

    // Start a dangling scan and leave it in flight (streamId 1, never resolved).
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));

    // Start the mismatch scan and let it FINISH (its rescan button only appears once loading clears).
    await fireEvent.click(screen.getByTestId("fh-tab-mismatch"));
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    await finishCmd("find_type_mismatches_stream", 0, 3);
    await waitFor(() => expect(screen.getByTestId("fh-rescan-btn")).toBeTruthy());

    // Start an orphan scan and leave IT in flight too (streamId 1, never resolved).
    await fireEvent.click(screen.getByTestId("fh-tab-orphan"));
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));

    // Now go back to the mismatch tab (still showing its finished results) and rescan it. Only
    // mismatch's gen-1 stream should be cancelled; the still-in-flight dangling/orphan streamIds must
    // never appear in a cancel call for the wrong command.
    await fireEvent.click(screen.getByTestId("fh-tab-mismatch"));
    await fireEvent.click(screen.getByTestId("fh-rescan-btn"));

    expect(cancels).toEqual([{ cmd: "cancel_type_mismatches_stream", streamId: 1 }]);
    expect(invoke).toHaveBeenCalledWith("find_type_mismatches_stream", expect.objectContaining({ streamId: 2 }));
    // The dangling and orphan scans, still in flight, were never cancelled by the mismatch rescan.
    expect(cancels.some((c) => c.cmd === "cancel_dangling_links_stream")).toBe(false);
    expect(cancels.some((c) => c.cmd === "cancel_orphan_sidecars_stream")).toBe(false);
  });

  it("initialTab prop opens the panel straight to the requested tab", async () => {
    render(FileHealthDialog, { root: "/repo", initialTab: "orphan" });
    // The orphan tab's intro/scan button is visible immediately — no tab click needed.
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    expect(invoke).toHaveBeenCalledWith("find_orphan_sidecars_stream", expect.objectContaining({ streamId: 1 }));
    expect(invoke).not.toHaveBeenCalledWith("find_dangling_links_stream", expect.anything());
  });
});

// CPE-1317's already-open tab-switch fix. Slice-2 UAT finding: `activeTab = initialTab` is a one-time
// initializer, so invoking a different File-Health Tools-menu/palette entry while the panel is ALREADY
// OPEN never jumped the visible tab (`{#if fileHealthOpen}` never remounts, so the initializer never
// re-runs). The fix is a host-bumped `openNonce` prop that FileHealthDialog reacts to — this suite drives
// that reaction directly via `rerender` (simulating App.svelte's prop updates from `openFileHealth`),
// without needing App.svelte itself, and is written to be FALSIFIABLE: each assertion targets a specific
// tab's `aria-selected` state, so a broken (or reverted) fix fails these deterministically rather than by
// accident.
describe("FileHealthDialog — already-open tab-switch fix (CPE-1317)", () => {
  function isActive(tabId: string): boolean {
    return screen.getByTestId(`fh-tab-${tabId}`).getAttribute("aria-selected") === "true";
  }

  it("bumping openNonce with a new initialTab jumps the ALREADY-OPEN panel to that tab", async () => {
    const { rerender } = render(FileHealthDialog, { root: "/repo", initialTab: "dangling", openNonce: 1 });
    expect(isActive("dangling")).toBe(true);
    expect(isActive("mismatch")).toBe(false);

    // Simulate re-invoking "Find type mismatches…" while the panel is already open on Dangling.
    await rerender({ root: "/repo", initialTab: "mismatch", openNonce: 2 });
    expect(isActive("mismatch")).toBe(true);
    expect(isActive("dangling")).toBe(false);

    // A different entry again (empty-folders, slice 3's new tab) also jumps straight there.
    await rerender({ root: "/repo", initialTab: "empty", openNonce: 3 });
    expect(isActive("empty")).toBe(true);
    expect(isActive("mismatch")).toBe(false);
  });

  it("re-invoking the SAME entry while manually parked on a different tab jumps back — a changed initialTab is not required, only a changed nonce", async () => {
    const { rerender } = render(FileHealthDialog, { root: "/repo", initialTab: "dangling", openNonce: 1 });

    // User manually switches to Orphan sidecars (no prop change — a plain in-panel click).
    await fireEvent.click(screen.getByTestId("fh-tab-orphan"));
    expect(isActive("orphan")).toBe(true);
    expect(isActive("dangling")).toBe(false);

    // The host re-invokes the SAME "Find dangling links…" entry: `initialTab` is UNCHANGED ("dangling"),
    // only `openNonce` bumps. A plain `$: activeTab = initialTab` would see no change here and do
    // nothing — the panel would stay stuck on Orphan. The nonce-based fix must still jump back.
    await rerender({ root: "/repo", initialTab: "dangling", openNonce: 2 });
    expect(isActive("dangling")).toBe(true);
    expect(isActive("orphan")).toBe(false);
  });

  it("manual in-panel tab clicks still work and are not clobbered by a re-render that does NOT bump the nonce", async () => {
    const { rerender } = render(FileHealthDialog, { root: "/repo", initialTab: "dangling", openNonce: 1 });

    await fireEvent.click(screen.getByTestId("fh-tab-mismatch"));
    expect(isActive("mismatch")).toBe(true);

    // A re-render with the SAME openNonce (e.g. an unrelated prop like `root` changing) must NOT reset
    // the user's manual tab choice — only a nonce bump should move `activeTab`.
    await rerender({ root: "/repo/other", initialTab: "dangling", openNonce: 1 });
    expect(isActive("mismatch")).toBe(true);
    expect(isActive("dangling")).toBe(false);
  });
});
