/**
 * The file types offered by the New ▸ submenu beyond the two built-ins (Folder + Text file).
 * Single source of truth (CPE-1161): the ContextMenu renders these rows, and App.svelte maps the
 * `new-file:<ext>` / `new-file-in:<ext>` / `drive-new-file:<ext>` action back to a spec here to
 * decide how the file is created — so adding a type is one entry, not nine hand-wired actions × 3
 * menus.
 *
 * Creation strategy per spec:
 *   - default (neither `content` nor `zip`): an EMPTY file (json/yaml/xml/html/css/js/py/csv/md are
 *     fine empty — the user fills them in), via the existing `create_file` command;
 *   - `content`: a minimal valid stub written at create time (Rich Text needs `{\rtf1\ansi }` to be a
 *     valid .rtf), via `create_file_with_content`;
 *   - `zip`: a VALID EMPTY archive (an empty file is not a valid .zip), via `create_empty_zip`.
 *
 * Grouping mirrors MENUS.md readability: text-ish formats, then data/code formats, then the archive —
 * the menu draws a separator between groups. The existing "Text file" (.txt) row sits just above the
 * first group; Folder above that.
 */
export interface NewFileType {
  /** Extension WITHOUT the leading dot. Also the suffix carried in the dispatched action. */
  ext: string;
  /** i18n key for the menu label. */
  labelKey: string;
  /** Icon glyph (reuses existing ContextMenu glyphs). */
  icon: string;
  /** Default base filename (before the extension), Explorer-style. */
  base: string;
  /** Minimal valid stub content written at create time; omit for an empty file. */
  content?: string;
  /** Create a valid empty .zip archive instead of a plain file. */
  zip?: boolean;
}

export const NEW_FILE_TYPE_GROUPS: NewFileType[][] = [
  // Text-ish — sits with the built-in "Text file" row.
  [
    { ext: "md", labelKey: "ctx.newMarkdown", icon: "document", base: "New Markdown File" },
    {
      ext: "rtf",
      labelKey: "ctx.newRichText",
      icon: "document",
      base: "New Rich Text Document",
      content: "{\\rtf1\\ansi }",
    },
  ],
  // Data / code formats — fine empty.
  [
    { ext: "json", labelKey: "ctx.newJson", icon: "code", base: "New JSON File" },
    { ext: "yaml", labelKey: "ctx.newYaml", icon: "code", base: "New YAML File" },
    { ext: "xml", labelKey: "ctx.newXml", icon: "code", base: "New XML File" },
    { ext: "html", labelKey: "ctx.newHtml", icon: "code", base: "New HTML File" },
    { ext: "css", labelKey: "ctx.newCss", icon: "code", base: "New CSS File" },
    { ext: "js", labelKey: "ctx.newJs", icon: "code", base: "New JavaScript File" },
    { ext: "py", labelKey: "ctx.newPython", icon: "code", base: "New Python File" },
    { ext: "csv", labelKey: "ctx.newCsv", icon: "filter", base: "New CSV File" },
  ],
  // Archive.
  [
    {
      ext: "zip",
      labelKey: "ctx.newZip",
      icon: "archive",
      base: "New Compressed (zipped) Folder",
      zip: true,
    },
  ],
];

/** Flat list of the extra New ▸ file types (all groups). */
export const NEW_FILE_TYPES: readonly NewFileType[] = NEW_FILE_TYPE_GROUPS.flat();

/** Lookup by extension (no dot) — used by App.svelte to resolve a dispatched `new-file:<ext>`. */
export const NEW_FILE_TYPE_BY_EXT: Readonly<Record<string, NewFileType>> = Object.fromEntries(
  NEW_FILE_TYPES.map((t) => [t.ext, t]),
);
