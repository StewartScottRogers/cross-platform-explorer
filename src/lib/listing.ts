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

/** Escape one CSV cell: quote it when it contains a comma, quote, or newline; double inner quotes. */
function csvCell(s: string): string {
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

/** A CSV manifest: `Name,Size,Modified` (size in bytes, empty for folders; modified as a UTC ISO
 *  timestamp so the output is deterministic). Suitable for saving to a `.csv` file. */
export function csvList(entries: DirEntry[]): string {
  const header = "Name,Size,Modified";
  const rows = entries.map((e) => {
    const size = e.is_dir ? "" : String(e.size);
    const modified = e.modified ? new Date(e.modified).toISOString() : "";
    return [csvCell(e.name), size, modified].join(",");
  });
  return [header, ...rows].join("\n");
}
