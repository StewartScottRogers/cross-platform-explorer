// Frontend thumbnail client (CPE-1237, epic CPE-718): drives the backend's `thumb_queue` priority
// scheduler (CPE-950, Visible > Prefetch > Background) + `thumb_cache` in-memory LRU (CPE-939) via the
// `thumbnails_stream` command, wired up in CPE-1237 (`crates/server/src/thumb_pipeline.rs`). Replaces
// `ThumbnailImage.svelte`'s old naive "one `invoke("thumbnail", …)` per tile, whenever it scrolls near
// the viewport" behaviour, which never gave the backend queue anything to actually schedule (one request
// in flight is not a queue).
//
// The key idea: every `requestThumbnail()` call queues into a *shared, module-level* pending list and
// resolves in the next microtask. Every `ThumbnailImage` that mounts/observes within the same tick — e.g.
// a whole freshly-rendered virtualization window (CPE-690) — piles onto that list before it's flushed, so
// ONE `thumbnails_stream` call carries the whole batch and the backend's priority queue has real work to
// arbitrate (Visible tiles decoded before Prefetch ones). A cache hit resolves synchronously — no
// backend round-trip — satisfying "feature-off/no-thumbnail formats incur no cost" for a repeat render.
//
// Cancellation: a newer batch cancels whatever the previous flush is still draining (the previous visible
// window is no longer relevant), and any of that cancelled batch's requests left unresolved are folded
// into the *next* flush automatically — so a tile that got caught mid-cancellation still eventually
// resolves rather than hanging forever.

import { rawInvoke, createChannel } from "./invoke";

/** Mirrors the Rust `thumb_queue::Priority` enum (CPE-950). */
export type ThumbPriority = "visible" | "prefetch" | "background";

/** Mirrors the Rust `thumb_pipeline::ThumbResult` streamed by `thumbnails_stream`. */
interface ThumbStreamResult {
  path: string;
  target_px: number;
  data_url: string | null;
}

interface QueuedReq {
  path: string;
  targetPx: number;
  priority: ThumbPriority;
  resolve: (dataUrl: string | null) => void;
  /** How many prior flushes already tried (and failed to resolve) this exact request. Bounds the
   *  cancel-and-reflush re-queue below to a handful of turns, so a batch that keeps completing without
   *  ever actually caching anything (a backend that no-ops, a broken/unmocked transport in a test) can't
   *  spin the microtask queue forever — it gives up and resolves `null` instead. */
  attempt: number;
}

const PRIORITY_RANK: Record<ThumbPriority, number> = { visible: 0, prefetch: 1, background: 2 };

/** Cap on re-queue attempts for a request left unresolved by a "completed" (not hard-failed) batch — see
 *  `QueuedReq.attempt`. A real cancel-then-supersede chain resolves within one or two turns; this is a
 *  safety net, not an expected steady-state path. */
const MAX_REQUEUE_ATTEMPTS = 4;

// Module-level shared state — deliberately a singleton, not per-component: the whole point is that
// concurrently-mounting tiles across the app share ONE queue+cache, mirroring the single shared
// `ThumbCacheService` the backend holds in Tauri-managed state.
const cache = new Map<string, string | null>();
let pending: QueuedReq[] = [];
let flushScheduled = false;
let currentStreamId: number | null = null;
let seq = 0;

/** The cache key a request resolves under: path + the rounded target edge (a different tile size is a
 *  different thumbnail — mirrors the backend's `ThumbKey`, which also folds in mtime/size so an edited
 *  file is a fresh key server-side; the client only needs enough to avoid a same-session re-request). */
export function thumbKey(path: string, targetPx: number): string {
  return `${path}::${Math.round(targetPx)}`;
}

/** The currently-known thumbnail for a tile, if any request for it has resolved this session.
 *  `undefined` = never requested (or still in flight) — `null` = resolved, but the format can't be
 *  rendered (icon fallback). */
export function cachedThumbnail(path: string, targetPx: number): string | null | undefined {
  return cache.get(thumbKey(path, targetPx));
}

function scheduleFlush(): void {
  if (flushScheduled) return;
  flushScheduled = true;
  queueMicrotask(() => void flush());
}

async function flush(): Promise<void> {
  flushScheduled = false;
  const batch = pending;
  pending = [];
  if (batch.length === 0) return;

  // A newer batch supersedes whatever the previous flush's `thumbnails_stream` call is still draining —
  // its remaining (lower-priority / scrolled-away) work stops competing with this one.
  if (currentStreamId !== null) {
    void rawInvoke("cancel_thumbnails_stream", { streamId: currentStreamId });
  }
  const streamId = ++seq;
  currentStreamId = streamId;

  // De-dupe within this batch (the same tile requested by more than one caller, or by both the prefetch-
  // margin and strict-visible observers, in the same tick): keep every resolver, and the HIGHEST priority
  // anyone asked for.
  const byKey = new Map<
    string,
    {
      path: string;
      targetPx: number;
      priority: ThumbPriority;
      attempt: number;
      resolvers: ((v: string | null) => void)[];
    }
  >();
  for (const req of batch) {
    const key = thumbKey(req.path, req.targetPx);
    const existing = byKey.get(key);
    if (existing) {
      existing.resolvers.push(req.resolve);
      if (PRIORITY_RANK[req.priority] < PRIORITY_RANK[existing.priority]) existing.priority = req.priority;
      existing.attempt = Math.max(existing.attempt, req.attempt);
    } else {
      byKey.set(key, {
        path: req.path,
        targetPx: req.targetPx,
        priority: req.priority,
        attempt: req.attempt,
        resolvers: [req.resolve],
      });
    }
  }
  const requests = [...byKey.values()];

  let completed = false;
  try {
    const channel = createChannel<ThumbStreamResult>();
    channel.onmessage = (r) => {
      const key = thumbKey(r.path, r.target_px);
      cache.set(key, r.data_url);
      byKey.get(key)?.resolvers.forEach((resolve) => resolve(r.data_url));
    };
    await rawInvoke("thumbnails_stream", {
      requests: requests.map((r) => ({ path: r.path, target_px: Math.round(r.targetPx), priority: r.priority })),
      streamId,
      onThumb: channel,
    });
    completed = true; // the command settled normally — a partial drain here means a genuine supersede/cancel
  } catch {
    // A hard infra-level failure (transport/channel setup, or the command call itself — not a per-tile
    // decode failure; those stream back as `data_url: null` via `onmessage` above and never reach here).
    // Resolved as a fallback below, once, rather than retried — an environment that can't stream at all
    // won't start streaming on a retry, so looping would just spin forever.
  } finally {
    if (currentStreamId === streamId) currentStreamId = null;
  }

  for (const r of requests) {
    const key = thumbKey(r.path, r.targetPx);
    if (cache.has(key)) continue;
    if (completed && r.attempt < MAX_REQUEUE_ATTEMPTS) {
      // Genuinely cancelled mid-drain (a newer batch superseded this one) — give it one more turn in the
      // next flush instead of leaving its caller's promise hanging forever.
      pending.push({
        path: r.path,
        targetPx: r.targetPx,
        priority: r.priority,
        attempt: r.attempt + 1,
        resolve: (v) => r.resolvers.forEach((resolve) => resolve(v)),
      });
    } else {
      // Either a hard failure (give up so callers resolve to the icon fallback instead of hanging; a
      // future fresh request for the same tile — e.g. it scrolls out and back in — tries again from
      // scratch), or `MAX_REQUEUE_ATTEMPTS` genuinely-cancelled re-queues in a row without ever landing a
      // result — stop retrying rather than spin the microtask queue indefinitely.
      r.resolvers.forEach((resolve) => resolve(null));
    }
  }
  if (pending.length > 0) scheduleFlush();
}

/**
 * Request one tile's thumbnail at `priority`. Resolves immediately from the shared cache on a hit (no
 * backend round-trip — repeat renders / cache-hit tiles cost nothing); otherwise joins the next
 * microtask's batch. `null` means the format can't be rendered — the caller falls back to its type icon.
 */
export function requestThumbnail(path: string, targetPx: number, priority: ThumbPriority): Promise<string | null> {
  const cached = cachedThumbnail(path, targetPx);
  if (cached !== undefined) return Promise.resolve(cached);
  return new Promise<string | null>((resolve) => {
    pending.push({ path, targetPx, priority, resolve, attempt: 0 });
    scheduleFlush();
  });
}

/** Test-only: reset all shared module state between vitest cases (the cache/queue are intentionally a
 *  singleton in production, which a test file must be able to isolate between cases). */
export function _resetThumbnailClientForTests(): void {
  cache.clear();
  pending = [];
  flushScheduled = false;
  currentStreamId = null;
  seq = 0;
}
