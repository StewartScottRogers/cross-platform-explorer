/**
 * CPE-1533 (epic CPE-1489 finale) — "Move all here"/"Copy all here": the Drop Stack panel's two new
 * buttons (CPE-1532's DropStackPanel.svelte) wired to App.svelte's `doDropStackMoveAll`/
 * `doDropStackCopyAll` (declared right next to `doPaste`), reusing the SAME transfer commands
 * (`commands.moveEntries` / `commands.startTransfer`) `doPaste` already uses — no new backend surface.
 * Move-all takes the synchronous per-item-result path (mirroring `doPaste`'s cut branch: `moveEntries`
 * resolves one `OpResult` per source, so a partial failure clears only what actually moved). Copy-all
 * queues through the transfer engine (mirroring `doPaste`'s copy branch: a name collision pauses for the
 * CPE-624 conflict dialog first), so its Drop-Stack-clearing has to wait for a `transfer://done` event
 * rather than an awaited call — and since that report is aggregate-only (no per-path result), a failed
 * transfer leaves the WHOLE captured batch shelved rather than guessing which paths landed.
 *
 * Same mounted-App-with-mocked-backend single-pane harness as App.dropStackEntry.test.ts, plus
 * App.paneBArchiveVault.test.ts's multi-handler `transfer://done` event bus (App.svelte's own listener
 * AND `lib/transfers.ts`'s `initTransfers` both subscribe to it, so a single-handler stub would silently
 * drop one of them).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import { get } from "svelte/store";
import App from "./App.svelte";
import { resetSettings } from "./lib/settings";
import { clearDropStack, dropStackEntries, addToDropStack } from "./lib/dropStack";
import type { DirEntry, Place } from "./lib/types";

const file = (name: string, dir: string): DirEntry => ({
  name,
  path: `${dir}\\${name}`,
  is_dir: false,
  size: 10,
  modified: new Date(2026, 6, 10, 15, 0).getTime(),
  extension: "txt",
  hidden: false,
  is_symlink: false,
});

const PATH_A = "C:\\d"; // the folder the app navigates into — the Move-all/Copy-all DESTINATION
const PATH_ELSEWHERE = "C:\\elsewhere"; // where the shelved items were picked up FROM — never navigated to,
// proving the Drop Stack's whole point: it acts on paths regardless of which folder they came from.
const drives: Place[] = [{ name: "Local Disk (C:)", path: PATH_A, kind: "drive" }];

const { invoke, Channel } = vi.hoisted(() => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: (batch: unknown) => void = () => {};
  },
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke, convertFileSrc: (p: string) => `asset://${p}`, Channel }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

// Same multi-handler event bus as App.paneBArchiveVault.test.ts — needed because BOTH App.svelte's own
// `transfer://done` listener and `lib/transfers.ts`'s `initTransfers` subscribe to the same event name.
const { eventHandlers } = vi.hoisted(() => ({ eventHandlers: new Map<string, Array<(e: { payload: unknown }) => void>>() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, handler: (e: { payload: unknown }) => void) => {
    const list = eventHandlers.get(event) ?? [];
    list.push(handler);
    eventHandlers.set(event, list);
    return () => eventHandlers.set(event, (eventHandlers.get(event) ?? []).filter((h) => h !== handler));
  }),
}));
function emitEvent(event: string, payload: unknown) {
  for (const handler of eventHandlers.get(event) ?? []) handler({ payload });
}

let moveEntriesCalls: { paths: string[]; dest: string }[] = [];
let startTransferCalls: { sources: string[]; dest: string; kind: string; policy: string }[] = [];
let moveEntriesResult: { path: string; ok: boolean; error: string }[] | null = null; // set per-test to override the default all-ok result
let nextTransferId = 1;
let disks: Record<string, DirEntry[]> = {};

beforeEach(() => {
  localStorage.clear();
  resetSettings();
  clearDropStack();
  eventHandlers.clear();
  Element.prototype.scrollIntoView = vi.fn();
  moveEntriesCalls = [];
  startTransferCalls = [];
  moveEntriesResult = null;
  nextTransferId = 1;
  disks = { [PATH_A]: [file("existing.txt", PATH_A)] };
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const listingFor = (path: unknown) => disks[path as string] ?? [];
    switch (cmd) {
      case "special_folders": return [];
      case "list_drives": return drives;
      case "home_dir": return "C:\\Users\\t";
      case "can_restore_from_trash": return true;
      case "list_dir": return listingFor(args.path);
      case "list_dir_stream": {
        const ch = args.onEntry as { onmessage: (b: unknown) => void };
        const data = listingFor(args.path);
        ch.onmessage(data);
        return data.length;
      }
      case "parent_dir": return null;
      case "read_file_text": return "";
      case "move_entries": {
        const paths = args.paths as string[];
        const dest = args.dest as string;
        moveEntriesCalls.push({ paths, dest });
        return moveEntriesResult ?? paths.map((p) => ({ path: p, ok: true, error: "" }));
      }
      case "start_transfer": {
        const sources = args.sources as string[];
        const dest = args.dest as string;
        const kind = args.kind as string;
        const policy = args.policy as string;
        startTransferCalls.push({ sources, dest, kind, policy });
        return nextTransferId++;
      }
      default: return null;
    }
  });
});

/** Boot into PATH_A, shelve two items picked up from a DIFFERENT folder, then open the Drop Stack panel
 *  so its buttons are on screen. */
async function bootWithShelvedItems() {
  addToDropStack([`${PATH_ELSEWHERE}\\one.txt`, `${PATH_ELSEWHERE}\\two.txt`], PATH_ELSEWHERE);
  render(App);
  const driveButtons = await screen.findAllByText("Local Disk (C:)");
  await fireEvent.click(driveButtons[0]);
  await waitFor(() => expect(screen.getByText("existing.txt")).toBeTruthy());

  await fireEvent.click(screen.getByLabelText("Show Drop Stack"));
}

describe("App — Drop Stack Move-all/Copy-all (CPE-1533)", () => {
  it("Move all here calls moveEntries with the full stack's paths and currentPath, then clears the stack", async () => {
    await bootWithShelvedItems();

    await fireEvent.click(screen.getByText("Move all here"));

    await waitFor(() => expect(moveEntriesCalls.length).toBe(1));
    expect(moveEntriesCalls[0]).toEqual({
      paths: [`${PATH_ELSEWHERE}\\one.txt`, `${PATH_ELSEWHERE}\\two.txt`],
      dest: PATH_A,
    });
    await waitFor(() => expect(get(dropStackEntries)).toHaveLength(0));
  });

  it("Move all here: a partial failure leaves only the un-moved path shelved (mirrors doPaste's per-item handling)", async () => {
    await bootWithShelvedItems();
    moveEntriesResult = [
      { path: `${PATH_A}\\one.txt`, ok: true, error: "" },
      { path: `${PATH_ELSEWHERE}\\two.txt`, ok: false, error: "locked" },
    ];

    await fireEvent.click(screen.getByText("Move all here"));

    await waitFor(() => expect(moveEntriesCalls.length).toBe(1));
    await waitFor(() => expect(get(dropStackEntries)).toHaveLength(1));
    expect(get(dropStackEntries)[0].path).toBe(`${PATH_ELSEWHERE}\\two.txt`);
  });

  it("Copy all here calls startTransfer (kind copy) with the full stack's paths and currentPath, then clears the stack once transfer://done reports a clean finish", async () => {
    await bootWithShelvedItems();

    await fireEvent.click(screen.getByText("Copy all here"));

    await waitFor(() => expect(startTransferCalls.length).toBe(1));
    expect(startTransferCalls[0]).toEqual({
      sources: [`${PATH_ELSEWHERE}\\one.txt`, `${PATH_ELSEWHERE}\\two.txt`],
      dest: PATH_A,
      kind: "copy",
      policy: "keepboth",
    });
    // Not cleared yet — the transfer is still in flight (async, unlike the synchronous move path).
    expect(get(dropStackEntries)).toHaveLength(2);

    emitEvent("transfer://done", {
      id: 1,
      op: "copy",
      transferred: 2,
      skipped: 0,
      failed: 0,
      cancelled: false,
      errors: [],
    });

    await waitFor(() => expect(get(dropStackEntries)).toHaveLength(0));
  });

  it("Copy all here: a failed transfer leaves the whole batch shelved for retry, and still surfaces the error", async () => {
    await bootWithShelvedItems();

    await fireEvent.click(screen.getByText("Copy all here"));
    await waitFor(() => expect(startTransferCalls.length).toBe(1));

    emitEvent("transfer://done", {
      id: 1,
      op: "copy",
      transferred: 1,
      skipped: 0,
      failed: 1,
      cancelled: false,
      errors: ["permission denied"],
    });

    await waitFor(() => expect(screen.getByText((c) => c.includes("1 failed"))).toBeTruthy());
    expect(get(dropStackEntries)).toHaveLength(2);
  });

  it("a name collision against the destination pauses for the CPE-624 conflict dialog before starting the transfer", async () => {
    disks[PATH_A] = [...disks[PATH_A], file("one.txt", PATH_A)]; // basename collides with a shelved item
    await bootWithShelvedItems();

    await fireEvent.click(screen.getByText("Copy all here"));

    expect(startTransferCalls.length).toBe(0);
    expect(await screen.findByText("Some items already exist")).toBeTruthy();

    await fireEvent.click(screen.getByText("Keep both"));

    await waitFor(() => expect(startTransferCalls.length).toBe(1));
    expect(startTransferCalls[0]).toEqual({
      sources: [`${PATH_ELSEWHERE}\\one.txt`, `${PATH_ELSEWHERE}\\two.txt`],
      dest: PATH_A,
      kind: "copy",
      policy: "keepboth",
    });
  });

  it("both buttons are absent once the Drop Stack is empty (nothing left to act on)", async () => {
    render(App);
    const driveButtons = await screen.findAllByText("Local Disk (C:)");
    await fireEvent.click(driveButtons[0]);
    await waitFor(() => expect(screen.getByText("existing.txt")).toBeTruthy());

    await fireEvent.click(screen.getByLabelText("Show Drop Stack"));

    expect(screen.queryByText("Move all here")).toBeNull();
    expect(screen.queryByText("Copy all here")).toBeNull();
  });
});

describe("App — doDropStackMoveAll is re-entrancy-safe against a double-fire click (CPE-1538, CPE-1385 parity)", () => {
  it("two rapid 'Move all here' clicks call moveEntries EXACTLY ONCE, not twice", async () => {
    await bootWithShelvedItems();

    // Swap in a move_entries handler backed by a promise WE control, so the assertion below can prove
    // the guard fires before the first click's moveEntries even resolves — not just "eventually settles
    // at 1" (same technique as App.clipboardPaneRouting.test.ts's CPE-1385 doPaste guard test).
    let resolveMove!: (v: { path: string; ok: boolean; error: string }[]) => void;
    const movePromise = new Promise<{ path: string; ok: boolean; error: string }[]>((res) => { resolveMove = res; });
    invoke.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      const listingFor = (path: unknown) => disks[path as string] ?? [];
      switch (cmd) {
        case "special_folders": return [];
        case "list_drives": return drives;
        case "home_dir": return "C:\\Users\\t";
        case "can_restore_from_trash": return true;
        case "list_dir": return listingFor(args.path);
        case "list_dir_stream": {
          const ch = args.onEntry as { onmessage: (b: unknown) => void };
          const data = listingFor(args.path);
          ch.onmessage(data);
          return data.length;
        }
        case "parent_dir": return null;
        case "read_file_text": return "";
        case "move_entries": {
          const paths = args.paths as string[];
          const dest = args.dest as string;
          moveEntriesCalls.push({ paths, dest });
          return movePromise; // stays pending until we resolve it below
        }
        default: return null;
      }
    });

    // Two rapid clicks, NOT awaited between them: `fireEvent.click` dispatches synchronously and
    // `doDropStackMoveAll` is invoked fire-and-forget from the button handler, so this reproduces the
    // exact CPE-1538 window — both calls run their synchronous prefix back-to-back, before either's
    // `await commands.moveEntries(...)` has a chance to settle.
    fireEvent.click(screen.getByText("Move all here"));
    fireEvent.click(screen.getByText("Move all here"));

    // Let any already-queued microtasks flush WITHOUT resolving `movePromise` — proves the second click
    // no-op'd via the `dropStackMoveInFlight` guard rather than merely "not yet reached" moveEntries.
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(moveEntriesCalls.length).toBe(1); // NOT 2 — the sync in-flight-flag guard did its job

    resolveMove([
      { path: `${PATH_A}\\one.txt`, ok: true, error: "" },
      { path: `${PATH_A}\\two.txt`, ok: true, error: "" },
    ]);
    await waitFor(() => expect(get(dropStackEntries)).toHaveLength(0)); // the single move completed + cleared the stack
  });
});
