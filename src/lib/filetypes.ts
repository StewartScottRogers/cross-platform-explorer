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
  | "executable"
  | "unknown";

export const CATEGORY_BY_EXT: Record<string, FileCategory> = {
  // images
  png: "image", jpg: "image", jpeg: "image", gif: "image", bmp: "image",
  webp: "image", svg: "image", ico: "image", tif: "image", tiff: "image",
  heic: "image", avif: "image", jfif: "image",
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
  cs: "code", sln: "code", slnx: "code", xml: "code", mjs: "code", cjs: "code",
  // more languages (highlight.js common bundle — CPE-120..178)
  c: "code", h: "code", cpp: "code", cc: "code", cxx: "code", hpp: "code", hh: "code",
  m: "code", mm: "code", go: "code", java: "code", kt: "code", kts: "code",
  swift: "code", rb: "code", php: "code", pl: "code", pm: "code", lua: "code",
  r: "code", vb: "code", graphql: "code", gql: "code", wat: "code",
  scss: "code", sass: "code", less: "code", sql: "code", mk: "code",
  // more languages via individually-registered grammars (CPE-128..205)
  scala: "code", jl: "code", dart: "code", hs: "code", ex: "code", exs: "code",
  erl: "code", clj: "code", cljs: "code", edn: "code", fs: "code", fsx: "code",
  ml: "code", mli: "code", elm: "code", re: "code", rei: "code", rkt: "code",
  scm: "code", ss: "code", lisp: "code", lsp: "code", nim: "code", cr: "code",
  d: "code", f90: "code", f95: "code", pas: "code", adb: "code", ads: "code",
  prolog: "code", pro: "code", asm: "code", s: "code", ll: "code", v: "code",
  sv: "code", vhd: "code", vhdl: "code", tcl: "code", awk: "code", hx: "code",
  groovy: "code", gradle: "code", psm1: "code", psd1: "code", cmake: "code",
  proto: "code", nginx: "code", nix: "code", styl: "code", hbs: "code",
  twig: "code", xsl: "code", xslt: "code", json5: "code", jsonc: "code",
  ron: "code",
  // data/config text (CPE-083..115)
  diff: "code", patch: "code", properties: "code", ndjson: "code",
  jsonl: "code", tsv: "spreadsheet", tab: "spreadsheet",
  // XML- and JSON-derived data formats (CPE-082/094/206/207/208/211)
  geojson: "code", gpx: "code", kml: "code", musicxml: "code", plist: "code",
  // infra / config formats (CPE-191/192/193)
  tf: "code", hcl: "code", tfvars: "code", dhall: "code",
  jsonnet: "code", libsonnet: "code",
  // markup / doc source formats (CPE-073/074/075/076/188/189/190)
  tex: "code", rst: "code", adoc: "code", asciidoc: "code", org: "code",
  mdx: "code", textile: "code", bib: "code",
  // languages & templates (CPE-146/150/158/180/181/183/184/185)
  cbl: "code", cob: "code", cpy: "code", zig: "code", sol: "code",
  vue: "code", astro: "code", pug: "code", ejs: "code", liquid: "code",
  // text-based data / diagram / comms formats (CPE-079/092/093/106/119/202/203/204/209/212/213)
  dot: "code", gv: "code", puml: "code", plantuml: "code", mmd: "code",
  mermaid: "code", fasta: "code", fa: "code", fna: "code", faa: "code",
  wkt: "code", eml: "code", ics: "code", vcf: "code", srt: "code",
  vtt: "code", pem: "code", crt: "code", cer: "code", csr: "code",
  key: "code", reg: "code",
  // Jupyter notebooks are JSON documents (CPE-114)
  ipynb: "code",
  // archives
  zip: "archive", rar: "archive", "7z": "archive", tar: "archive", gz: "archive",
  xz: "archive", bz2: "archive", zst: "archive", tgz: "archive",
  // zip-based application packages (CPE-217)
  jar: "archive", apk: "archive", war: "archive", ear: "archive",
  ipa: "archive", xpi: "archive",
  // audio
  mp3: "audio", wav: "audio", flac: "audio", m4a: "audio", ogg: "audio",
  aac: "audio", opus: "audio",
  // video
  mp4: "video", mkv: "video", mov: "video", avi: "video", webm: "video",
  wmv: "video", flv: "video", m4v: "video",
  // text
  txt: "text", md: "text", log: "text", ini: "text", cfg: "text",
  // executables
  exe: "executable", msi: "executable", dll: "executable",
};

export const TYPE_NAME_BY_EXT: Record<string, string> = {
  png: "PNG image", jpg: "JPEG image", jpeg: "JPEG image", gif: "GIF image",
  bmp: "Bitmap image", webp: "WebP image", svg: "SVG image", ico: "Icon",
  heic: "HEIC image", avif: "AVIF image", jfif: "JPEG image",
  doc: "Word document", docx: "Word document", odt: "OpenDocument text",
  rtf: "Rich Text document",
  xls: "Excel worksheet", xlsx: "Excel worksheet", csv: "CSV file",
  ods: "OpenDocument spreadsheet",
  ppt: "PowerPoint presentation", pptx: "PowerPoint presentation",
  pdf: "PDF document",
  ts: "TypeScript file", tsx: "TypeScript file", js: "JavaScript file",
  mjs: "JavaScript module", cjs: "JavaScript module",
  jsx: "JavaScript file", rs: "Rust source file", py: "Python file",
  json: "JSON file", html: "HTML document", css: "Cascading Style Sheet",
  svelte: "Svelte component", toml: "TOML file", yml: "YAML file",
  yaml: "YAML file", sh: "Shell script", ps1: "PowerShell script",
  cmd: "Windows Command Script", bat: "Windows Batch File",
  xml: "XML document", sln: "Visual Studio Solution", slnx: "Visual Studio Solution",
  c: "C source file", h: "C/C++ header", cpp: "C++ source file", cc: "C++ source file",
  cxx: "C++ source file", hpp: "C++ header", hh: "C++ header",
  m: "Objective-C source", mm: "Objective-C++ source", go: "Go source file",
  java: "Java source file", kt: "Kotlin source file", kts: "Kotlin script",
  swift: "Swift source file", rb: "Ruby source file", php: "PHP source file",
  pl: "Perl script", pm: "Perl module", lua: "Lua script", r: "R script",
  vb: "Visual Basic source", graphql: "GraphQL schema", gql: "GraphQL schema",
  wat: "WebAssembly text", scss: "SCSS stylesheet", less: "Less stylesheet",
  sql: "SQL script", mk: "Makefile",
  geojson: "GeoJSON file", gpx: "GPX GPS track", kml: "KML geographic data",
  musicxml: "MusicXML score", plist: "Apple property list",
  tf: "Terraform file", hcl: "HCL file", tfvars: "Terraform variables",
  dhall: "Dhall config", jsonnet: "Jsonnet file", libsonnet: "Jsonnet library",
  tex: "LaTeX source", rst: "reStructuredText", adoc: "AsciiDoc",
  asciidoc: "AsciiDoc", org: "Org-mode document", mdx: "MDX document",
  textile: "Textile document", bib: "BibTeX bibliography",
  cbl: "COBOL source", cob: "COBOL source", cpy: "COBOL copybook",
  zig: "Zig source", sol: "Solidity source", vue: "Vue component",
  astro: "Astro component", pug: "Pug template", ejs: "EJS template",
  liquid: "Liquid template",
  dot: "Graphviz DOT", gv: "Graphviz DOT", puml: "PlantUML diagram",
  plantuml: "PlantUML diagram", mmd: "Mermaid diagram", mermaid: "Mermaid diagram",
  fasta: "FASTA sequence", fa: "FASTA sequence", fna: "FASTA sequence",
  faa: "FASTA sequence", wkt: "WKT geometry", eml: "Email message",
  ics: "iCalendar file", vcf: "vCard contact", srt: "Subtitle file",
  vtt: "WebVTT subtitle", pem: "PEM certificate", crt: "Certificate",
  cer: "Certificate", csr: "Certificate request", key: "Key file",
  reg: "Registry export", ipynb: "Jupyter notebook",
  zip: "Compressed (zipped) Folder", rar: "RAR archive", "7z": "7z archive",
  tar: "TAR archive", gz: "GZ archive", xz: "XZ archive", bz2: "BZ2 archive",
  zst: "Zstandard archive", tgz: "Gzipped TAR archive",
  jar: "Java archive", apk: "Android package", war: "Web application archive",
  ear: "Enterprise archive", ipa: "iOS app archive", xpi: "Firefox add-on",
  mp3: "MP3 audio", wav: "WAV audio", flac: "FLAC audio", m4a: "M4A audio",
  ogg: "OGG audio", aac: "AAC audio", opus: "Opus audio",
  mp4: "MP4 video", mkv: "Matroska video", mov: "QuickTime movie",
  avi: "AVI video", webm: "WebM video", wmv: "Windows Media Video",
  flv: "Flash video", m4v: "MPEG-4 video",
  txt: "Text Document", md: "Markdown file", log: "Log file",
  ini: "Configuration settings", cfg: "Configuration file",
  exe: "Application", msi: "Windows Installer Package", dll: "Application extension",
};

/**
 * Well-known code files that have no useful extension (Dockerfile, Makefile,
 * dot-config files, …). Matched by full lowercased name.
 */
const CODE_FILENAMES = new Set([
  "dockerfile", "containerfile", "makefile", "gnumakefile", "cmakelists.txt",
  "rakefile", "gemfile", "brewfile", "procfile", "vagrantfile",
  ".gitignore", ".gitattributes", ".gitconfig", ".gitmodules",
  ".npmrc", ".yarnrc", ".editorconfig", ".env",
  ".bashrc", ".zshrc", ".bash_profile", ".profile",
]);

/** Visual category for an entry, used to choose its icon and preview provider. */
export function categoryOf(
  entry: Pick<DirEntry, "is_dir" | "extension"> & { name?: string },
): FileCategory {
  if (entry.is_dir) return "folder";
  if (entry.name && CODE_FILENAMES.has(entry.name.toLowerCase())) return "code";
  return CATEGORY_BY_EXT[entry.extension] ?? "unknown";
}

/**
 * More specific per-extension row icons (CPE-233). Distinct glyphs for common
 * formats that would otherwise share a broad category (or fall to "unknown").
 * Deliberately separate from CATEGORY_BY_EXT so the PREVIEW provider selection
 * (driven by categoryOf) is unaffected — this only makes the list icon richer.
 */
const ICON_BY_EXT: Record<string, string> = {
  // fonts
  ttf: "font", otf: "font", woff: "font", woff2: "font", eot: "font",
  // disk images
  iso: "disk", img: "disk", dmg: "disk", vhd: "disk", vhdx: "disk", vmdk: "disk",
  // databases
  db: "database", sqlite: "database", sqlite3: "database", db3: "database",
  sql: "database", parquet: "database", mdb: "database", accdb: "database",
  // ebooks
  epub: "ebook", mobi: "ebook", azw: "ebook", azw3: "ebook", fb2: "ebook",
  // certificates / keys
  pem: "certificate", crt: "certificate", cer: "certificate", csr: "certificate",
  key: "certificate", der: "certificate", p12: "certificate", pfx: "certificate",
  // 3D models
  stl: "cube", obj: "cube", gltf: "cube", glb: "cube", "3mf": "cube",
  fbx: "cube", dae: "cube", ply: "cube",
  // web pages
  html: "web", htm: "web",
};

/** Extensions treated as executable — eligible for Execute / Run as admin (CPE-241). */
const EXECUTABLE_EXTS = new Set(["exe", "cmd", "bat", "msi", "com", "ps1", "scr", "vbs"]);

/** True when the entry is a runnable executable/script (by extension). */
export const isExecutable = (entry: Pick<DirEntry, "is_dir" | "extension">): boolean =>
  !entry.is_dir && EXECUTABLE_EXTS.has(entry.extension);

/**
 * Icon name for an entry's row/tile. Prefers a format-specific glyph, then the
 * broad category icon, then "unknown". Folders are always the folder icon.
 */
export function iconFor(
  entry: Pick<DirEntry, "is_dir" | "extension"> & { name?: string },
): string {
  if (entry.is_dir) return "folder";
  return (entry.extension && ICON_BY_EXT[entry.extension]) || categoryOf(entry);
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
