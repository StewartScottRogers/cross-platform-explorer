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
 *   * DELETE is NOT undoable here. Items go to the Recycle Bin, so they are
 *     already recoverable by the user through the OS. Programmatic restore
 *     (`trash::os_limited`) is not implemented on macOS, so wiring it up would
 *     produce an "Undo" that silently does nothing on one of the three
 *     platforms we ship. A button that lies is worse than no button.
 *
 * The stack is bounded so it cannot grow without limit.
 */
export type UndoableKind = "rename" | "move";

export interface UndoEntry {
  kind: UndoableKind;
  /** Where each item ended up, paired with where it came from. */
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
 * Returns the source paths and the destination directory for each item.
 */
export function invert(entry: UndoEntry): { from: string; to: string }[] {
  return entry.moves.map((m) => ({ from: m.to, to: m.from }));
}
