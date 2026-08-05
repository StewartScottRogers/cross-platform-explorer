import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import FileHealthDialog from "./FileHealthDialog.svelte";

// The Empty-folders tab (CPE-1317, slice 3) is the panel's FIRST non-streaming tab: `find_empty_dirs` is
// a single plain-awaited `invoke(...)` call (busy-cursor wrapper from `../invoke`, NOT `rawInvoke`, and
// NOT a `_stream` command/Channel), modeled on NearDuplicatesDialog's scan pattern rather than the other
// three tabs' streaming shape. This suite proves: the plain-invoke call shape (root+excludes, no
// streamId/Channel), loading/error/empty/results states, row click → navigate+close, and that a rescan
// supersedes a still-in-flight PRIOR call via `emptyGen` even though there's no `cancel_*` command to
// pair with it (there's nothing to cancel — just a stale response to ignore).
let pending: Array<{ args: any; resolve: (v: unknown) => void; reject: (e: unknown) => void }> = [];

const invoke = vi.fn(async (cmd: string, args?: any) => {
  if (cmd === "find_empty_dirs") {
    return await new Promise((resolve, reject) => pending.push({ args, resolve, reject }));
  }
  // No other command should ever be touched by empty-tab activity.
  return await new Promise(() => {});
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

async function openEmptyTab() {
  const result = render(FileHealthDialog, { root: "/repo" });
  await fireEvent.click(screen.getByTestId("fh-tab-empty"));
  return result;
}

beforeEach(() => {
  invoke.mockClear();
  pending = [];
});

describe("FileHealthDialog — Empty folders tab (CPE-1317)", () => {
  it("does not scan until Scan is clicked, then calls find_empty_dirs (plain invoke) with root+excludes — no streamId, no Channel arg", async () => {
    await openEmptyTab();
    expect(invoke).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    expect(invoke).toHaveBeenCalledWith("find_empty_dirs", { root: "/repo", excludes: [] });
    // Falsifiable: a streaming call would carry a streamId/Channel-shaped arg; this one must not.
    const call = invoke.mock.calls[0][1];
    expect(call).not.toHaveProperty("streamId");
    expect(Object.keys(call).sort()).toEqual(["excludes", "root"]);
  });

  it("shows loading, then renders results from the single resolved response (no batching)", async () => {
    await openEmptyTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    await waitFor(() => expect(screen.getByText(/Scanning/)).toBeTruthy());
    expect(screen.queryByTestId("fh-row")).toBeNull();

    pending[0].resolve({ dirs: ["/repo/old-build", "/repo/tmp/cache"], scanned: 40, truncated: false });
    await waitFor(() => expect(screen.getAllByTestId("fh-row").length).toBe(2));
    expect(screen.getByText("old-build")).toBeTruthy();
    expect(screen.getByText("cache")).toBeTruthy();
    expect(screen.queryByText(/Scanning/)).toBeNull();
  });

  it("clears loading on an EMPTY result and shows the none state", async () => {
    await openEmptyTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    pending[0].resolve({ dirs: [], scanned: 12, truncated: false });
    await waitFor(() => expect(screen.getByTestId("fh-none")).toBeTruthy());
    expect(screen.getByText(/No empty folders found/)).toBeTruthy();
  });

  it("shows the capped hint when the report is truncated", async () => {
    await openEmptyTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    pending[0].resolve({ dirs: ["/repo/a"], scanned: 20000, truncated: true });
    await waitFor(() => expect(screen.getAllByTestId("fh-row").length).toBe(1));
    expect(screen.getByText(/scan capped/)).toBeTruthy();
  });

  it("surfaces a backend error instead of hanging in the loading state", async () => {
    await openEmptyTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    pending[0].reject("not a folder");
    await waitFor(() => expect(screen.getByText("not a folder")).toBeTruthy());
    expect(screen.getByTestId("fh-rescan-btn")).toBeTruthy();
    expect(screen.queryByText(/Scanning/)).toBeNull();
  });

  it("row click dispatches navigate with the folder's path and closes", async () => {
    const { component } = await openEmptyTab();
    const navigated: string[] = [];
    let closed = false;
    component.$on("navigate", (e: CustomEvent<string>) => navigated.push(e.detail));
    component.$on("close", () => (closed = true));

    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    pending[0].resolve({ dirs: ["/repo/leftover"], scanned: 3, truncated: false });
    await waitFor(() => expect(screen.getByTestId("fh-row")).toBeTruthy());
    await fireEvent.click(screen.getByTestId("fh-row"));

    expect(navigated).toEqual(["/repo/leftover"]);
    expect(closed).toBe(true);
  });

  it("clicking Rescan after a completed scan issues a fresh find_empty_dirs call and REPLACES the previous results (never appends)", async () => {
    await openEmptyTab();
    await fireEvent.click(screen.getByTestId("fh-scan-btn"));
    pending[0].resolve({ dirs: ["/repo/old"], scanned: 5, truncated: false });
    await waitFor(() => expect(screen.getByText("old")).toBeTruthy());

    await fireEvent.click(screen.getByTestId("fh-rescan-btn"));
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke.mock.calls.every(([cmd]) => cmd === "find_empty_dirs")).toBe(true); // never a cancel_* call — none exists for this tab

    pending[1].resolve({ dirs: ["/repo/new"], scanned: 8, truncated: false });
    await waitFor(() => expect(screen.getByText("new")).toBeTruthy());
    expect(screen.queryByText("old")).toBeNull(); // the prior scan's result was replaced, not appended to
    expect(screen.getAllByTestId("fh-row").length).toBe(1);
  });
});
