// Native OS drag-out plumbing (CPE-1264, epic CPE-661 follow-on for CPE-672/674): a thin wrapper over
// `@crabnebula/tauri-plugin-drag`'s `startDrag`, which starts a REAL OS-level file drag so a drop can
// land in Explorer/Finder/another app — plain HTML5 webview drag can only expose in-page `DataTransfer`
// strings, never real filesystem paths, cross-platform (see the vetted research entry
// `.claude/research-library/entries/drag-out-to-os-tauri-plugin-drag-2026-08-02.md`).
//
// This module is PURE PLUMBING (CPE-1264, slice A): nothing calls it yet. No row's `dragstart` is wired
// to it — that's a later, attended ticket (CPE-672) that also has to pick how a native drag coexists
// with the existing HTML5 internal drag (`dnd.ts`, `FileList.svelte`) drops rely on. Wiring it in here
// would be a silent behavior change for existing drag/drop, which this ticket must not cause.
//
// Every call is wrapped so a caller can feature-gate instead of crashing: outside a Tauri webview (plain
// browser, unit tests, an unsupported target) `startDrag` throws/rejects because there's no
// `__TAURI_INTERNALS__` IPC bridge — this wrapper catches that and resolves to an `"unavailable"` result,
// mirroring how `invoke.ts`/other lib modules keep the Tauri boundary a call site never has to guard by
// hand.
import { startDrag, type Options as DragPluginOptions } from "@crabnebula/tauri-plugin-drag";
import { resolveResource } from "@tauri-apps/api/path";

/** Copy-vs-move hint for the OS drag, same vocabulary as the existing HTML5 drag (`dnd.ts`). Optional —
 *  omit it to let the OS/target decide (its native default). */
export type DragOutMode = "copy" | "move";

/** How the OS-level drag ended, reported by the plugin's event channel. */
export type DragOutResult = "Dropped" | "Cancelled";

/** Logical cursor position at the end of the drag, as reported by the plugin. */
export interface DragOutCursorPos {
  x: number;
  y: number;
}

/** Fired once the OS drag ends — either dropped somewhere or cancelled (e.g. Esc). */
export interface DragOutEvent {
  result: DragOutResult;
  cursorPos: DragOutCursorPos;
}

export interface StartFileDragOptions {
  /** Preview image shown under the cursor while dragging. `startDrag`'s `icon` is REQUIRED and must be a
   *  non-empty absolute filesystem path (not a webview asset URL or data URI) — a missing icon is the
   *  plugin's classic first bug. Omit to fall back to {@link DEFAULT_DRAG_ICON}. */
  icon?: string;
  /** Copy-vs-move hint; omitted lets the OS/target decide. */
  mode?: DragOutMode;
  /** Called once when the drag ends (drop or cancel). Useful for cleanup — e.g. deleting a temp-extracted
   *  archive entry only after the OS confirms the drop, per CPE-674's flow. */
  onEvent?: (event: DragOutEvent) => void;
}

/** Relative RESOURCE identifier for the bundled app icon used as the drag preview when the caller
 *  doesn't supply one. This is NOT a usable value on its own — the plugin's `icon` must be an absolute
 *  filesystem path (CPE-1264 reviewer flag) — so it's resolved to an absolute path at runtime by
 *  {@link resolveDragIcon}. `icons/icon.png` ships in `bundle.resources` (src-tauri/tauri.conf.json) so
 *  `resolveResource` can turn it into a real on-disk path inside the app's resource directory. The
 *  attended CPE-672/674 tickets that wire up drag-out can still pass a more specific preview (a
 *  thumbnail, a multi-file badge, …) via `opts.icon`. */
export const DEFAULT_DRAG_ICON = "icons/icon.png";

// Resolve the bundled icon once and reuse it — `resolveResource` is a Tauri IPC round-trip, and a drag
// starts on a mouse gesture where we want the icon ready synchronously (pre-warm it via
// `resolveDragIcon()` at mount). Only a SUCCESSFUL absolute resolution is cached, so a transient failure
// (called before the bridge is up) never poisons later calls — and a failed attempt never poisons a
// *later* successful one either, since nothing is cached until resolution actually succeeds.
let cachedDragIcon: string | null = null;

/**
 * Resolve {@link DEFAULT_DRAG_ICON} to an ABSOLUTE filesystem path the plugin will accept, via Tauri's
 * `resolveResource`. Returns `null` if resolution fails (no Tauri bridge, resource missing) — CPE-1269
 * hardening: earlier this fell back to the RELATIVE `DEFAULT_DRAG_ICON` identifier, which a caller (e.g.
 * `FileList`'s `dragOutIcon` cache) could then pass straight through to the plugin as `icon`, violating
 * its "must be an absolute filesystem path" invariant and silently failing the drag. Returning `null`
 * instead means "no resolved icon" is unambiguous, so both the cache and {@link startFileDrag} can tell a
 * good absolute path apart from a failed resolution and never store/forward a relative one. Never throws.
 * Call it once at mount to pre-warm the cache so a later drag has the icon ready.
 */
export async function resolveDragIcon(): Promise<string | null> {
  if (cachedDragIcon) return cachedDragIcon;
  try {
    const abs = await resolveResource(DEFAULT_DRAG_ICON);
    if (abs && abs.length > 0) {
      cachedDragIcon = abs;
      return abs;
    }
  } catch {
    // No Tauri IPC bridge (plain browser / jsdom) or the resource isn't present — fall through to `null`
    // rather than the relative identifier or a surfaced error; see the doc comment above (CPE-1269).
  }
  return null;
}

export type StartFileDragResult =
  /** The OS drag was started. (This resolves once the drag *starts*, not once it ends — see `onEvent`
   *  for the outcome.) */
  | { status: "ok" }
  /** No paths were given, so there was nothing to drag — never reaches the plugin. */
  | { status: "unavailable"; reason: "no-paths" }
  /** The plugin isn't usable in this environment (no Tauri IPC bridge — plain browser, unit test,
   *  unsupported target/platform). Callers should feature-gate on this instead of surfacing an error. */
  | { status: "unavailable"; reason: "plugin-unavailable" }
  /** `startDrag` itself rejected (e.g. the OS refused the drag). */
  | { status: "error"; error: unknown };

/**
 * Start a native OS file drag for `paths` (absolute filesystem paths of the dragged selection), so a
 * drop can land in Explorer/Finder/another app. Resolves once the drag has *started* — use `opts.onEvent`
 * to learn how it ended.
 *
 * Never throws: an empty `paths`, a missing plugin bridge, or a rejected `startDrag` call all resolve to
 * a `StartFileDragResult` the caller can branch on, so a call site can feature-gate drag-out without a
 * try/catch of its own.
 */
export async function startFileDrag(
  paths: string[],
  opts: StartFileDragOptions = {},
): Promise<StartFileDragResult> {
  if (paths.length === 0) {
    return { status: "unavailable", reason: "no-paths" };
  }

  // A caller-supplied icon is used verbatim (it's expected to already be absolute); otherwise resolve the
  // bundled default to an absolute path — `startDrag`'s `icon` must be a real filesystem path, and a bare
  // relative "icons/icon.png" was the CPE-1264 reviewer's flag. `resolveDragIcon` now returns `null`
  // rather than a relative fallback (CPE-1269) when resolution fails, so `icon` here is either a verified
  // absolute path or nothing at all — never a relative string that would reach the plugin.
  const icon = opts.icon && opts.icon.length > 0 ? opts.icon : await resolveDragIcon();

  // The plugin's `Options.icon` is typed as a required `string`, but there is no absolute path to give it
  // when resolution failed and the caller didn't supply one. Sending the known-bad relative
  // `DEFAULT_DRAG_ICON` would violate the plugin's absolute-path invariant and silently break the drag
  // (the CPE-1269 bug this hardens); sending nothing is the lesser risk — worst case the plugin falls
  // back to its own default preview (or, if it truly requires the field, rejects the call, which surfaces
  // through the existing catch below as `status: "error"` same as any other rejected `startDrag`). The
  // `as DragPluginOptions` cast documents that tradeoff explicitly rather than lying about the type.
  const dragOptions = (
    icon
      ? { item: [...paths], icon, mode: opts.mode }
      : { item: [...paths], mode: opts.mode }
  ) as DragPluginOptions;

  try {
    await startDrag(
      dragOptions,
      opts.onEvent
        ? (payload) =>
            opts.onEvent!({
              result: payload.result,
              cursorPos: { x: Number(payload.cursorPos.x), y: Number(payload.cursorPos.y) },
            })
        : undefined,
    );
    return { status: "ok" };
  } catch (error) {
    // No Tauri IPC bridge (plain browser / jsdom / unsupported target) surfaces as a rejected/thrown
    // `invoke` inside the plugin — treat that whole class of failure as "unavailable" rather than an
    // error, since it's an environment gap, not a drag failure.
    if (!isTauriEnv()) {
      return { status: "unavailable", reason: "plugin-unavailable" };
    }
    return { status: "error", error };
  }
}

/** Whether this code is running inside a real Tauri webview (i.e. `startDrag` has an IPC bridge to talk
 *  to). Absent in a plain browser, jsdom/unit tests, or any environment the Tauri preload script hasn't
 *  run in. Exported so tests can assert the no-op path explicitly. */
export function isTauriEnv(): boolean {
  return (
    typeof window !== "undefined" &&
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== undefined
  );
}
