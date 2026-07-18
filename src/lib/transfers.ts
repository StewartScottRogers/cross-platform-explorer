// Frontend transfer state (CPE-622, epic CPE-613): folds the backend's `transfer://progress` and
// `transfer://done` events into a reactive list the operations panel renders. The reducer is pure +
// DOM-free so it's unit-tested; the store tail just wires the Tauri events. Idle by default — nothing
// is allocated until a transfer actually starts, so the plain explorer is unaffected.

import { writable, type Readable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "./invoke";

/** A progress snapshot from the backend engine (mirror of the Rust `TransferProgress`). */
export interface TransferProgress {
  id: number;
  total_bytes: number;
  done_bytes: number;
  total_items: number;
  done_items: number;
  current: string;
}

/** The final report of a transfer (mirror of the Rust `TransferReport`). */
export interface TransferReport {
  id: number;
  transferred: number;
  skipped: number;
  failed: number;
  cancelled: boolean;
  errors: string[];
}

/** One row in the operations panel: the latest progress, plus its final report once finished. */
export interface TransferState extends TransferProgress {
  finished: boolean;
  report?: TransferReport;
}

/** Whole-batch completion percentage (0–100), by bytes; a finished transfer is always 100. */
export function percent(t: TransferState): number {
  if (t.finished) return 100;
  if (t.total_bytes <= 0) return t.total_items > 0 ? Math.round((t.done_items / t.total_items) * 100) : 0;
  return Math.min(100, Math.round((t.done_bytes / t.total_bytes) * 100));
}

/** Fold a progress event into the list: update the matching transfer or append a new one. Pure. */
export function upsertProgress(list: TransferState[], p: TransferProgress): TransferState[] {
  const row: TransferState = { ...p, finished: false };
  return list.some((t) => t.id === p.id)
    ? list.map((t) => (t.id === p.id ? { ...row, report: t.report } : t))
    : [...list, row];
}

/** Mark a transfer finished, attaching its report. Pure. (Ignores an unknown id.) */
export function markFinished(list: TransferState[], r: TransferReport): TransferState[] {
  return list.map((t) => (t.id === r.id ? { ...t, finished: true, current: "", report: r } : t));
}

/** Remove a transfer (e.g. the user dismissed a finished row). Pure. */
export function dismiss(list: TransferState[], id: number): TransferState[] {
  return list.filter((t) => t.id !== id);
}

/**
 * The base names among `sources` that already exist in `existing` (the destination folder's entry
 * names) — i.e. copying these here would collide. Pure; drives the conflict chooser. Exact-match (a
 * case-only difference just falls through to keep-both auto-rename, which is harmless).
 */
export function collidingNames(sources: string[], existing: string[]): string[] {
  const set = new Set(existing);
  const out: string[] = [];
  for (const s of sources) {
    const base = s.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? s;
    if (set.has(base)) out.push(base);
  }
  return out;
}

const store = writable<TransferState[]>([]);

/** Reactive list of active + just-finished transfers (empty when idle). */
export const transfers: Readable<TransferState[]> = store;

let started = false;
/** Subscribe to the backend transfer events once (idempotent). Call at app start. */
export async function initTransfers(): Promise<void> {
  if (started) return;
  started = true;
  await listen<TransferProgress>("transfer://progress", (e) => store.update((l) => upsertProgress(l, e.payload)));
  await listen<TransferReport>("transfer://done", (e) => store.update((l) => markFinished(l, e.payload)));
}

/** Drop a finished transfer from the panel. */
export function dismissTransfer(id: number): void {
  store.update((l) => dismiss(l, id));
}

export type TransferKind = "copy" | "move";
export type ConflictPolicy = "overwrite" | "skip" | "keepboth";

/** Start a copy/move; resolves to the new transfer's id. Progress arrives via the events above. */
export function startTransfer(sources: string[], dest: string, kind: TransferKind, policy: ConflictPolicy): Promise<number> {
  return invoke<number>("start_transfer", { sources, dest, kind, policy });
}

/** Ask a running transfer to stop at the next chunk boundary. */
export function cancelTransfer(id: number): Promise<void> {
  return invoke("cancel_transfer", { id });
}
