/**
 * Component tests for the disk-usage treemap (CPE-751, epic CPE-706) — untested state machines flagged
 * by CPE-1404: the per-path scan cache (Up / re-drill must never re-walk), the generation-token supersede
 * guard on the streaming channel (a stale batch/resolve from a superseded scan must be dropped), and the
 * `refreshToken` reactive re-scan (after a delete, the current folder's cache entry is busted and re-walked).
 *
 * Mirrors `FileNameSearchDialog.test.ts` (CPE-1401, same gen-token pattern): mock `@tauri-apps/api/core`'s
 * `invoke` + `Channel`, since `rawInvoke`/`createChannel` (`../invoke`) flow through that module under the
 * local transport. `dir_children_sizes_stream` calls are queued as manually-resolvable deferreds so a test
 * can control arrival order independently of call order — the whole point of the supersede guard.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import DiskSpaceView from "./DiskSpaceView.svelte";

interface Child {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
}

interface Deferred {
  args: any;
  resolve: (v: undefined) => void;
  reject: (e: unknown) => void;
}

let calls: Deferred[] = [];

const invoke = vi.fn((cmd: string, args?: any) => {
  if (cmd === "dir_children_sizes_stream") {
    return new Promise<undefined>((resolve, reject) => calls.push({ args, resolve, reject }));
  }
  return Promise.reject(new Error(`unexpected command: ${cmd}`));
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

/** Flush the microtask hops a manually-resolved deferred needs before Svelte's DOM-update cycle. */
async function settle() {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  await tick();
}

const child = (name: string, path: string, is_dir: boolean, size: number): Child => ({
  name,
  path,
  is_dir,
  size,
});

const rowTitles = () => Array.from(document.querySelectorAll(".lg-row")).map((el) => el.getAttribute("title"));
const rowByPath = (path: string) => screen.getByTitle(path) as HTMLButtonElement;
const upButton = () => screen.getByTitle("Go up to the parent folder") as HTMLButtonElement;
const totalText = () => document.querySelector(".sp-total")?.textContent ?? "";

beforeEach(() => {
  invoke.mockClear();
  calls = [];
});

describe("DiskSpaceView — per-path scan cache (CPE-751)", () => {
  it("scans on mount, then reuses the cache for a path visited before instead of re-invoking", async () => {
    render(DiskSpaceView, { path: "Z:\\root" });
    await settle();
    expect(calls).toHaveLength(1);
    expect(calls[0].args).toEqual(expect.objectContaining({ path: "Z:\\root" }));
    expect(screen.getByText(/scanning/)).toBeTruthy();

    // Deliver the root's children and finalize the scan (this is what populates cache["Z:\root"]).
    calls[0].args.onChild.onmessage([
      child("sub", "Z:\\root\\sub", true, 200),
      child("file.txt", "Z:\\root\\file.txt", false, 50),
    ]);
    await settle();
    expect(screen.queryByText(/scanning/)).toBeNull();
    expect(rowTitles()).toEqual(["Z:\\root\\sub", "Z:\\root\\file.txt"]);
    calls[0].resolve(undefined);
    await settle();

    // Drill into "sub" — not cached yet, so a fresh scan is invoked.
    await fireEvent.click(rowByPath("Z:\\root\\sub"));
    await settle();
    expect(calls).toHaveLength(2);
    expect(calls[1].args).toEqual(expect.objectContaining({ path: "Z:\\root\\sub" }));
    calls[1].args.onChild.onmessage([child("deep.txt", "Z:\\root\\sub\\deep.txt", false, 10)]);
    await settle();
    calls[1].resolve(undefined);
    await settle();
    expect(rowTitles()).toEqual(["Z:\\root\\sub\\deep.txt"]);

    // Go back Up to the root — this path was already fully scanned, so it must be a CACHE HIT: no
    // third invoke, and the cached children reappear immediately (no scanning flicker).
    await fireEvent.click(upButton());
    await settle();
    expect(calls).toHaveLength(2); // <-- no re-invoke
    expect(screen.queryByText(/scanning/)).toBeNull();
    expect(rowTitles()).toEqual(["Z:\\root\\sub", "Z:\\root\\file.txt"]);
  });
});

describe("DiskSpaceView — streaming batch accumulation (CPE-706)", () => {
  it("accumulates batches sorted by size descending and flips the scanning indicator off on the first batch", async () => {
    render(DiskSpaceView, { path: "Z:\\root" });
    await settle();
    expect(calls).toHaveLength(1);
    expect(screen.getByText(/scanning/)).toBeTruthy();

    calls[0].args.onChild.onmessage([child("small.txt", "Z:\\root\\small.txt", false, 5)]);
    await settle();
    expect(screen.queryByText(/scanning/)).toBeNull();
    expect(rowTitles()).toEqual(["Z:\\root\\small.txt"]);

    // A second, larger batch arrives — accumulates AND re-sorts, doesn't just append.
    calls[0].args.onChild.onmessage([child("big.txt", "Z:\\root\\big.txt", false, 500)]);
    await settle();
    expect(rowTitles()).toEqual(["Z:\\root\\big.txt", "Z:\\root\\small.txt"]);

    calls[0].resolve(undefined);
    await settle();
    expect(totalText()).toContain("505 B");
  });

  it("surfaces a rejected scan as an error message", async () => {
    render(DiskSpaceView, { path: "Z:\\root" });
    await settle();
    calls[0].reject(new Error("permission denied"));
    await settle();
    expect(screen.getByText("Couldn't scan this folder: Error: permission denied")).toBeTruthy();
  });
});

describe("DiskSpaceView — generation-token supersede (the key regression risk)", () => {
  it("drops a stale batch delivered on a superseded scan's channel; only the newer scan's results render", async () => {
    const { component } = render(DiskSpaceView, { path: "Z:\\root", refreshToken: 0 });
    await settle();
    expect(calls).toHaveLength(1);
    const scanA = calls[0];

    // Bump refreshToken before A delivers anything — the reactive block busts cache["Z:\root"] and
    // calls scan() again, which bumps the generation token and starts scan B, superseding A's channel.
    await component.$set({ refreshToken: 1 });
    await settle();
    expect(calls).toHaveLength(2);
    const scanB = calls[1];
    expect(scanB.args).toEqual(expect.objectContaining({ path: "Z:\\root" }));

    // A's late-arriving batch must be DROPPED, not merged into the (reset) children list.
    scanA.args.onChild.onmessage([child("a-stale.txt", "Z:\\root\\a-stale.txt", false, 999)]);
    await settle();
    expect(rowTitles()).toEqual([]);
    expect(screen.getByText(/scanning/)).toBeTruthy(); // still cold — B hasn't delivered yet

    // B's own batch renders normally.
    scanB.args.onChild.onmessage([child("b-fresh.txt", "Z:\\root\\b-fresh.txt", false, 42)]);
    await settle();
    expect(rowTitles()).toEqual(["Z:\\root\\b-fresh.txt"]);

    // A resolving late must also be dropped — it must not write cache["Z:\root"] over B's result nor
    // flip loading back off/on incorrectly.
    scanA.resolve(undefined);
    await settle();
    expect(rowTitles()).toEqual(["Z:\\root\\b-fresh.txt"]);

    // B resolving normally finalizes the cache with B's own data.
    scanB.resolve(undefined);
    await settle();
    expect(rowTitles()).toEqual(["Z:\\root\\b-fresh.txt"]);
  });

  it("drops a stale scan's rejection from clobbering a newer scan that's still in flight", async () => {
    const { component } = render(DiskSpaceView, { path: "Z:\\root", refreshToken: 0 });
    await settle();
    expect(calls).toHaveLength(1);
    const scanA = calls[0];

    await component.$set({ refreshToken: 1 });
    await settle();
    expect(calls).toHaveLength(2);
    const scanB = calls[1];

    // A rejects after being superseded — must not surface as an error over B's still-loading state.
    scanA.reject(new Error("stale failure"));
    await settle();
    expect(screen.queryByText(/Couldn't scan this folder/)).toBeNull();
    expect(screen.getByText(/scanning/)).toBeTruthy();

    scanB.args.onChild.onmessage([child("b-fresh.txt", "Z:\\root\\b-fresh.txt", false, 42)]);
    await settle();
    expect(rowTitles()).toEqual(["Z:\\root\\b-fresh.txt"]);
    scanB.resolve(undefined);
    await settle();
    expect(screen.queryByText(/Couldn't scan this folder/)).toBeNull();
  });
});

describe("DiskSpaceView — refreshToken re-scan busts the cache (CPE-752)", () => {
  it("bumping refreshToken forces a fresh, uncached scan of the same folder and replaces (not merges) its children", async () => {
    const { component } = render(DiskSpaceView, { path: "Z:\\root", refreshToken: 0 });
    await settle();
    expect(calls).toHaveLength(1);

    // Finish the initial scan so the path is cached.
    calls[0].args.onChild.onmessage([child("old.txt", "Z:\\root\\old.txt", false, 5)]);
    await settle();
    calls[0].resolve(undefined);
    await settle();
    expect(rowTitles()).toEqual(["Z:\\root\\old.txt"]);

    // Now bump refreshToken — this must bust the cache and re-invoke, NOT reuse cache["Z:\root"].
    await component.$set({ refreshToken: 1 });
    await settle();
    expect(calls).toHaveLength(2);
    expect(calls[1].args).toEqual(expect.objectContaining({ path: "Z:\\root" }));

    // While the re-scan is cold, children reset to empty (the old cached "old.txt" row disappears —
    // this is a real re-walk, not a cache replay) before the fresh batch lands.
    expect(rowTitles()).toEqual([]);

    calls[1].args.onChild.onmessage([child("new.txt", "Z:\\root\\new.txt", false, 7)]);
    await settle();
    calls[1].resolve(undefined);
    await settle();
    // The fresh scan's result replaces the stale cached one entirely.
    expect(rowTitles()).toEqual(["Z:\\root\\new.txt"]);

    // A subsequent identical bump does nothing further (refreshToken didn't change since lastRefresh).
    await component.$set({ refreshToken: 1 });
    await settle();
    expect(calls).toHaveLength(2);
  });
});
