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
  | "audio"
  | "video"
  | "pdf"
  | "json"
  | "csv"
  | "markdown"
  | "text"
  | "none";

export interface PreviewProvider {
  /** Stable id. */
  id: string;
  /** Human-readable label (for a future provider picker / diagnostics). */
  label: string;
  /** How the pane should render this preview. */
  kind: PreviewKind;
  /** Whether this provider can preview the given entry. */
  canPreview(entry: DirEntry): boolean;
}

const MARKDOWN_EXT = new Set(["md", "markdown"]);

/**
 * Ordered by priority — the first match wins. Markdown is listed before text
 * because a `.md` file's category is "text"; without the ordering, text would
 * claim it first.
 */
export const providers: PreviewProvider[] = [
  {
    id: "image",
    label: "Image",
    kind: "image",
    canPreview: (e) => !e.is_dir && categoryOf(e) === "image",
  },
  {
    id: "audio",
    label: "Audio",
    kind: "audio",
    canPreview: (e) => !e.is_dir && categoryOf(e) === "audio",
  },
  {
    id: "video",
    label: "Video",
    kind: "video",
    canPreview: (e) => !e.is_dir && categoryOf(e) === "video",
  },
  {
    id: "pdf",
    label: "PDF",
    kind: "pdf",
    canPreview: (e) => !e.is_dir && categoryOf(e) === "pdf",
  },
  // JSON and CSV are declared before the generic text provider because their
  // categories ("code"/"spreadsheet") would otherwise be claimed by it / skipped.
  {
    id: "json",
    label: "JSON",
    kind: "json",
    canPreview: (e) => !e.is_dir && e.extension === "json",
  },
  {
    id: "csv",
    label: "CSV",
    kind: "csv",
    canPreview: (e) => !e.is_dir && e.extension === "csv",
  },
  {
    id: "markdown",
    label: "Markdown",
    kind: "markdown",
    canPreview: (e) => !e.is_dir && MARKDOWN_EXT.has(e.extension),
  },
  {
    id: "text",
    label: "Text",
    kind: "text",
    canPreview: (e) =>
      !e.is_dir && (categoryOf(e) === "text" || categoryOf(e) === "code"),
  },
];

/** Used when no richer provider matches — the pane shows metadata instead. */
export const FALLBACK: PreviewProvider = {
  id: "none",
  label: "Details",
  kind: "none",
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
