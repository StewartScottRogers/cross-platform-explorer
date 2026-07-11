import type { DirEntry } from "./types";

/** Broad visual categories used to pick an icon. */
export type FileCategory =
  | "folder"
  | "image"
  | "document"
  | "spreadsheet"
  | "presentation"
  | "pdf"
  | "code"
  | "archive"
  | "audio"
  | "video"
  | "text"
  | "unknown";

const CATEGORY_BY_EXT: Record<string, FileCategory> = {
  // images
  png: "image", jpg: "image", jpeg: "image", gif: "image", bmp: "image",
  webp: "image", svg: "image", ico: "image", tif: "image", tiff: "image",
  // documents
  doc: "document", docx: "document", odt: "document", rtf: "document",
  // spreadsheets
  xls: "spreadsheet", xlsx: "spreadsheet", csv: "spreadsheet", ods: "spreadsheet",
  // presentations
  ppt: "presentation", pptx: "presentation", odp: "presentation",
  // pdf
  pdf: "pdf",
  // code
  ts: "code", tsx: "code", js: "code", jsx: "code", rs: "code", py: "code",
  json: "code", html: "code", css: "code", svelte: "code", toml: "code",
  yml: "code", yaml: "code", sh: "code", ps1: "code", cmd: "code", bat: "code",
  cs: "code", sln: "code", slnx: "code", xml: "code",
  // archives
  zip: "archive", rar: "archive", "7z": "archive", tar: "archive", gz: "archive",
  // audio
  mp3: "audio", wav: "audio", flac: "audio", m4a: "audio", ogg: "audio",
  // video
  mp4: "video", mkv: "video", mov: "video", avi: "video", webm: "video",
  // text
  txt: "text", md: "text", log: "text", ini: "text", cfg: "text",
};

const TYPE_NAME_BY_EXT: Record<string, string> = {
  png: "PNG image", jpg: "JPEG image", jpeg: "JPEG image", gif: "GIF image",
  bmp: "Bitmap image", webp: "WebP image", svg: "SVG image", ico: "Icon",
  doc: "Word document", docx: "Word document", odt: "OpenDocument text",
  rtf: "Rich Text document",
  xls: "Excel worksheet", xlsx: "Excel worksheet", csv: "CSV file",
  ods: "OpenDocument spreadsheet",
  ppt: "PowerPoint presentation", pptx: "PowerPoint presentation",
  pdf: "PDF document",
  ts: "TypeScript file", tsx: "TypeScript file", js: "JavaScript file",
  jsx: "JavaScript file", rs: "Rust source file", py: "Python file",
  json: "JSON file", html: "HTML document", css: "Cascading Style Sheet",
  svelte: "Svelte component", toml: "TOML file", yml: "YAML file",
  yaml: "YAML file", sh: "Shell script", ps1: "PowerShell script",
  cmd: "Windows Command Script", bat: "Windows Batch File",
  xml: "XML document", sln: "Visual Studio Solution", slnx: "Visual Studio Solution",
  zip: "Compressed (zipped) Folder", rar: "RAR archive", "7z": "7z archive",
  tar: "TAR archive", gz: "GZ archive",
  mp3: "MP3 audio", wav: "WAV audio", flac: "FLAC audio", m4a: "M4A audio",
  ogg: "OGG audio",
  mp4: "MP4 video", mkv: "Matroska video", mov: "QuickTime movie",
  avi: "AVI video", webm: "WebM video",
  txt: "Text Document", md: "Markdown file", log: "Log file",
  ini: "Configuration settings", cfg: "Configuration file",
  exe: "Application", msi: "Windows Installer Package", dll: "Application extension",
};

/** Visual category for an entry, used to choose its icon. */
export function categoryOf(entry: Pick<DirEntry, "is_dir" | "extension">): FileCategory {
  if (entry.is_dir) return "folder";
  return CATEGORY_BY_EXT[entry.extension] ?? "unknown";
}

/**
 * Human-readable type, as shown in Explorer's "Type" column.
 * Folders are "File folder"; known extensions get a friendly name; unknown
 * extensions fall back to "XYZ File"; extensionless files are just "File".
 */
export function typeName(entry: Pick<DirEntry, "is_dir" | "extension">): string {
  if (entry.is_dir) return "File folder";
  const ext = entry.extension;
  if (!ext) return "File";
  return TYPE_NAME_BY_EXT[ext] ?? `${ext.toUpperCase()} File`;
}
