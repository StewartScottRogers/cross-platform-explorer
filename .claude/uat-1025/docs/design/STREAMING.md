# Streaming liveness — bulk data over an IPC channel

_Convention for producers of large or slow payloads (epic CPE-662)._

## What it is

Anything that can return **many rows** or take a **while to produce** (a directory listing, a recursive
name search, archive contents, thumbnails) streams its results to the frontend in **batches over a Tauri
`ipc::Channel`** instead of returning one big `Vec` from a blocking `invoke`. The pane paints the first
batch within a frame or two and fills in progressively, so a huge or slow folder stays interactive
instead of blocking on a blank view.

This outranks the plain-explorer *fast/small/predictable* tiebreaker only where it must: opening folders
is the core interaction, so a stall there feels broken. The hard constraint is that a **small** payload
must be no slower and no larger than the old one-shot path.

## The one rule: stream anything potentially large or slow

If a producer's output size is bounded and small, return a `Vec` and `invoke` it. If it can be large or
slow, **stream it**. Don't guess — a folder or a tree walk is unbounded, so it streams.

## Backend shape

Factor the work into **one walker** that both a collect-to-vec command and a streaming command call, so
they can never diverge. The walker takes a `flush` closure returning `ControlFlow` (so a streaming caller
can stop early), and skips unreadable entries rather than failing the whole job.

```rust
fn stream_dir_entries(
    path: &str,
    batch: usize,
    mut flush: impl FnMut(Vec<DirEntry>) -> std::ops::ControlFlow<()>,
) -> Result<usize, String> { /* read, buffer to `batch`, flush, honour Break */ }

// Collect-to-vec (tests, callers that need the whole list at once):
fn list_dir(path: String) -> Result<Vec<DirEntry>, String> {
    let mut out = Vec::new();
    stream_dir_entries(&path, LIST_DIR_BATCH, |b| { out.extend(b); ControlFlow::Continue(()) })?;
    Ok(out)
}

// Streaming command — pushes batches over the channel as they're read:
#[tauri::command]
fn list_dir_stream(path: String, stream_id: u64, on_entry: tauri::ipc::Channel<Vec<DirEntry>>)
    -> Result<usize, String> { /* flush → on_entry.send(batch) */ }
```

- **Batch size** is a small constant (256 entries for listings, 32 for search hits) — big enough that a
  tiny folder is one flush, small enough that the first rows show immediately.
- **Keep the synchronous command.** Tests and any collect-to-vec caller use it; both share the walker.

## Frontend shape

Open a `Channel`, append each batch, and **flip `loading` off on the first batch** (list/dialog views
gate their rows on `!loading`, so the first batch reveals them). Use `rawInvoke` — a stream shows its own
progress and must not also raise the busy cursor (see [BUSY-CURSOR.md](BUSY-CURSOR.md)). Guard with a
**generation token** so a newer load supersedes an in-flight one and stale batches are dropped.

```ts
import { Channel } from "@tauri-apps/api/core";
import { rawInvoke } from "../invoke";

const gen = ++loadGen;
entries = [];
loading = true;
const channel = new Channel<DirEntry[]>();
channel.onmessage = (batch) => {
  if (gen !== loadGen) return;          // superseded — drop stale rows
  entries = entries.concat(batch);       // reactive pipeline re-sorts as it grows
  loading = false;                       // first real rows are in — reveal them
};
await rawInvoke("list_dir_stream", { path, streamId: gen, onEntry: channel });
```

**Sorting is free.** The reactive `visible = sortEntries(entries, …)` re-derives every time `entries`
grows, so the final order always matches the one-shot path — no backend sort, no row-jump special-casing.
In-place insertion as batches land is accepted (it matches OS explorers).

## Cancellation (optional, per producer)

To stop the backend *walking* a payload the user has navigated away from (not just ignore its batches),
register a cancel flag keyed by the frontend-supplied `stream_id` and poll it in the `flush` closure —
mirroring the transfer cancel registry:

```rust
static DIR_STREAM_CANCELS: OnceLock<Mutex<HashMap<u64, Arc<AtomicBool>>>> = OnceLock::new();

// in the flush closure: if cancel.load(Relaxed) { ControlFlow::Break(()) } else { Continue(()) }

#[tauri::command]
fn cancel_dir_stream(stream_id: u64) { /* set the flag; no-op if already finished */ }
```

The frontend fires `cancel_dir_stream(prevGen)` when a new navigation supersedes the previous stream.
Cancellation is worth adding when the walk is unbounded (directory listings); a producer already capped
by match/size limits (name search) can skip it.

## Implementations

| Producer | Command | Walker | Cancel |
|----------|---------|--------|--------|
| Directory listing | `list_dir_stream` (`list_dir` collects) | `stream_dir_entries` | `cancel_dir_stream` (CPE-665) |
| Filename search | `find_files_by_name_stream` (`find_files_by_name` collects) | `walk_name_matches` | capped, no cancel |

New bulk producers follow this shape: one shared walker, a streaming command over an `ipc::Channel`, a
frontend `Channel` subscription that flips `loading` on the first batch and supersedes by generation.
