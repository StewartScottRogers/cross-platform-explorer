// Shared drag-and-drop model (CPE-669, epic CPE-661): one place that owns what carries in a file drag,
// whether a target is a valid drop, and the copy-vs-move decision — so every view (FileList in all its
// modes, Sidebar, SidebarNode) reuses the same rules instead of re-implementing them. Pure + DOM-light
// so the decision logic is unit-tested; the `setDragData` helper just wraps the DataTransfer calls.

/** The MIME the drag payload uses — newline-joined absolute paths of the dragged selection. */
export const DRAG_MIME = "text/plain";

/** Modifier state that influences the copy-vs-move choice. */
export interface DragMods {
  ctrlKey: boolean;
  shiftKey: boolean;
}

/** Normalize a path for comparison: backslashes → forward slashes, trailing slash dropped. Pure. */
function norm(p: string): string {
  return p.replace(/\\/g, "/").replace(/\/+$/, "");
}

/** Put the dragged selection on the DataTransfer and allow both copy and move. No-op if `dt` is null. */
export function setDragData(dt: DataTransfer | null, paths: string[]): void {
  if (!dt) return;
  dt.setData(DRAG_MIME, paths.join("\n"));
  dt.effectAllowed = "copyMove";
}

/**
 * Whether dropping the dragged selection onto `dest` is allowed: something must be dragged, and `dest`
 * cannot be one of the dragged paths or a descendant of one (which would move a folder into itself).
 * Callers that only accept folders (FileList rows) still gate on `is_dir` separately. Pure.
 */
export function isValidDrop(draggedPaths: string[], dest: string): boolean {
  if (draggedPaths.length === 0 || !dest) return false;
  const d = norm(dest);
  return !draggedPaths.some((p) => {
    const s = norm(p);
    return d === s || d.startsWith(s + "/");
  });
}

/**
 * The copy-vs-move decision for a drop (CPE-661 OS convention): Ctrl forces copy, Shift forces move;
 * otherwise same-volume = move and cross-volume = copy. `sameVolume` is `null` when not yet known, which
 * resolves to copy — the safe default that never loses the source. Pure.
 */
export function resolveEffect(mods: DragMods, sameVolume: boolean | null): "copy" | "move" {
  if (mods.ctrlKey) return "copy";
  if (mods.shiftKey) return "move";
  return sameVolume === true ? "move" : "copy";
}

/**
 * The cursor effect to show while hovering a target (CPE-1372): same modifier precedence as
 * {@link resolveEffect} (Ctrl forces copy, Shift forces move), and once `sameVolume` is confirmed
 * `false` (cross-volume) it shows "copy" too, so the hover cursor matches what drop will actually
 * do instead of always claiming "move". `sameVolume` is a best-effort signal a caller can thread
 * through from an async same-volume check (see {@link resolveEffect}'s doc); pass `null` (the
 * default) when no signal is available yet. Unlike `resolveEffect`, an unknown/`null` signal here
 * still resolves to "move" — the common case, and the pre-CPE-1372 default — rather than "copy", so
 * a caller with no hover-time signal doesn't regress every same-volume drag to a "copy" cursor. Pure.
 */
export function hoverEffect(mods: DragMods, sameVolume: boolean | null = null): "copy" | "move" {
  if (mods.ctrlKey) return "copy";
  if (mods.shiftKey) return "move";
  return sameVolume === false ? "copy" : "move";
}
