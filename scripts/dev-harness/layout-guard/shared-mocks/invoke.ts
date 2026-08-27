// CPE-1882 — shared, PLUGGABLE dev-only mock for `src/lib/invoke.ts`, aliased in for every
// scripts/dev-harness/layout-guard case (see ../../../vite.harness.layout-guard.config.ts). Unlike the
// bespoke per-component mocks under scripts/dev-harness/checkpoint-narrow/mocks/ and friends (each
// hand-written for the one component it serves), this one is generic: a case's own main.ts calls
// `registerRawInvoke(cmd, handler)` BEFORE mounting its component to supply canned fixture data, so
// adding a new case that needs a backend call never means writing (or editing) a mock module — only
// the case's own harness page. That is CPE-1882's own acceptance criterion: "a ticket author can add a
// component and a width list without touching harness internals."
//
// `unwrap` is copied verbatim from the real module (pure, tiny) so `unwrap(await commands.xxx(...))`
// call sites behave identically to production.
const handlers = new Map<string, (args: unknown) => unknown>();

/** Register a canned response for one `rawInvoke` command name. Call this from a case's own main.ts
 *  BEFORE mounting the component that will trigger it (components call it from `onMount`, which runs
 *  synchronously-enough after `new Component(...)` that "before mount" is the only safe order). */
export function registerRawInvoke<TArgs = unknown, TResult = unknown>(
  cmd: string,
  handler: (args: TArgs) => TResult | Promise<TResult>,
): void {
  handlers.set(cmd, handler as (args: unknown) => unknown);
}

export function rawInvoke<T = unknown>(cmd: string, args?: unknown): Promise<T> {
  const handler = handlers.get(cmd);
  if (!handler) {
    return Promise.reject(
      new Error(
        `layout-guard shared mock: no rawInvoke handler registered for "${cmd}" — call registerRawInvoke("${cmd}", ...) from this case's main.ts before mounting.`,
      ),
    );
  }
  return Promise.resolve(handler(args)) as Promise<T>;
}

export function unwrap<T>(r: { status: "ok"; data: T } | { status: "error"; error: unknown }): T {
  if (r.status === "ok") return r.data;
  throw r.error instanceof Error ? r.error : new Error(String(r.error));
}

/** Minimal stand-in for the real `StreamChannel<T>` — just enough for a component's own
 *  `channel.onmessage = fn; rawInvoke(cmd, { onEntry: channel })` pattern (STREAMING.md) to work: the
 *  registered handler receives the SAME channel object passed in `args` and can call
 *  `args.onEntry.onmessage(batch)` itself to deliver fixture rows before resolving. */
export function createChannel<T = unknown>(): { onmessage: ((data: T) => void) | null } {
  return { onmessage: null };
}
