/**
 * Undo stack for file operations.
 *
 * Deliberate scope — read this before extending it:
 *
 *   * RENAME and MOVE are undoable. Both are reversible by moving the file
 *     back; nothing is destroyed, so undo is safe.
 *
 *   * COPY is NOT undoable. "Undoing" a copy would mean deleting the file that
 *     was just created — and if the destination already held a file of that
 *     name, or the user has since edited the copy, we'd be destroying real data
 *     to reverse a harmless action. Refusing to undo is strictly safer than
 *     guessing.
 *
 *   * DELETE is undoable ONLY where the platform can restore from the trash.
 *     `trash::os_limited` is implemented on Windows and Linux but NOT on macOS.
 *     So the app asks the backend (`can_restore_from_trash`) and simply does not
 *     push a delete onto the stack when restore is unavailable — Ctrl+Z then
 *     offers whatever came before it, instead of presenting an Undo that would
 *     silently do nothing. Never offer an action that cannot happen.
 *
 * The stack is bounded so it cannot grow without limit.
 */
export type UndoableKind = "rename" | "move" | "delete";

export interface UndoEntry {
  kind: UndoableKind;
  /**
   * For rename/move: where each item ended up, paired with where it came from.
   * For delete: the original paths (`to` is unused).
   */
  moves: { from: string; to: string }[];
  /** Human description, e.g. "Rename to report.txt". */
  label: string;
}

const MAX_UNDO = 25;

export function pushUndo(stack: UndoEntry[], entry: UndoEntry): UndoEntry[] {
  return [entry, ...stack].slice(0, MAX_UNDO);
}

export function popUndo(
  stack: UndoEntry[],
): { entry: UndoEntry | null; rest: UndoEntry[] } {
  if (stack.length === 0) return { entry: null, rest: stack };
  return { entry: stack[0], rest: stack.slice(1) };
}

export function canUndo(stack: UndoEntry[]): boolean {
  return stack.length > 0;
}

export function peekLabel(stack: UndoEntry[]): string {
  return stack.length > 0 ? stack[0].label : "";
}

/**
 * Invert an entry: to undo, move everything from `to` back to `from`.
 * Only meaningful for rename/move — a delete is undone by restoring from the
 * trash, not by moving anything.
 */
export function invert(entry: UndoEntry): { from: string; to: string }[] {
  return entry.moves.map((m) => ({ from: m.to, to: m.from }));
}

/** Original paths of a deleted entry, used to restore it from the trash. */
export function deletedPaths(entry: UndoEntry): string[] {
  return entry.moves.map((m) => m.from);
}
