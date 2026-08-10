// Pure, DOM/Tauri-free helpers for the split/join dialogs (CPE-1509, parent CPE-1491):
// SplitFileDialog.svelte ("Split file…") and JoinPartsDialog.svelte ("Join parts…"). Both dialogs own
// their own backend call (`commands.splitFile` / `commands.joinFiles`) + form state; these functions are
// just the part-size preset/parsing and part-file/manifest detection logic, so they're unit-testable
// without a webview — same split as certCreate.ts/vaultCreate.ts for their dialogs.
//
// The part-file/manifest naming rules here MUST mirror `cpe_server::split_join`'s `resolve_manifest_path`
// (`crates/server/src/split_join.rs`) exactly — this module only decides which context-menu row to show
// and where to point the "Join parts…" dialog's default output path; the backend is the sole authority
// that actually validates a manifest/part set, and it errors clearly on anything that doesn't hold up.

import { baseName, parentDir } from "./contentSearch";
import { joinPath } from "./certCreate";

const MiB = 1024 * 1024;
const GiB = 1024 * 1024 * 1024;

/** Common part-size presets (bytes) offered by the Split dialog, alongside a free-entry MiB/GiB field. */
export const PART_SIZE_PRESETS: { label: string; bytes: number }[] = [
  { label: "1.44 MB — floppy disk", bytes: 1_474_560 },
  { label: "650 MB — CD", bytes: 650 * MiB },
  { label: "4 GB − 1 byte — FAT32 max file size", bytes: 4 * GiB - 1 },
];

/** Parse the Split dialog's free-entry part-size field (a positive number + a MiB/GiB unit) to bytes,
 *  or `null` when the value isn't a positive finite number — the Split button stays disabled on `null`,
 *  same reasoning as `parseCustomPartSize`'s callers gating on it. */
export function parseCustomPartSize(value: number, unit: "MiB" | "GiB"): number | null {
  if (!Number.isFinite(value) || value <= 0) return null;
  return Math.round(value * (unit === "GiB" ? GiB : MiB));
}

/** Suffix of the manifest file `splitFile` writes alongside a split's parts — mirrors the backend's
 *  `MANIFEST_SUFFIX` constant (`<original_name>.split-manifest.json`). */
export const MANIFEST_SUFFIX = ".split-manifest.json";

/** True for a `<name>.split-manifest.json` file with a non-empty `<name>` — the same exact-suffix +
 *  non-empty-stem check the backend's `resolve_manifest_path` uses. */
export function isSplitManifestName(name: string): boolean {
  return name.endsWith(MANIFEST_SUFFIX) && name.length > MANIFEST_SUFFIX.length;
}

/** True for a numbered split part — `<stem>.NNN` where `NNN` is one or more digits and `<stem>` is
 *  non-empty. Not fixed-width (a split with 1000+ parts pads wider than 3 digits), mirroring the
 *  backend's tolerant digit-only suffix check rather than a strict `\.\d{3}$`. */
export function isSplitPartName(name: string): boolean {
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return false;
  const seq = name.slice(dot + 1);
  return seq.length > 0 && /^\d+$/.test(seq);
}

/** "Join parts…" context-menu eligibility: a manifest OR a numbered part name. */
export function isSplitPartOrManifestName(name: string): boolean {
  return isSplitManifestName(name) || isSplitPartName(name);
}

/** "Split file…" context-menu eligibility (CPE-1509): a single, non-empty, regular file — splitting an
 *  empty file is pointless and the backend refuses `part_size == 0` anyway, but an empty *source* file
 *  is excluded here so the row doesn't even appear. */
export function canSplitFile(entry: { is_dir: boolean; size: number }): boolean {
  return !entry.is_dir && entry.size > 0;
}

/** "Join parts…" context-menu eligibility (CPE-1509): a single regular file that's either the manifest
 *  itself or one of its numbered parts. */
export function canJoinFile(entry: { is_dir: boolean; name: string }): boolean {
  return !entry.is_dir && isSplitPartOrManifestName(entry.name);
}

/** The manifest's own path for `partOrManifestPath` — itself when it's already the manifest, or
 *  `<dir>/<stem><MANIFEST_SUFFIX>` for a numbered part (`<stem>.NNN`), mirroring the backend's
 *  `resolve_manifest_path`. A pure path computation — doesn't touch disk or validate anything; the Join
 *  dialog uses it only to know which file to `readFileText` for a manifest preview (part count, total
 *  size, original name) before the real, disk-validating `joinFiles` call. */
export function manifestPathFor(partOrManifestPath: string): string {
  const name = baseName(partOrManifestPath);
  if (isSplitManifestName(name)) return partOrManifestPath;
  const dot = name.lastIndexOf(".");
  const stem = dot > 0 ? name.slice(0, dot) : name;
  return joinPath(parentDir(partOrManifestPath), `${stem}${MANIFEST_SUFFIX}`);
}

/** The Join dialog's default output path: the manifest's `original_name`, in the same folder as the
 *  clicked part/manifest file (CPE-1509 — "pre-filled with manifest originalName in same folder"). */
export function defaultJoinOutputPath(partOrManifestPath: string, originalName: string): string {
  return joinPath(parentDir(partOrManifestPath), originalName);
}

/** Best-effort guess at the original filename from a part/manifest NAME alone, with no manifest read —
 *  used only as a Join dialog fallback default while the manifest is still loading, or if reading/parsing
 *  it fails; once the manifest loads, its authoritative `original_name` field replaces this guess via
 *  {@link defaultJoinOutputPath}. */
export function guessOriginalName(name: string): string {
  if (isSplitManifestName(name)) return name.slice(0, name.length - MANIFEST_SUFFIX.length);
  if (isSplitPartName(name)) return name.slice(0, name.lastIndexOf("."));
  return name;
}
