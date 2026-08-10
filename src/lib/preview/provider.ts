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
import { invoke as ipcInvoke } from "../invoke";

export type PreviewKind =
  | "image"
  | "decoded-image"
  | "raw-image"
  | "dicom"
  | "heic"
  | "media"
  | "pdf"
  | "json"
  | "csv"
  | "tsv"
  | "archive"
  | "markdown"
  | "text"
  | "info"
  | "data-grid"
  | "font"
  | "email"
  | "calendar"
  | "vcard"
  | "jwt"
  | "cert"
  | "hex"
  | "folder"
  | "none";

/** One entry inside an archive (mirrors the Rust `ArchiveEntry`). */
export interface ArchiveEntry {
  name: string;
  size: number;
  is_dir: boolean;
}

/**
 * Context handed to a {@link PreviewAction}'s `enabled`/`run` (CPE-1570, epic CPE-1568). Modeled on what
 * `PreviewPane` already threads through its own copy/edit logic, so a declared action never needs a new
 * plumbing path of its own — it reads what the pane already has.
 */
export interface PreviewActionCtx {
  /** The previewed entry the action applies to. */
  entry: DirEntry;
  /** The pane's current "primary" text for this preview: the raw file text for a text/json/csv/tsv
   *  provider. Undefined for provider kinds (JWT, certificate, …) that render their own self-contained
   *  component instead of using the pane's text pipeline — those report values via `values` below. */
  text?: string;
  /** The user's current selection inside the preview (the editor selection while editing, else the
   *  window selection), when relevant to the action. */
  selectionText?: string;
  /** Extra named values a self-contained preview component (JWT, certificate, …) reports up for its own
   *  actions to read, keyed by the action `id` that consumes them (e.g. a JWT preview reports
   *  `{ "copy-claims": <claims JSON>, "copy-header": <header JSON> }`). Empty when not applicable. */
  values: Record<string, string>;
  /** Copy text to the clipboard — the same "best-effort, ignore if unavailable" path the pane's own Copy
   *  menu item uses, so a copy action behaves identically to the inline buttons it replaces. */
  copyToClipboard(text: string): Promise<void>;
  /** The busy-tracking backend `invoke` (`src/lib/invoke.ts`) — an action that needs a backend command
   *  MUST call through this, never `@tauri-apps/api/core` directly (CLAUDE.md busy-cursor convention). */
  invoke: typeof ipcInvoke;
}

/**
 * A single declarative action a provider exposes in the preview pane's generic action bar (CPE-1570,
 * epic CPE-1568 — custom per-file-type right pane, slice 1). Providers list these instead of a preview
 * component wiring up its own ad hoc buttons, so the pane can render + run them uniformly.
 */
export interface PreviewAction {
  /** Stable id, unique within the provider's own action list. Also doubles as the key a preview
   *  component uses in `ctx.values` for an action that copies a provider-specific value. */
  id: string;
  /** i18n key for the button label — resolved via `$t()`, never a raw string (CLAUDE.md i18n rule). */
  labelKey: string;
  /** `Icon` glyph name shown on the button. */
  icon: string;
  /** Whether the action is currently available; omitted means always enabled. */
  enabled?(ctx: PreviewActionCtx): boolean;
  /** Run the action. May be async (a clipboard write, a backend `invoke`, …). */
  run(ctx: PreviewActionCtx): void | Promise<void>;
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
  /** Declarative actions rendered in the pane's generic action bar (CPE-1570), filtered by `enabled(ctx)`
   *  at render time. Omitted/empty means no action bar for this provider. */
  actions?: PreviewAction[];
}

/** Actions a provider exposes for the given context, filtered to those currently enabled (an action with
 *  no `enabled` guard is always shown). The single point `PreviewPane`'s generic action bar consults —
 *  kept here, next to the type declarations, so the filtering/enablement logic is unit-testable without
 *  mounting a component. */
export function visibleActions(provider: PreviewProvider, ctx: PreviewActionCtx): PreviewAction[] {
  return (provider.actions ?? []).filter((a) => !a.enabled || a.enabled(ctx));
}

const MARKDOWN_EXT = new Set(["md", "markdown"]);

/** Extensions the read_archive_entries backend can list (zip family, tar, gzip, 7z, iso, rar). RAR is
 *  browse-only (listed via the pure-Rust rar_entries walker, CPE-1347/1348) — it must be here too, not just
 *  in archiveExts.ts's ARCHIVE_EXTS, or a .rar falls through to the hex/info provider instead of listing
 *  its entries (CPE-1359). */
const ARCHIVE_EXT = new Set([
  "zip", "jar", "apk", "war", "ear", "ipa", "xpi",
  "tar", "tgz", "gz", "7z", "iso", "rar",
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
  "bin", "dat", // generic binary -> hex dump
  // Single-file compression formats (CPE-1439): xz/bz2/zst/lz/lzma are categorised "archive" in
  // filetypes.ts but have NO entry list — a bare .xz etc. isn't a container, and this crate has no
  // xz/bzip2/zstd decoder wired in (unlike gzip, which flate2 already handles for .gz). Routing them to
  // ARCHIVE_EXT would hand them to read_archive_entries, which falls through to zip_entries and errors.
  // read_preview_info's compressed_file_info arm reports the format + compressed size instead — a
  // graceful summary rather than a raw hex dump of compressed bytes. dmg/cab are deliberately NOT here:
  // they'd need a real Apple-disk-image / MS-cabinet reader, which is out of scope (won't-fix, CPE-1439).
  "xz", "bz2", "zst", "lz", "lzma",
]);

/**
 * Structured data files opened in the interactive data-grid (schema + paged rows, table/sheet
 * navigation, and a read-only SQL console for SQLite) instead of a text summary. CPE-849 (epic CPE-721).
 */
const DATA_GRID_EXT = new Set([
  "sqlite", "sqlite3", "db", // SQLite tables/views
  "xlsx", "xlsm", "xlsb", "xls", "ods", // spreadsheet sheets
  "parquet", // parquet
]);

/**
 * Images the webview can't render natively — decoded to PNG by the
 * read_image_data_url backend and shown via a data URL. CPE-099/101.
 */
const DECODED_IMAGE_EXT = new Set(["tiff", "tif", "psd"]);

/**
 * Camera-RAW formats: TIFF-based containers around undemosaiced sensor data that the webview can't
 * render. Rather than decode the raw pixels, the backend extracts whichever embedded JPEG
 * preview/thumbnail the camera already wrote via `read_raw_preview_data_url` (CPE-1349, epic
 * CPE-102) — same data-URL shape as `read_image_data_url`, different backend call.
 */
const RAW_EXT = new Set(["cr2", "nef", "arw"]);

/**
 * DICOM medical-image files: the pixel data is decoded to a PNG `data:` URL via
 * `read_dicom_image_data_url`, alongside a curated tag list via `read_dicom_tags` (CPE-1350, epic
 * CPE-102). Same data-URL shape as `read_image_data_url`/`read_raw_preview_data_url`, plus the tags.
 */
const DICOM_EXT = new Set(["dcm"]);

/**
 * HEIC/HEIF images (Apple's HEVC-based photo container): the webview can't render them and no
 * pure-Rust decoder handles them cleanly, so the backend decodes them through the platform image
 * stack (Windows Imaging Component / macOS ImageIO) via `read_heic_preview_data_url` (CPE-1351, epic
 * CPE-097) — same PNG data-URL shape as `read_image_data_url`. On Windows this needs the OS "HEIF
 * Image Extensions"; when they're absent the backend returns `Err` and the pane falls back to the
 * metadata slot (like raw-image), so the miss is clean.
 */
const HEIC_EXT = new Set(["heic", "heif", "hif"]);

/** Font files rendered as a live specimen via the webview's FontFace API (CPE-117). */
const FONT_EXT = new Set(["ttf", "otf", "woff", "woff2"]);

/**
 * JWT / JWS tokens: decoded via the `jwt_preview` backend command (CPE-1418, epic CPE-1417) into
 * header/claims/signature info and rendered by `JwtPreview.svelte` (CPE-1422). A read-only VIEWER —
 * never a signature verifier.
 */
const JWT_EXT = new Set(["jwt", "jws"]);

/**
 * Certificate-family files: X.509 certificates (PEM or DER), PKCS#10 CSRs, standalone public keys, and
 * private-key files — all auto-detected and decoded via the `cert_decode` backend command (CPE-1419,
 * epic CPE-1417) and rendered by `CertPreview.svelte` (CPE-1422). Must precede the generic text provider
 * below: pem/crt/cer/csr categorise as "code" text in `filetypes.ts` and would otherwise be claimed by
 * it. A private-key file's key material is never read; the backend surfaces only its algorithm + size.
 */
const CERT_EXT = new Set(["pem", "crt", "cer", "der", "csr", "pub", "key"]);

/**
 * `.eml` email messages (RFC 822 / MIME): decoded via the `email_preview` backend command (CPE-1434,
 * epic CPE-1433) into headers + MIME parts + attachments + a sanitized plain-text body, and rendered by
 * `EmailPreview.svelte`. A read-only VIEWER — it never renders HTML and never loads remote resources.
 * Must precede the generic text provider: `.eml` categorises as "text" in `filetypes.ts` and would
 * otherwise be claimed by it and shown as raw source instead of the structured card.
 */
const EMAIL_EXT = new Set(["eml"]);

/**
 * `.ics` iCalendar files (RFC 5545): decoded via the `ical_preview` backend command (CPE-1435, epic
 * CPE-1433) into VEVENT/VTODO/VJOURNAL event cards (summary + when/where/who + recurrence) and rendered by
 * `IcalPreview.svelte`. A read-only VIEWER. Must precede the generic text provider: `.ics` categorises as
 * "text" in `filetypes.ts` and would otherwise be claimed by it and shown as raw source instead of the
 * structured event cards.
 */
const CALENDAR_EXT = new Set(["ics", "ical", "ifb", "icalendar"]);

/**
 * `.vcf` vCard files (2.1/3.0/4.0): decoded via the `vcard_preview` backend command (CPE-1436, epic
 * CPE-1433) into contact cards (name/org/title + phones/emails/addresses/URLs) and rendered by
 * `VcardPreview.svelte`. A read-only VIEWER; a card's PHOTO is reported presence-only, never fetched over
 * IPC. Must precede the generic text provider: `.vcf` categorises as "text" and would otherwise be shown
 * as raw source instead of the structured contact card.
 */
const VCARD_EXT = new Set(["vcf", "vcard"]);

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
  // Must also precede the native image provider: cr2/nef/arw categorise as images but the webview
  // can't decode raw sensor data — route to the embedded-JPEG-extraction backend instead.
  {
    id: "raw-image",
    label: "Camera RAW",
    kind: "raw-image",
    editable: false,
    canPreview: (e) => !e.is_dir && RAW_EXT.has(e.extension),
  },
  // .dcm isn't recognised as an image category at all today (falls to hex otherwise) — this must
  // still precede the generic image/hex providers so it gets its own decode+tags rendering.
  {
    id: "dicom",
    label: "DICOM",
    kind: "dicom",
    editable: false,
    canPreview: (e) => !e.is_dir && DICOM_EXT.has(e.extension),
  },
  // heic/heif/hif categorise as images but the webview can't decode HEVC-based data — route to the
  // platform-API decode backend instead. Must precede the generic image provider.
  {
    id: "heic",
    label: "HEIC",
    kind: "heic",
    editable: false,
    canPreview: (e) => !e.is_dir && HEIC_EXT.has(e.extension),
  },
  {
    id: "image",
    label: "Image",
    kind: "image",
    editable: false,
    canPreview: (e) => !e.is_dir && categoryOf(e) === "image",
  },
  // Temporal media (CPE-1429, epic CPE-720): audio + video play in the pane's native <audio>/<video>
  // element behind a custom themed transport (MediaPlayer.svelte). One `media` kind covers both —
  // `mediaType()` below decides which element to render. Sits before the generic text/hex handlers, like
  // jwt/cert, so a media file is never claimed by them. Uses the same `categoryOf` audio/video mapping as
  // the file list, so any recognised clip gets the player (with a graceful open-externally fallback on an
  // unsupported codec) rather than a hex dump.
  {
    id: "media",
    label: "Media",
    kind: "media",
    editable: false,
    canPreview: (e) => !e.is_dir && (categoryOf(e) === "audio" || categoryOf(e) === "video"),
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
    id: "font",
    label: "Font",
    kind: "font",
    editable: false,
    canPreview: (e) => !e.is_dir && FONT_EXT.has(e.extension),
  },
  // Must precede the generic text provider: .eml categorises as "text" and would otherwise be shown as
  // raw source instead of the structured header/attachment/body card (CPE-1434, epic CPE-1433).
  {
    id: "email",
    label: "Email",
    kind: "email",
    editable: false,
    canPreview: (e) => !e.is_dir && EMAIL_EXT.has(e.extension),
  },
  // Must precede the generic text provider: .ics categorises as "text" and would otherwise be shown as
  // raw source instead of the structured event cards (CPE-1435, epic CPE-1433).
  {
    id: "calendar",
    label: "Calendar",
    kind: "calendar",
    editable: false,
    canPreview: (e) => !e.is_dir && CALENDAR_EXT.has(e.extension),
  },
  // Must precede the generic text provider: .vcf categorises as "text" and would otherwise be shown as
  // raw source instead of the structured contact card (CPE-1436, epic CPE-1433).
  {
    id: "vcard",
    label: "Contact",
    kind: "vcard",
    editable: false,
    canPreview: (e) => !e.is_dir && VCARD_EXT.has(e.extension),
  },
  // Must precede the generic text provider: .jwt/.jws have no CATEGORY_BY_EXT entry today (would
  // otherwise fall through to text/hex), and .pem/.crt/.cer/.csr categorise as "code" text.
  {
    id: "jwt",
    label: "JWT",
    kind: "jwt",
    editable: false,
    canPreview: (e) => !e.is_dir && JWT_EXT.has(e.extension),
    // Worked example for the action-bar mechanism (CPE-1570, epic CPE-1568): migrated off the hard-coded
    // inline copy buttons `JwtPreview.svelte` used to render itself. A JWT has no "file text" in the
    // pane's usual sense (it's a structured decode, not raw source), so the component reports its two
    // copyable JSON blobs up via `ctx.values`, keyed by these action ids.
    actions: [
      {
        id: "copy-claims",
        labelKey: "pv.action.copyClaims",
        icon: "copy",
        enabled: (ctx) => !!ctx.values["copy-claims"],
        run: (ctx) => ctx.copyToClipboard(ctx.values["copy-claims"] ?? ""),
      },
      {
        id: "copy-header",
        labelKey: "pv.action.copyHeader",
        icon: "copy",
        enabled: (ctx) => !!ctx.values["copy-header"],
        run: (ctx) => ctx.copyToClipboard(ctx.values["copy-header"] ?? ""),
      },
    ],
  },
  {
    id: "cert",
    label: "Certificate",
    kind: "cert",
    editable: false,
    canPreview: (e) => !e.is_dir && CERT_EXT.has(e.extension),
  },
  {
    id: "data-grid",
    label: "Data",
    kind: "data-grid",
    editable: false,
    canPreview: (e) => !e.is_dir && DATA_GRID_EXT.has(e.extension),
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
  // Last resort for any file no richer provider claimed (unrecognised binary): a paged hex/ASCII view
  // (CPE-773) — more useful than the metadata fallback for binary content. Folders still fall through to
  // FALLBACK via pickProvider's dir guard.
  {
    id: "hex",
    label: "Hex",
    kind: "hex",
    editable: false,
    canPreview: (e) => !e.is_dir,
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
 * A highlighted DIRECTORY (CPE-1426): rendered as a one-level "peek" browser (`FolderBrowser.svelte`)
 * instead of the metadata-only fallback — clicking a row in it drives the main pane's navigate + select,
 * so a folder tree can be walked entirely from the preview pane (Miller-columns / macOS Finder
 * column-view feel). Not part of the ordered `providers` list above (those all guard `!e.is_dir`) —
 * `pickProvider` below routes directories here directly, before ever consulting that list.
 */
export const FOLDER: PreviewProvider = {
  id: "folder",
  label: "Folder",
  kind: "folder",
  editable: false,
  canPreview: (e) => e.is_dir,
};

/**
 * Pick the best preview provider for an entry. Never returns null: nothing selected and unrecognised
 * files resolve to the metadata fallback; a directory resolves to the folder-peek browser (CPE-1426).
 */
export function pickProvider(entry: DirEntry | null | undefined): PreviewProvider {
  if (!entry) return FALLBACK;
  if (entry.is_dir) return FOLDER;
  return providers.find((p) => p.canPreview(entry)) ?? FALLBACK;
}

/**
 * Which element a `media`-kind entry should render in (CPE-1429). Video files use `<video>`; everything
 * else in the media kind (including the ambiguous `.ogg`, which `filetypes.ts` maps to audio) uses
 * `<audio>`. Derived from the same `categoryOf` mapping the provider matches on, so the two never
 * disagree.
 */
export function mediaType(entry: DirEntry): "audio" | "video" {
  return categoryOf(entry) === "video" ? "video" : "audio";
}
