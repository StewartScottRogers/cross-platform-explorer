/**
 * The keyboard-shortcut cheat sheet (CPE-339). This is the single source of
 * truth the Shortcuts dialog renders. Every entry here is transcribed verbatim
 * from `handleKeydown` in App.svelte — keep the two in lockstep so the sheet can
 * never claim a binding the app does not actually honour.
 *
 * Pure data with no imports, so it is trivially unit-testable and carries zero
 * runtime weight for the plain explorer.
 */

export interface Shortcut {
  /** Human-readable key combo, e.g. "Ctrl+L" or "Alt+↑". */
  keys: string;
  description: string;
}

export interface ShortcutGroup {
  title: string;
  items: Shortcut[];
}

export const SHORTCUT_GROUPS: ShortcutGroup[] = [
  {
    title: "Navigation",
    items: [
      { keys: "Alt+←", description: "Back" },
      { keys: "Alt+→", description: "Forward" },
      { keys: "Alt+↑", description: "Up one folder" },
      { keys: "Backspace", description: "Up one folder" },
      { keys: "F5", description: "Refresh" },
      { keys: "Ctrl+L", description: "Edit address (type a path)" },
      { keys: "Alt+D", description: "Edit address (type a path)" },
      { keys: "Ctrl+F", description: "Search the current folder" },
      { keys: "Ctrl+Shift+F", description: "Search inside files (content search)" },
      { keys: "Enter", description: "Open the selected item" },
      { keys: "Type a name", description: "Jump to the matching item" },
    ],
  },
  {
    title: "Tabs",
    items: [
      { keys: "Ctrl+T", description: "New tab" },
      { keys: "Ctrl+W", description: "Close tab" },
      { keys: "Ctrl+Shift+T", description: "Reopen last closed tab" },
      { keys: "Ctrl+Tab", description: "Next tab" },
      { keys: "Ctrl+Shift+Tab", description: "Previous tab" },
    ],
  },
  {
    title: "Selection",
    items: [
      { keys: "Ctrl+A", description: "Select all" },
      { keys: "↑ / ↓", description: "Move selection" },
      { keys: "Shift+↑ / ↓", description: "Extend selection" },
      { keys: "Home / End", description: "First / last item" },
      { keys: "Esc", description: "Clear selection" },
    ],
  },
  {
    title: "File actions",
    items: [
      { keys: "Ctrl+C", description: "Copy" },
      { keys: "Ctrl+X", description: "Cut" },
      { keys: "Ctrl+V", description: "Paste" },
      { keys: "Ctrl+D", description: "Duplicate" },
      { keys: "Ctrl+Z", description: "Undo" },
      { keys: "F2", description: "Rename" },
      { keys: "Delete", description: "Delete to Recycle Bin / Trash" },
      { keys: "Shift+Delete", description: "Delete permanently" },
      { keys: "Ctrl+Shift+N", description: "New folder" },
      { keys: "Ctrl+Shift+C", description: "Copy as path" },
      { keys: "Alt+Enter", description: "Properties" },
    ],
  },
  {
    title: "View",
    items: [
      { keys: "Alt+P", description: "Toggle the details panel" },
      { keys: "Ctrl+Shift+O", description: "Pop out the preview" },
      { keys: "F1", description: "Show this shortcuts list" },
    ],
  },
];
