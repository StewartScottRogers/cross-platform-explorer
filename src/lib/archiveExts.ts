// Archive extension classifications (extracted from App.svelte, CPE-1181) so membership is a single
// source of truth AND unit-testable. These were component-local consts, which let a real regression
// ship silently: adding "iso" to the browse set also flipped on the "Extract" action for .iso even
// though the backend has no ISO extractor (CPE-1181 review). Keeping them here with a guard test
// prevents that class of bug.

/**
 * Zip-family formats whose single-entry leaf-open goes through `extract_archive_entry` (CPE-242).
 */
export const ZIP_FAMILY_EXTS = new Set([
  "zip", "jar", "apk", "war", "ear", "ipa", "xpi", "whl", "nupkg", "vsix",
]);

/**
 * Formats that can be ENTERED and browsed in the virtual archive view (CPE-1181, CPE-1348): the zip
 * family plus tar/tar.gz/tgz/7z/iso/rar. `read_archive_entries` is format-agnostic on the backend, so
 * browsing needs no per-format frontend code. `rar` is listing-only (see EXTRACT_EXTS below) — there is
 * no free/pure-Rust RAR decoder, so `crate::rar::rar_entries` only parses header metadata to list names,
 * never decompresses.
 */
export const ARCHIVE_EXTS = new Set([...ZIP_FAMILY_EXTS, "tar", "gz", "tgz", "7z", "iso", "rar"]);

/**
 * Formats `extract_archive` can actually unpack to a folder (CPE-252): the zip family plus
 * tar/gz/tgz/7z. ("foo.tar.gz" has extension "gz"; the backend disambiguates by the full path.)
 *
 * Deliberately derives from ZIP_FAMILY_EXTS (NOT ARCHIVE_EXTS) and EXCLUDES "iso" and "rar": both are
 * browsable but the backend has no extractor for either (RAR has no free decompressor at all, ISO none
 * wired), so offering "Extract" for them would fail with a leaked parser error (CPE-1181 review, CPE-1348
 * for rar). Deriving from the narrower set keeps future browse-only formats from leaking an Extract
 * action.
 */
export const EXTRACT_EXTS = new Set([...ZIP_FAMILY_EXTS, "tar", "gz", "tgz", "7z"]);

/**
 * Formats `analyze_archive_safety` can actually score for zip-bomb / expansion-ratio risk (CPE-1318):
 * exactly the zip family. The backend scan (`cpe_server::archive_safety_scan`) opens the file directly
 * with the `zip` crate — tar/gz/tgz/7z/iso aren't ZIP containers, so scoring them isn't wired yet (see
 * that module's doc comment) and would silently return a "0 entries scanned, not dangerous" report that
 * *looks* safe but was never actually analyzed. Deriving from ZIP_FAMILY_EXTS (not ARCHIVE_EXTS or
 * EXTRACT_EXTS) keeps that misleading state out of the UI — the same discipline EXTRACT_EXTS follows
 * per the CPE-1181 lesson above.
 */
export const ARCHIVE_SAFETY_EXTS = new Set([...ZIP_FAMILY_EXTS]);
