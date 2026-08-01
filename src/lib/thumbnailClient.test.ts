/**
 * Tests for the frontend thumbnail streaming client (CPE-1237, epic CPE-718) — the module that drives the
 * backend's `thumb_queue` priority scheduler + `thumb_cache` LRU via `thumbnails_stream`, replacing
 * `ThumbnailImage.svelte`'s old naive one-shot-per-tile `invoke("thumbnail", …)`.
 *
 * Mirrors the repo's established streaming-client mocking (`InstantSearch.test.ts`,
 * `BatchMediaDialog.test.ts`): mock `@tauri-apps/api/core`'s `invoke` + `Channel`, since `rawInvoke`/
 * `createChannel` (`./invoke`) ultimately flow through that module. Backend calls are queued as
 * manually-resolvable deferreds so each test controls exactly when a batch "completes".
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

interface Deferred {
  args: any;
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
}

let streamCalls: Deferred[] = [];
let cancelledStreamIds: number[] = [];

const invoke = vi.fn((cmd: string, args?: any) => {
  if (cmd === "thumbnails_stream") {
    return new Promise((resolve, reject) => streamCalls.push({ args, resolve, reject }));
  }
  if (cmd === "cancel_thumbnails_stream") {
    cancelledStreamIds.push(args.streamId);
    return Promise.resolve();
  }
  return Promise.reject(new Error(`unexpected command: ${cmd}`));
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

// Imported AFTER the mock is registered (vi.mock is hoisted by vitest, so this is safe either way).
import { requestThumbnail, cachedThumbnail, thumbKey, _resetThumbnailClientForTests } from "./thumbnailClient";

/** Drain the microtask queue so `queueMicrotask(flush)` has run. */
async function flushMicrotasks(): Promise<void> {
  for (let i = 0; i < 8; i++) await Promise.resolve();
}

function resolveBatch(call: Deferred, results: { path: string; target_px: number; data_url: string | null }[]) {
  for (const r of results) call.args.onThumb.onmessage(r);
  call.resolve(results.length);
}

beforeEach(() => {
  invoke.mockClear();
  streamCalls = [];
  cancelledStreamIds = [];
  _resetThumbnailClientForTests();
});

describe("thumbnailClient batching (CPE-1237)", () => {
  it("batches every tile requested in the same tick into one thumbnails_stream call", async () => {
    const p1 = requestThumbnail("/a.png", 96, "visible");
    const p2 = requestThumbnail("/b.png", 96, "visible");
    await flushMicrotasks();

    expect(streamCalls).toHaveLength(1);
    expect(streamCalls[0].args.requests).toEqual(
      expect.arrayContaining([
        { path: "/a.png", target_px: 96, priority: "visible" },
        { path: "/b.png", target_px: 96, priority: "visible" },
      ]),
    );

    resolveBatch(streamCalls[0], [
      { path: "/a.png", target_px: 96, data_url: "data:a" },
      { path: "/b.png", target_px: 96, data_url: "data:b" },
    ]);
    expect(await p1).toBe("data:a");
    expect(await p2).toBe("data:b");
  });

  it("requests visible tiles at Visible priority and prefetch-margin tiles at Prefetch priority", async () => {
    requestThumbnail("/vis.png", 64, "visible");
    requestThumbnail("/pf.png", 64, "prefetch");
    await flushMicrotasks();

    const reqs: { path: string; target_px: number; priority: string }[] = streamCalls[0].args.requests;
    expect(reqs.find((r) => r.path === "/vis.png")?.priority).toBe("visible");
    expect(reqs.find((r) => r.path === "/pf.png")?.priority).toBe("prefetch");
  });

  it("upgrades a tile requested at both priorities in one tick to Visible (the stronger ask wins)", async () => {
    requestThumbnail("/dup.png", 64, "prefetch");
    requestThumbnail("/dup.png", 64, "visible");
    await flushMicrotasks();

    // De-duped into ONE backend request for the key, at the higher (Visible) priority.
    const reqs: { path: string; priority: string }[] = streamCalls[0].args.requests;
    expect(reqs.filter((r) => r.path === "/dup.png")).toHaveLength(1);
    expect(reqs.find((r) => r.path === "/dup.png")?.priority).toBe("visible");
  });
});

describe("thumbnailClient cancellation of superseded batches (CPE-1237)", () => {
  it("cancels the previous in-flight stream when a newer batch supersedes it", async () => {
    requestThumbnail("/first.png", 64, "prefetch");
    await flushMicrotasks();
    expect(streamCalls).toHaveLength(1);
    expect(cancelledStreamIds).toEqual([]); // nothing to cancel yet

    // The first batch never resolved (simulates it still draining low-priority work) — a new visible
    // window arrives and issues a second batch before the first finished.
    requestThumbnail("/second.png", 64, "visible");
    await flushMicrotasks();

    expect(streamCalls).toHaveLength(2);
    expect(cancelledStreamIds).toEqual([streamCalls[0].args.streamId]);
  });

  it("re-queues a request left unresolved by a cancelled batch into the next flush", async () => {
    const p1 = requestThumbnail("/stuck.png", 64, "prefetch");
    await flushMicrotasks();
    const firstCall = streamCalls[0];

    // Supersede it before it resolves.
    requestThumbnail("/other.png", 64, "visible");
    await flushMicrotasks();

    // The FIRST batch's underlying command now completes (cooperative cancellation: the backend stopped
    // early and returned without ever emitting "/stuck.png") — its promise settles empty-handed.
    firstCall.resolve(0);
    await flushMicrotasks();

    // "/stuck.png" was folded into a follow-up batch rather than left hanging forever.
    const laterCall = streamCalls.find(
      (c) => c !== firstCall && c.args.requests.some((r: any) => r.path === "/stuck.png"),
    );
    expect(laterCall).toBeDefined();
    resolveBatch(laterCall!, [{ path: "/stuck.png", target_px: 64, data_url: "data:stuck" }]);
    expect(await p1).toBe("data:stuck");
  });
});

describe("thumbnailClient retry-cap exhaustion (CPE-1239 regression pin for CPE-1237's OOM fix)", () => {
  it("resolves null after exactly MAX_REQUEUE_ATTEMPTS requeues when a batch keeps 'completing' without " +
    "ever yielding that key, without spinning, and issues no thumbnails_stream calls past the cap", async () => {
    // Simulates the exact bug CPE-1237 fixed: a `thumbnails_stream` call that settles ("completes")
    // normally but never emits an `onThumb` message for this key — e.g. a backend that reports the batch
    // done without ever actually resolving this particular tile. Without the requeue cap, the client would
    // fold the still-unresolved request into the next flush forever (unbounded microtask spin -> OOM).
    const p1 = requestThumbnail("/never-yields.png", 64, "visible");
    await flushMicrotasks();
    expect(streamCalls).toHaveLength(1);

    // Drive it through every requeue: each call "completes" with zero results (no onThumb message at all)
    // for "/never-yields.png". 1 initial attempt + MAX_REQUEUE_ATTEMPTS (4) requeues = 5 total calls before
    // the client gives up.
    for (let i = 0; i < 5; i++) {
      expect(streamCalls).toHaveLength(i + 1); // exactly one new call appeared per requeue, no extra spin
      streamCalls[i].resolve(0); // "completed" -- settled normally, but nothing was ever yielded for the key
      await flushMicrotasks();
    }

    expect(streamCalls).toHaveLength(5);
    expect(await p1).toBeNull(); // icon fallback, not a hang

    // Cap reached: further microtask turns issue no additional thumbnails_stream calls.
    await flushMicrotasks();
    await flushMicrotasks();
    expect(streamCalls).toHaveLength(5);

    // A gave-up request was never cached (unlike a genuine null decode result) -- a fresh future request
    // for the same tile tries again from scratch rather than being permanently stuck at null.
    expect(cachedThumbnail("/never-yields.png", 64)).toBeUndefined();
  });
});

describe("thumbnailClient cache (CPE-1237)", () => {
  it("resolves a cache hit synchronously, without a new backend call", async () => {
    const p1 = requestThumbnail("/x.png", 96, "visible");
    await flushMicrotasks();
    resolveBatch(streamCalls[0], [{ path: "/x.png", target_px: 96, data_url: "data:x" }]);
    expect(await p1).toBe("data:x");

    const callsBefore = streamCalls.length;
    const p2 = requestThumbnail("/x.png", 96, "visible");
    expect(await p2).toBe("data:x");
    expect(streamCalls).toHaveLength(callsBefore); // no new thumbnails_stream call for the repeat request
    expect(cachedThumbnail("/x.png", 96)).toBe("data:x");
  });

  it("caches a null result (unsupported/undecodable format) so a repeat request also avoids the backend", async () => {
    const p1 = requestThumbnail("/broken.svg", 96, "visible");
    await flushMicrotasks();
    resolveBatch(streamCalls[0], [{ path: "/broken.svg", target_px: 96, data_url: null }]);
    expect(await p1).toBeNull();

    const callsBefore = streamCalls.length;
    const p2 = requestThumbnail("/broken.svg", 96, "visible");
    expect(await p2).toBeNull();
    expect(streamCalls).toHaveLength(callsBefore);
  });

  it("keys the cache by path AND target size, so a different tile size is a fresh request", async () => {
    expect(thumbKey("/a.png", 48)).not.toBe(thumbKey("/a.png", 128));

    const p1 = requestThumbnail("/a.png", 48, "visible");
    await flushMicrotasks();
    resolveBatch(streamCalls[0], [{ path: "/a.png", target_px: 48, data_url: "data:small" }]);
    await p1;

    const p2 = requestThumbnail("/a.png", 128, "visible");
    await flushMicrotasks();
    // A second, distinct backend request was needed — the 48px cache entry didn't satisfy the 128px ask.
    const call128 = streamCalls.find((c) => c.args.requests.some((r: any) => r.target_px === 128));
    expect(call128).toBeDefined();
    resolveBatch(call128!, [{ path: "/a.png", target_px: 128, data_url: "data:big" }]);
    expect(await p2).toBe("data:big");
  });
});
