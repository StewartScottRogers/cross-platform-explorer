// `DirEntry` + `Place` are the Rust `cpe_server::model` structs — the single source of truth is the
// generated typed client, so these are RE-EXPORTED from it rather than hand-declared (CPE-813, epic
// CPE-810). Deleting the hand-copies removes the drift surface; the ~26 importers are unchanged (they
// still import these names from `./types`). The drift-guard CI regenerates `bindings.gen.ts` and fails on
// any mismatch, so these can never silently diverge from Rust again.
export type { DirEntry, Place } from "./bindings.gen";

// The 4 built-ins, plus `meta:<columnId>` for an active metadata column (CPE-1146, epic CPE-707) —
// `string & {}` keeps the 4 literals autocompleting in editors while still accepting any string
// (TS can't express "these literals OR a template-prefixed string" more precisely without losing the
// plain `sortKey === "name"` narrowing call sites already rely on).
export type SortKey = "name" | "modified" | "type" | "size" | (string & {});
export type SortDir = "asc" | "desc";
export type ViewMode = "details" | "list" | "icons" | "gallery";

export interface RecentFile {
  path: string;
  name: string;
  /** Epoch ms when it was last opened from this app. */
  opened: number;
}

export interface Favorite {
  path: string;
  name: string;
  /** Folders navigate on click; files open. */
  is_dir: boolean;
}
