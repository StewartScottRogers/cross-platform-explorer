// CPE-1600: a shared, best-effort "record this failed checkpoint attempt" helper — the single call site
// every "checkpoint before an irreversible batch" caller (BatchMediaDialog, MetadataStudioDialog,
// DeclutterDialog, SimilarImagesDialog) routes through from its existing `catch`, so a failed pre-write
// checkpoint gets a durable home in the Checkpoints panel (`checkpoint_failures_list`) instead of only
// the ~5s `showNotice` banner. One shared mechanism per the ticket's "prefer one shared mechanism over a
// per-dialog one" — a new caller of the same pattern only needs to add this one call.
import { commands } from "./bindings.gen";

/**
 * Best-effort record of a FAILED `checkpointCreate` attempt. Every existing caller already logs the
 * failure with `console.error` and proceeds with the write unblocked (a checkpoint is a bonus safety
 * net, never a gate) — call this ALONGSIDE that `console.error`, never in place of it, so the durable
 * record and the dev-console trace both exist independently.
 *
 * Deliberately swallows its OWN failure too: recording that a checkpoint failed touches the same
 * per-root store a broken checkpoint attempt just failed to write to (plausibly the same root cause —
 * e.g. an unwritable app-data dir), so a second failure here must never throw into an already-degraded
 * path or surface a second error to the user. It only logs and returns.
 */
export async function recordCheckpointFailure(root: string, operation: string, error: unknown): Promise<void> {
  const reason = error instanceof Error ? error.message : String(error);
  try {
    const res = await commands.checkpointRecordFailure(root, operation, reason);
    if (res.status === "error") {
      console.error("Failed to record checkpoint failure (best-effort, not surfaced further)", res.error);
    }
  } catch (e) {
    console.error("Failed to record checkpoint failure (best-effort, not surfaced further)", e);
  }
}
