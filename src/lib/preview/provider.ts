/**
 * Preview provider architecture for the file preview pane (CPE-059).
 *
 * A provider declares which entries it can preview and the "kind" of preview to
 * render. Providers are an ordered registry; the first that matches an entry
 * wins, falling back to a metadata-only view. They are all bundled — "plugin"
 * here means an internal registered provider, not runtime-loaded code.
 */
import type { DirEntry } from "../types";
import { categoryOf } from "../filetypes";

export type PreviewKind =
  | "image"
  | "decoded-image"
  | "audio"
  | "video"
  | "pdf"
  | "json"
  | "csv"
  | "tsv"
  | "archive"
  | "markdown"
  | "text"
  | "info"
  | "none";

/** One entry inside an archive (mirrors the Rust `ArchiveEntry`). */
export interface ArchiveEntry {
  name: string;
  size: number;
  is_dir: boolean;
}

export interface PreviewProvider {
  /** Stable id. */
  id: string;
  /** Human-readable label (for a future provider picker / diagnostics). */
  label: string;
  /** How the pane should render this preview. */
  kind: PreviewKind;
  /** Whether this file type can be edited as raw text (text/md/json/csv). */
  editable: boolean;
  /** Whether this provider can preview the given entry. */
  canPreview(entry: DirEntry): boolean;
}

const MARKDOWN_EXT = new Set(["md", "markdown"]);

/** Extensions the read_archive_entries backend can list (zip family, tar, gzip). */
const ARCHIVE_EXT = new Set([
  "zip", "jar", "apk", "war", "ear", "ipa", "xpi",
  "tar", "tgz", "gz", "7z",
]);

/**
 * Binary formats previewed as a read-only text summary produced by the
 * read_preview_info backend (hex dump / PE headers / MIDI / wasm / torrent).
 * CPE-210/214/215/216/218.
 */
const INFO_EXT = new Set([
  "exe", "dll", "sys", "efi", "ocx", "scr", "cpl", // PE headers
  "wasm", "torrent", "mid", "midi",
  "rtf", "docx", "odt", "epub", // document text extraction
  "sqlite", "sqlite3", "db", // SQLite schema + row counts
  "xlsx", "xlsm", "ods", // spreadsheet grid
  "parquet", // parquet schema + row count
  "bin", "dat", // generic binary -> hex dump
]);

/**
 * Images the webview can't render natively — decoded to PNG by the
 * read_image_data_url backend and shown via a data URL. CPE-099/101.
 */
const DECODED_IMAGE_EXT = new Set(["tiff", "tif", "psd"]);

/**
 * Ordered by priority — the first match wins. Markdown is listed before text
 * because a `.md` file's category is "text"; without the ordering, text would
 * claim it first.
 */
export const providers: PreviewProvider[] = [
  // Must precede the native image provider: .tiff is categorised as an image but
  // the webview can't decode it, so it needs the backend transcode path instead.
  {
    id: "decoded-image",
    label: "Image (decoded)",
    kind: "decoded-image",
    editable: false,
    canPreview: (e) => !e.is_dir && DECODED_IMAGE_EXT.has(e.extension),
  },
  {
    id: "image",
    label: "Image",
    kind: "image",
    editable: false,
    canPreview: (e) => !e.is_dir && categoryOf(e) === "image",
  },
  {
    id: "audio",
    label: "Audio",
    kind: "audio",
    editable: false,
    canPreview: (e) => !e.is_dir && categoryOf(e) === "audio",
  },
  {
    id: "video",
    label: "Video",
    kind: "video",
    editable: false,
    canPreview: (e) => !e.is_dir && categoryOf(e) === "video",
  },
  {
    id: "pdf",
    label: "PDF",
    kind: "pdf",
    editable: false,
    canPreview: (e) => !e.is_dir && categoryOf(e) === "pdf",
  },
  // JSON and CSV are declared before the generic text provider because their
  // categories ("code"/"spreadsheet") would otherwise be claimed by it / skipped.
  {
    id: "json",
    label: "JSON",
    kind: "json",
    editable: true,
    canPreview: (e) => !e.is_dir && e.extension === "json",
  },
  {
    id: "csv",
    label: "CSV",
    kind: "csv",
    editable: true,
    canPreview: (e) => !e.is_dir && e.extension === "csv",
  },
  {
    id: "tsv",
    label: "TSV",
    kind: "tsv",
    editable: true,
    canPreview: (e) => !e.is_dir && (e.extension === "tsv" || e.extension === "tab"),
  },
  {
    id: "archive",
    label: "Archive",
    kind: "archive",
    editable: false,
    // zip family (jar/apk/war/… are zips), plus tar and gzip — all listed by the
    // read_archive_entries backend (CPE-064/109/112/217).
    canPreview: (e) => !e.is_dir && ARCHIVE_EXT.has(e.extension),
  },
  {
    id: "info",
    label: "Info",
    kind: "info",
    editable: false,
    canPreview: (e) => !e.is_dir && INFO_EXT.has(e.extension),
  },
  {
    id: "markdown",
    label: "Markdown",
    kind: "markdown",
    editable: true,
    canPreview: (e) => !e.is_dir && MARKDOWN_EXT.has(e.extension),
  },
  {
    id: "text",
    label: "Text",
    kind: "text",
    editable: true,
    canPreview: (e) =>
      !e.is_dir && (categoryOf(e) === "text" || categoryOf(e) === "code"),
  },
];

/** Used when no richer provider matches — the pane shows metadata instead. */
export const FALLBACK: PreviewProvider = {
  id: "none",
  label: "Details",
  kind: "none",
  editable: false,
  canPreview: () => true,
};

/**
 * Pick the best preview provider for an entry. Never returns null: folders,
 * nothing selected, and unrecognised files all resolve to the metadata fallback.
 */
export function pickProvider(entry: DirEntry | null | undefined): PreviewProvider {
  if (!entry || entry.is_dir) return FALLBACK;
  return providers.find((p) => p.canPreview(entry)) ?? FALLBACK;
}
