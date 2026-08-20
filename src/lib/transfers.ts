// Frontend transfer state (CPE-622, epic CPE-613): folds the backend's `transfer://progress` and
// `transfer://done` events into a reactive list the operations panel renders. The reducer is pure +
// DOM-free so it's unit-tested; the store tail just wires the Tauri events. Idle by default — nothing
// is allocated until a transfer actually starts, so the plain explorer is unaffected.

import { writable, type Readable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import { commands } from "./bindings.gen"; // typed client (CPE-964)
import { unwrap } from "./invoke";

/** What produced a progress/report row (mirror of the Rust `TransferOp`) — a copy/move from the
 *  transfer engine, or an archive compress/extract now routed through the same queue (CPE-1184). */
export type TransferOp = "copy" | "move" | "compress" | "extract";

/** A progress snapshot from the backend engine (mirror of the Rust `TransferProgress`). */
export interface TransferProgress {
  id: number;
  op: TransferOp;
  total_bytes: number;
  done_bytes: number;
  total_items: number;
  done_items: number;
  current: string;
}

/** The final report of a transfer (mirror of the Rust `TransferReport`). */
export interface TransferReport {
  id: number;
  op: TransferOp;
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

/**
 * The notice an archive compress/extract must show when the backend **refused** one or more entries
 * (CPE-1775), or `null` when there is nothing new to say.
 *
 * ## What was wrong
 *
 * `extract_zip_archive_stream` and friends have always recorded a refusal in `TransferReport.errors`.
 * `App.svelte` read that array **only when `report.failed > 0`**, and a refused entry increments neither
 * `failed` nor (before this ticket) `skipped`. So the user got a plain *"1 item extracted"* success
 * toast for an archive whose second entry had just been refused as unsafe, with the count quietly one
 * lower than the archive's contents and the real message reachable only by hovering a small `· N error`
 * annotation in a panel they have no reason to open. That is the same experience as the bug CPE-1758 was
 * written to fix — extraction succeeded, the file is not there, nothing says why — with the mechanism
 * changed from "written somewhere invisible" to "refused and not mentioned".
 *
 * It also enables a partial-extraction deception: an archive with a benign entry A and a refused entry B
 * extracts A, drops B, and reports success, so the recipient believes they received both.
 *
 * ## Why the message carries only counts
 *
 * The reason strings embed the **archive-controlled entry name**, so they are attacker text. They belong
 * where they can be escaped and read at leisure — the operations panel, which now renders them through
 * `displaySafePath` behind a one-click disclosure — not spliced into a five-second toast. The headline's
 * job is to make the user *look*, and a count does that without giving an attacker a sentence in it.
 *
 * Returns `null` for a cancelled run (its own notice already says what happened), for a failed one (the
 * failure is the headline, and a skip must never be mistaken for a failure or vice versa), and for the
 * ordinary case of nothing skipped — so an unremarkable extraction gains no new noise.
 */
/**
 * The operations panel's disclosure-button text for a finished row (CPE-1775) — `null` when there is
 * nothing to disclose.
 *
 * Counts and literals only: the reason strings themselves embed an archive-controlled entry name and are
 * rendered through `displaySafePath` in the list this button opens, never in the button.
 *
 * **The number and the noun must come from the same field.** The first version read `errors.length` for
 * the count while branching on `skipped` for the noun. On the archive paths those are 1:1 by
 * `ArchiveReport::skip`'s invariant, so it looked right — but a copy/move row can carry conflict-skips
 * *and* a separate error line, and it would then have shown a number and a noun describing different
 * things. Lives here rather than inside the component so it can be tested without a render harness.
 */
export function transferReasonsLabel(
  r: Pick<TransferReport, "skipped" | "failed" | "errors"> | undefined,
): string | null {
  if (!r || r.errors.length === 0) return null;
  // A refused entry did NOT fail. Calling it a failure is the mirror of the success toast this ticket
  // removed, so the two read differently and each names its own count.
  if (r.skipped > 0 && r.failed === 0) return `· ${r.skipped} skipped — why?`;
  const n = r.errors.length;
  return `· ${n} problem${n === 1 ? "" : "s"} — why?`;
}

export function archiveSkipNotice(
  r: Pick<TransferReport, "op" | "transferred" | "skipped" | "failed" | "cancelled">,
  t: (key: string, params?: Record<string, string | number>) => string,
): string | null {
  if (r.cancelled || r.failed > 0 || r.skipped <= 0) return null;
  const compress = r.op === "compress";
  const doneKey = compress
    ? r.transferred === 1
      ? "notice.archiveCompressedOne"
      : "notice.archiveCompressedMany"
    : r.transferred === 1
      ? "notice.archiveExtractedOne"
      : "notice.archiveExtractedMany";
  const skipKey = r.skipped === 1 ? "notice.archiveSkippedOne" : "notice.archiveSkippedMany";
  return `${t(doneKey, { count: r.transferred })} ${t(skipKey, { count: r.skipped })}`;
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

/**
 * Start a copy/move; resolves to the new transfer's id. Progress arrives via the events above.
 *
 * `confirmed` (CPE-1662) is the backend's consent gate for the **overwrite** policy only — the one
 * policy whose collision handling deletes whatever already sits at the destination path (recursively,
 * for a folder). It is a **separate argument from `policy`** on purpose (the CPE-1646 lesson): the
 * policy is what the user chose, the flag is that they were actually asked. It therefore defaults to
 * `false`, so a caller that passes `"overwrite"` without routing through the conflict dialog is
 * rejected by the backend rather than quietly clobbering the destination. Skip/keep-both ignore it.
 */
export function startTransfer(
  sources: string[],
  dest: string,
  kind: TransferKind,
  policy: ConflictPolicy,
  confirmed = false,
): Promise<number> {
  return commands.startTransfer(sources, dest, kind, policy, confirmed).then(unwrap);
}

/** Ask a running transfer to stop at the next chunk boundary. Also cancels a queued archive
 *  compress/extract — they share the same backend registry (CPE-1184). */
export function cancelTransfer(id: number): Promise<void> {
  return commands.cancelTransfer(id);
}

/** Start a compress through the transfer queue (CPE-1184): resolves to the new transfer's id, throwing
 *  on an up-front failure (e.g. an empty selection). Progress/completion arrive via the same
 *  `transfer://progress`/`transfer://done` events copy/move use, tagged `op: "compress"`. `password`
 *  (non-empty) packs an AES-256 encrypted zip instead of the plain format picked by `dest`'s extension. */
export function startArchiveCompress(paths: string[], dest: string, password: string | null): Promise<number> {
  return commands.startArchiveCompress(paths, dest, password).then(unwrap);
}

/** Start an extract through the transfer queue (CPE-1184): resolves to the new transfer's id. A missing
 *  or wrong `password` for an encrypted zip rejects synchronously (before anything is queued), so the
 *  existing password-prompt-and-retry UX keeps its plain try/catch shape. Progress/completion arrive via
 *  the same events copy/move use, tagged `op: "extract"`. */
export function startArchiveExtract(path: string, dest: string, password: string | null): Promise<number> {
  return commands.startArchiveExtract(path, dest, password).then(unwrap);
}
