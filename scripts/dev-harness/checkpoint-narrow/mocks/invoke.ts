// Dev-only mock for `src/lib/invoke.ts`, used ONLY by the CPE-1635 narrow-width verification harness
// (scripts/dev-harness/checkpoint-narrow). `unwrap` is copied verbatim from the real module (it's pure
// and tiny) so CheckpointDialog.svelte's `unwrap(await commands.xxx(...))` calls behave identically.
export function unwrap<T>(r: { status: "ok"; data: T } | { status: "error"; error: unknown }): T {
  if (r.status === "ok") return r.data;
  throw r.error instanceof Error ? r.error : new Error(String(r.error));
}
