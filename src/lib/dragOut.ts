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
import { startDrag } from "@crabnebula/tauri-plugin-drag";

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

/** Bundled app icon used as the drag preview when the caller doesn't supply one. Good enough as a
 *  placeholder for this plumbing slice; the attended CPE-672/674 tickets that actually wire up drag-out
 *  can pass a more specific preview (a thumbnail, a multi-file badge, …) via `opts.icon`. */
export const DEFAULT_DRAG_ICON = "icons/icon.png";

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

  const icon = opts.icon && opts.icon.length > 0 ? opts.icon : DEFAULT_DRAG_ICON;

  try {
    await startDrag(
      {
        item: [...paths],
        icon,
        mode: opts.mode,
      },
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
