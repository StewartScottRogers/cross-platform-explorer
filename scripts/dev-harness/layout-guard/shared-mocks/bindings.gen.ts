// CPE-1882 — shared dev-only mock for `src/lib/bindings.gen.ts`, aliased in for every
// scripts/dev-harness/layout-guard case (see ../../../vite.harness.layout-guard.config.ts). Generic on
// purpose: ANY `commands.xxx(...)` call resolves `{ status: "ok", data: undefined }`, so a component
// that only calls `commands.*` from a click handler (never before the harness has taken its own
// measurement) needs no per-command wiring at all. That covers every layout-guard case so far —
// TrashView.svelte's `commands.restoreTrashItems`/`commands.emptyTrash` are both click-triggered, never
// awaited before first paint.
//
// If a future case genuinely needs a specific `commands.*` return value BEFORE its first paint (unlike
// every case today), give IT its own case-scoped mock + alias instead of special-casing that command
// here — see scripts/dev-harness/checkpoint-narrow/mocks/bindings.gen.ts for that bespoke pattern, still
// the right tool when a generic stub can't say enough.
export const commands: Record<string, (...args: unknown[]) => Promise<{ status: "ok"; data: undefined }>> =
  new Proxy(
    {},
    {
      get: () => async () => ({ status: "ok" as const, data: undefined }),
    },
  );
