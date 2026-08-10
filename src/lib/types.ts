// `DirEntry` + `Place` are the Rust `cpe_server::model` structs — the single source of truth is the
// generated typed client, so these are RE-EXPORTED from it rather than hand-declared (CPE-813, epic
// CPE-810). Deleting the hand-copies removes the drift surface; the ~26 importers are unchanged (they
// still import these names from `./types`). The drift-guard CI regenerates `bindings.gen.ts` and fails on
// any mismatch, so these can never silently diverge from Rust again.
export type { DirEntry, Place, NetShare, Connection, AuthMethod } from "./bindings.gen";

// The 4 built-ins, plus `meta:<columnId>` for an active metadata column (CPE-1146, epic CPE-707) —
// `string & {}` keeps the 4 literals autocompleting in editors while still accepting any string
// (TS can't express "these literals OR a template-prefixed string" more precisely without losing the
// plain `sortKey === "name"` narrowing call sites already rely on).
export type SortKey = "name" | "modified" | "type" | "size" | (string & {});
export type SortDir = "asc" | "desc";
export type ViewMode = "details" | "list" | "icons" | "gallery";

// Row/chrome density (CPE-1526, epic CPE-1488): "comfortable" is today's unchanged spacing (the
// default); "compact" tightens row pitch and chrome once CPE-1527/1528/1529 consume it. This ticket
// only models + threads the value — no renderer reads it yet.
export type DensityMode = "comfortable" | "compact";

// Theme preference (CPE-1535/CPE-1540, epic CPE-1492): "system" resolves live against the OS
// prefers-color-scheme signal; "light"/"dark" are explicit overrides. See theme.ts's resolveTheme.
export type ThemeSetting = "system" | "light" | "dark";

// Contrast preference (CPE-1544, epic CPE-1496 "high contrast"): an axis ORTHOGONAL to `ThemeSetting`
// — a user can want high contrast independent of which base theme they're on. "off" is the default
// (no contrast boost); "high" is an explicit manual override; "system" follows the OS high-contrast
// signal once CPE-1546 supplies one (until then it behaves identically to "off"). See theme.ts's
// resolveContrast and the widened applyTheme that composes both axes into `hc-${base}`.
export type ContrastSetting = "system" | "off" | "high";

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
