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
 * Formats that can be ENTERED and browsed in the virtual archive view (CPE-1181): the zip family
 * plus tar/tar.gz/tgz/7z/iso. `read_archive_entries` is format-agnostic on the backend, so browsing
 * needs no per-format frontend code.
 */
export const ARCHIVE_EXTS = new Set([...ZIP_FAMILY_EXTS, "tar", "gz", "tgz", "7z", "iso"]);

/**
 * Formats `extract_archive` can actually unpack to a folder (CPE-252): the zip family plus
 * tar/gz/tgz/7z. ("foo.tar.gz" has extension "gz"; the backend disambiguates by the full path.)
 *
 * Deliberately derives from ZIP_FAMILY_EXTS (NOT ARCHIVE_EXTS) and EXCLUDES "iso": iso is browsable
 * but the backend has no ISO extractor, so offering "Extract" for it would fail with a leaked
 * zip-parser error (CPE-1181 review). Deriving from the narrower set keeps future browse-only
 * formats from leaking an Extract action.
 */
export const EXTRACT_EXTS = new Set([...ZIP_FAMILY_EXTS, "tar", "gz", "tgz", "7z"]);
