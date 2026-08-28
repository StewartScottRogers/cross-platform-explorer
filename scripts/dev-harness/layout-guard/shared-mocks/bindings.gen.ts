// CPE-1882 — shared dev-only mock for `src/lib/bindings.gen.ts`, aliased in for every
// scripts/dev-harness/layout-guard case (see ../../../vite.harness.layout-guard.config.ts). Generic on
// purpose: ANY `commands.xxx(...)` call resolves `{ status: "ok", data: undefined }`, so a component
// that only calls `commands.*` from a click handler (never before the harness has taken its own
// measurement) needs no per-command wiring at all. That covers every layout-guard case so far —
// TrashView.svelte's `commands.restoreTrashItems`/`commands.emptyTrash` are both click-triggered, never
// awaited before first paint.
//
// CPE-1968 made this PLUGGABLE rather than bespoke, which is what the paragraph below anticipated.
// It used to say: "if a future case genuinely needs a specific `commands.*` return value BEFORE its
// first paint (unlike every case today), give IT its own case-scoped mock + alias instead of
// special-casing that command here — see scripts/dev-harness/checkpoint-narrow/mocks/bindings.gen.ts
// for that bespoke pattern". CPE-1968 was that future case (`OrganizeDialog.svelte` awaits
// `commands.organizePlan` before it can render anything but its loading state), and a case-scoped
// alias would have meant editing vite.harness.layout-guard.config.ts — the exact "touching harness
// internals" CPE-1882's acceptance criterion rules out. So instead this grew the same
// register-before-mount seam its sibling `invoke.ts` mock already had, and the bespoke route stays
// available for anything a handler function genuinely cannot express.
//
// The DEFAULT is unchanged: an unregistered command still resolves `{ status: "ok", data: undefined }`,
// so every pre-existing case behaves exactly as before.
const commandHandlers = new Map<string, (...args: unknown[]) => unknown>();

/** Register a canned response for one `commands.<name>` call. Call from a case's own main.ts BEFORE
 *  mounting the component, for the same reason `registerRawInvoke` says so: components fire these
 *  from `onMount`/reactive statements that run as soon as the component is constructed. Return the
 *  DATA, not the envelope — the envelope is added here, exactly as the real client does. */
export function registerCommand<TResult = unknown>(
  name: string,
  handler: (...args: unknown[]) => TResult | Promise<TResult>,
): void {
  commandHandlers.set(name, handler as (...args: unknown[]) => unknown);
}

export const commands: Record<string, (...args: unknown[]) => Promise<{ status: "ok"; data: unknown }>> =
  new Proxy(
    {},
    {
      get:
        (_target, name: string) =>
        async (...args: unknown[]) => {
          const handler = commandHandlers.get(name);
          return { status: "ok" as const, data: handler ? await handler(...args) : undefined };
        },
    },
  );
