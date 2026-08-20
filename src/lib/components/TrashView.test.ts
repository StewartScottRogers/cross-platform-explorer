/**
 * Component tests for the browsable Trash view (CPE-1560, epic CPE-1486 final slice): streamed listing,
 * Restore (drops rows on success, surfaces per-item failures), and Empty (confirmed, irreversible) —
 * the GUI counterpart the headless harness can't drive.
 *
 * Mirrors `DiskSpaceView.test.ts`'s mocking strategy: mock `@tauri-apps/api/core`'s `invoke` + `Channel`
 * directly, since `rawInvoke`/`createChannel` (`../invoke`) flow through that module under the local
 * transport, and the generated `commands.*` client's `TAURI_INVOKE` is the same underlying `invoke`.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/svelte";
import { tick } from "svelte";
import TrashView from "./TrashView.svelte";

interface StreamCall {
  cmd: string;
  args: any;
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
}

let streamCalls: StreamCall[] = [];
let invokeImpl: (cmd: string, args?: any) => Promise<any> = () => Promise.reject(new Error("unhandled"));

const invoke = vi.fn((cmd: string, args?: any) => {
  if (cmd === "list_trash_stream") {
    return new Promise((resolve, reject) => streamCalls.push({ cmd, args, resolve, reject }));
  }
  return invokeImpl(cmd, args);
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

async function settle() {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  await tick();
}

const entry = (over: Partial<{ id: string; name: string; original_path: string; time_deleted: number; size: number | null }> = {}) => ({
  id: "id-1",
  name: "report.txt",
  original_path: "C:\\Users\\stew\\Documents\\report.txt",
  time_deleted: 1_700_000_000,
  size: 1024,
  ...over,
});

/** Deliver a batch on the most recent `list_trash_stream` call's channel. */
function deliver(batch: ReturnType<typeof entry>[], callIndex = streamCalls.length - 1) {
  streamCalls[callIndex].args.onEntry.onmessage(batch);
}

/** Resolve a `list_trash_stream` call as finished (the real command resolves with a `{ count, degraded }`
 *  summary once every batch has flushed — a genuinely empty Trash resolves with `{ count: 0, degraded:
 *  false }` having sent no batches at all; CPE-1803's degraded case also sends no batches but resolves
 *  with `degraded: true`, which is the whole point — the two are otherwise indistinguishable). */
function finishStream(count: number, degraded = false, callIndex = streamCalls.length - 1) {
  streamCalls[callIndex].resolve({ count, degraded });
}

beforeEach(() => {
  invoke.mockClear();
  streamCalls = [];
  invokeImpl = () => Promise.reject(new Error("unhandled"));
});

describe("TrashView — streamed listing (CPE-1560)", () => {
  it("shows a loading state, then paints rows as batches arrive", async () => {
    render(TrashView);
    await settle();
    expect(screen.getByText("Loading Trash…")).toBeTruthy();
    expect(streamCalls).toHaveLength(1);

    deliver([entry()]);
    await settle();

    expect(screen.queryByText("Loading Trash…")).toBeNull();
    expect(screen.getByText("report.txt")).toBeTruthy();
    expect(screen.getByText("C:\\Users\\stew\\Documents\\report.txt")).toBeTruthy();
  });

  it("shows the empty state once the stream finishes having sent no batches", async () => {
    render(TrashView);
    await settle();
    expect(screen.getByText("Loading Trash…")).toBeTruthy();

    // A genuinely empty Trash: the command resolves with count 0, degraded: false, without ever calling
    // onEntry.
    finishStream(0, false);
    await settle();

    expect(screen.queryByText("Loading Trash…")).toBeNull();
    expect(screen.getByText("Trash is empty")).toBeTruthy();
    // CPE-1803: a genuinely empty Trash must NOT also render the degraded message — half of the
    // distinguishability this ticket exists to prove is that these two zero-entry states diverge.
    expect(screen.queryByText(/couldn't be fully read/i)).toBeNull();
  });

  // CPE-1803: a degraded listing (CPE-1791's caught-panic boundary) sends zero batches over the channel,
  // same as a genuinely empty Trash — the ONLY difference the frontend has to go on is the `degraded`
  // flag on the resolved summary. This test and the one above must both pass for the two states to be
  // proven distinguishable; a suite that only asserted this one could pass even if `degraded` were
  // ignored and both cases always rendered "Trash is empty".
  it("shows a distinct degraded state — not 'Trash is empty' — when the listing degraded (CPE-1791/1803)", async () => {
    render(TrashView);
    await settle();
    expect(screen.getByText("Loading Trash…")).toBeTruthy();

    // A degraded pass: same shape as a genuinely empty Trash over the channel (no batches sent), but the
    // resolved summary flags degraded: true.
    finishStream(0, true);
    await settle();

    expect(screen.queryByText("Loading Trash…")).toBeNull();
    expect(screen.getByText("Trash couldn't be fully read — it may not be empty")).toBeTruthy();
    expect(screen.queryByText("Trash is empty")).toBeNull();
  });

  it("appends multiple batches across the same stream", async () => {
    render(TrashView);
    await settle();
    deliver([entry({ id: "a", name: "a.txt" })]);
    await settle();
    deliver([entry({ id: "b", name: "b.txt" })]);
    await settle();

    expect(screen.getByText("a.txt")).toBeTruthy();
    expect(screen.getByText("b.txt")).toBeTruthy();
  });

  it("Refresh re-invokes the stream and supersedes a stale in-flight one", async () => {
    render(TrashView);
    await settle();
    deliver([entry({ id: "a", name: "a.txt" })]);
    await settle();
    expect(screen.getByText("a.txt")).toBeTruthy();

    await fireEvent.click(screen.getByTitle("Refresh"));
    await settle();
    expect(streamCalls).toHaveLength(2);
    // The list is cleared immediately on refresh, even before the new stream sends anything.
    expect(screen.queryByText("a.txt")).toBeNull();

    // A stale batch from the FIRST (superseded) stream must be dropped.
    deliver([entry({ id: "stale", name: "stale.txt" })], 0);
    await settle();
    expect(screen.queryByText("stale.txt")).toBeNull();

    deliver([entry({ id: "b", name: "b.txt" })], 1);
    await settle();
    expect(screen.getByText("b.txt")).toBeTruthy();
  });

  it("shows a friendly error when the stream invoke rejects", async () => {
    invoke.mockImplementationOnce((cmd: string) => {
      if (cmd === "list_trash_stream") return Promise.reject("boom");
      return Promise.reject(new Error("unhandled"));
    });
    render(TrashView);
    await settle();
    expect(screen.getByText(/Couldn't load the Trash/)).toBeTruthy();
  });
});

describe("TrashView — Restore (CPE-1560)", () => {
  async function renderWithTwoEntries() {
    const utils = render(TrashView);
    await settle();
    deliver([
      entry({ id: "keep", name: "keep.txt" }),
      entry({ id: "gone", name: "gone.txt" }),
    ]);
    await settle();
    return utils;
  }

  it("Restore selected is disabled until something is checked", async () => {
    await renderWithTwoEntries();
    const restoreBtn = screen.getByText("Restore selected").closest("button") as HTMLButtonElement;
    expect(restoreBtn.disabled).toBe(true);

    await fireEvent.click(screen.getByLabelText("gone.txt"));
    expect(restoreBtn.disabled).toBe(false);
  });

  it("restores the checked item, drops it from the list, and calls the command with its id", async () => {
    invokeImpl = (cmd) => {
      if (cmd === "restore_trash_items") {
        return Promise.resolve([{ path: "C:\\restored\\gone.txt", ok: true, error: "" }]);
      }
      return Promise.reject(new Error(`unexpected ${cmd}`));
    };
    await renderWithTwoEntries();

    await fireEvent.click(screen.getByLabelText("gone.txt"));
    await fireEvent.click(screen.getByText("Restore selected"));
    await settle();

    expect(invoke).toHaveBeenCalledWith("restore_trash_items", { ids: ["gone"] });
    expect(screen.queryByText("gone.txt")).toBeNull();
    expect(screen.getByText("keep.txt")).toBeTruthy();
  });

  it("surfaces a per-item restore failure without removing the row or the rest of the selection", async () => {
    invokeImpl = (cmd) => {
      if (cmd === "restore_trash_items") {
        return Promise.resolve([
          { path: "C:\\restored\\keep.txt", ok: true, error: "" },
          { path: "gone", ok: false, error: "Something already exists at the original location" },
        ]);
      }
      return Promise.reject(new Error(`unexpected ${cmd}`));
    };
    await renderWithTwoEntries();

    await fireEvent.click(screen.getByLabelText("keep.txt"));
    await fireEvent.click(screen.getByLabelText("gone.txt"));
    await fireEvent.click(screen.getByText("Restore selected"));
    await settle();

    // The failed item stays in the list; the successfully restored one is dropped.
    expect(screen.queryByText("keep.txt")).toBeNull();
    expect(screen.getByText("gone.txt")).toBeTruthy();
    expect(screen.getByText(/Couldn't restore gone\.txt/)).toBeTruthy();
    expect(screen.getByText(/Something already exists/)).toBeTruthy();
  });
});

describe("TrashView — Empty (CPE-1560, irreversible → ConfirmDialog per MENUS.md)", () => {
  async function renderWithTwoEntries() {
    const utils = render(TrashView);
    await settle();
    deliver([
      entry({ id: "a", name: "a.txt" }),
      entry({ id: "b", name: "b.txt" }),
    ]);
    await settle();
    return utils;
  }

  it("Empty Trash opens a confirm dialog before purging anything", async () => {
    invokeImpl = () => Promise.reject(new Error("empty_trash must not be called before confirming"));
    await renderWithTwoEntries();

    await fireEvent.click(screen.getByText("Empty Trash"));
    await settle();

    expect(screen.getByText("Empty Trash?")).toBeTruthy();
    expect(invoke).not.toHaveBeenCalledWith("empty_trash", expect.anything());
  });

  it("confirming Empty Trash purges everything and calls empty_trash with no ids", async () => {
    invokeImpl = (cmd) => (cmd === "empty_trash" ? Promise.resolve(null) : Promise.reject(new Error(cmd)));
    await renderWithTwoEntries();

    await fireEvent.click(screen.getByText("Empty Trash"));
    await settle();
    // The dialog's own primary button carries the same confirm label.
    const dialog = screen.getByText("Empty Trash?").closest(".dialog") as HTMLElement;
    await fireEvent.click(within(dialog).getByText("Empty Trash"));
    await settle();

    // `confirmed: true` (CPE-1651) — the backend refuses an unconsented purge, and this dialog is the
    // only thing allowed to set it.
    expect(invoke).toHaveBeenCalledWith("empty_trash", { ids: null, confirmed: true });
    expect(screen.getByText("Trash is empty")).toBeTruthy();
  });

  it("cancelling the confirm dialog leaves the Trash untouched", async () => {
    invokeImpl = () => Promise.reject(new Error("empty_trash must not be called"));
    await renderWithTwoEntries();

    await fireEvent.click(screen.getByText("Empty Trash"));
    await settle();
    await fireEvent.click(screen.getByText("Cancel"));
    await settle();

    expect(screen.queryByText("Empty Trash?")).toBeNull();
    expect(screen.getByText("a.txt")).toBeTruthy();
    expect(screen.getByText("b.txt")).toBeTruthy();
  });

  it("Delete selected permanently confirms with the selected count, then purges only those ids", async () => {
    invokeImpl = (cmd) => (cmd === "empty_trash" ? Promise.resolve(null) : Promise.reject(new Error(cmd)));
    await renderWithTwoEntries();

    await fireEvent.click(screen.getByLabelText("a.txt"));
    await fireEvent.click(screen.getByText("Delete selected permanently"));
    await settle();

    expect(screen.getByText(/permanently deletes 1 selected item/)).toBeTruthy();
    const dialog = screen.getByText("Empty Trash?").closest(".dialog") as HTMLElement;
    await fireEvent.click(within(dialog).getByText("Empty Trash"));
    await settle();

    expect(invoke).toHaveBeenCalledWith("empty_trash", { ids: ["a"], confirmed: true });
    expect(screen.queryByText("a.txt")).toBeNull();
    expect(screen.getByText("b.txt")).toBeTruthy();
  });

  it("Delete selected permanently is disabled with nothing checked", async () => {
    await renderWithTwoEntries();
    const btn = screen.getByText("Delete selected permanently").closest("button") as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });
});

describe("TrashView — chrome", () => {
  it("dispatches close from the × button", async () => {
    const { component } = render(TrashView);
    await settle();
    const closed = vi.fn();
    component.$on("close", closed);

    await fireEvent.click(screen.getByTitle("Close"));
    expect(closed).toHaveBeenCalledOnce();
  });
});
