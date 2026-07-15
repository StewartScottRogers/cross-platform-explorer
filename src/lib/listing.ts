/**
 * Copy-the-folder-listing helpers (CPE-422). Pure text formatters for exporting the currently
 * visible entries to the clipboard — one plain name per line, or a tab-separated Name/Size table
 * that pastes cleanly into a spreadsheet or an issue. Operates on whatever is visible, so it honours
 * the active sort + filter.
 */
import type { DirEntry } from "./types";
import { formatSize } from "./format";

/** One name per line, in the given order. */
export function namesList(entries: DirEntry[]): string {
  return entries.map((e) => e.name).join("\n");
}

/** A tab-separated `Name\tSize` table with a header row; folders show `<folder>` for size. */
export function detailList(entries: DirEntry[]): string {
  const rows = entries.map((e) => `${e.name}\t${e.is_dir ? "<folder>" : formatSize(e.size) || "0 B"}`);
  return ["Name\tSize", ...rows].join("\n");
}
