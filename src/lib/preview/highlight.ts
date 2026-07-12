/**
 * Syntax highlighting for code previews, with LAZY-LOADED grammars.
 *
 * We start from highlight.js core (no languages) and dynamically import each
 * grammar the first time a file needing it is previewed. Vite splits every
 * import() below into its own chunk, so the initial bundle carries none of the
 * ~70 grammars — they load on demand. Highlighting is therefore async: callers
 * `await ensureLanguageForName(name)` and then call `highlightForFile`, which
 * escapes-plain until the grammar is registered.
 */
import hljs from "highlight.js/lib/core";

/* eslint-disable @typescript-eslint/no-explicit-any */
type Loader = () => Promise<{ default: any }>;

/** Language name → dynamic import of its grammar. One Vite chunk each. */
const LOADERS: Record<string, Loader> = {
  javascript: () => import("highlight.js/lib/languages/javascript"),
  typescript: () => import("highlight.js/lib/languages/typescript"),
  json: () => import("highlight.js/lib/languages/json"),
  xml: () => import("highlight.js/lib/languages/xml"),
  css: () => import("highlight.js/lib/languages/css"),
  scss: () => import("highlight.js/lib/languages/scss"),
  less: () => import("highlight.js/lib/languages/less"),
  stylus: () => import("highlight.js/lib/languages/stylus"),
  handlebars: () => import("highlight.js/lib/languages/handlebars"),
  twig: () => import("highlight.js/lib/languages/twig"),
  markdown: () => import("highlight.js/lib/languages/markdown"),
  rust: () => import("highlight.js/lib/languages/rust"),
  go: () => import("highlight.js/lib/languages/go"),
  c: () => import("highlight.js/lib/languages/c"),
  cpp: () => import("highlight.js/lib/languages/cpp"),
  csharp: () => import("highlight.js/lib/languages/csharp"),
  objectivec: () => import("highlight.js/lib/languages/objectivec"),
  java: () => import("highlight.js/lib/languages/java"),
  kotlin: () => import("highlight.js/lib/languages/kotlin"),
  swift: () => import("highlight.js/lib/languages/swift"),
  vbnet: () => import("highlight.js/lib/languages/vbnet"),
  d: () => import("highlight.js/lib/languages/d"),
  nim: () => import("highlight.js/lib/languages/nim"),
  crystal: () => import("highlight.js/lib/languages/crystal"),
  scala: () => import("highlight.js/lib/languages/scala"),
  dart: () => import("highlight.js/lib/languages/dart"),
  haxe: () => import("highlight.js/lib/languages/haxe"),
  nix: () => import("highlight.js/lib/languages/nix"),
  haskell: () => import("highlight.js/lib/languages/haskell"),
  elixir: () => import("highlight.js/lib/languages/elixir"),
  erlang: () => import("highlight.js/lib/languages/erlang"),
  clojure: () => import("highlight.js/lib/languages/clojure"),
  fsharp: () => import("highlight.js/lib/languages/fsharp"),
  ocaml: () => import("highlight.js/lib/languages/ocaml"),
  elm: () => import("highlight.js/lib/languages/elm"),
  reasonml: () => import("highlight.js/lib/languages/reasonml"),
  scheme: () => import("highlight.js/lib/languages/scheme"),
  lisp: () => import("highlight.js/lib/languages/lisp"),
  julia: () => import("highlight.js/lib/languages/julia"),
  python: () => import("highlight.js/lib/languages/python"),
  ruby: () => import("highlight.js/lib/languages/ruby"),
  php: () => import("highlight.js/lib/languages/php"),
  perl: () => import("highlight.js/lib/languages/perl"),
  lua: () => import("highlight.js/lib/languages/lua"),
  r: () => import("highlight.js/lib/languages/r"),
  bash: () => import("highlight.js/lib/languages/bash"),
  tcl: () => import("highlight.js/lib/languages/tcl"),
  awk: () => import("highlight.js/lib/languages/awk"),
  groovy: () => import("highlight.js/lib/languages/groovy"),
  powershell: () => import("highlight.js/lib/languages/powershell"),
  verilog: () => import("highlight.js/lib/languages/verilog"),
  vhdl: () => import("highlight.js/lib/languages/vhdl"),
  x86asm: () => import("highlight.js/lib/languages/x86asm"),
  llvm: () => import("highlight.js/lib/languages/llvm"),
  wasm: () => import("highlight.js/lib/languages/wasm"),
  fortran: () => import("highlight.js/lib/languages/fortran"),
  delphi: () => import("highlight.js/lib/languages/delphi"),
  ada: () => import("highlight.js/lib/languages/ada"),
  prolog: () => import("highlight.js/lib/languages/prolog"),
  yaml: () => import("highlight.js/lib/languages/yaml"),
  ini: () => import("highlight.js/lib/languages/ini"),
  sql: () => import("highlight.js/lib/languages/sql"),
  graphql: () => import("highlight.js/lib/languages/graphql"),
  protobuf: () => import("highlight.js/lib/languages/protobuf"),
  cmake: () => import("highlight.js/lib/languages/cmake"),
  nginx: () => import("highlight.js/lib/languages/nginx"),
  makefile: () => import("highlight.js/lib/languages/makefile"),
  diff: () => import("highlight.js/lib/languages/diff"),
  dockerfile: () => import("highlight.js/lib/languages/dockerfile"),
  latex: () => import("highlight.js/lib/languages/latex"),
  asciidoc: () => import("highlight.js/lib/languages/asciidoc"),
  accesslog: () => import("highlight.js/lib/languages/accesslog"),
};
/* eslint-enable @typescript-eslint/no-explicit-any */

const LANG_BY_EXT: Record<string, string> = {
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
  ts: "typescript", tsx: "typescript",
  json: "json", json5: "json", jsonc: "json",
  css: "css", scss: "scss", sass: "scss", less: "less", styl: "stylus",
  html: "xml", xml: "xml", svelte: "xml", xsl: "xml", xslt: "xml",
  hbs: "handlebars", twig: "twig",
  rs: "rust", ron: "rust", go: "go", c: "c",
  h: "cpp", hpp: "cpp", hh: "cpp", cpp: "cpp", cc: "cpp", cxx: "cpp",
  cs: "csharp", m: "objectivec", mm: "objectivec",
  java: "java", kt: "kotlin", kts: "kotlin", swift: "swift",
  vb: "vbnet", d: "d", nim: "nim", cr: "crystal",
  scala: "scala", dart: "dart", hx: "haxe", nix: "nix",
  hs: "haskell", ex: "elixir", exs: "elixir", erl: "erlang",
  clj: "clojure", cljs: "clojure", edn: "clojure",
  fs: "fsharp", fsx: "fsharp", ml: "ocaml", mli: "ocaml",
  elm: "elm", re: "reasonml", rei: "reasonml",
  rkt: "scheme", scm: "scheme", ss: "scheme", lisp: "lisp", lsp: "lisp",
  jl: "julia",
  py: "python", rb: "ruby", php: "php", pl: "perl", pm: "perl",
  lua: "lua", r: "r", sh: "bash", bash: "bash", tcl: "tcl", awk: "awk",
  groovy: "groovy", gradle: "groovy",
  ps1: "powershell", psm1: "powershell", psd1: "powershell",
  v: "verilog", sv: "verilog", vhd: "vhdl", vhdl: "vhdl",
  asm: "x86asm", s: "x86asm", ll: "llvm", wat: "wasm",
  f90: "fortran", f95: "fortran", pas: "delphi", adb: "ada", ads: "ada",
  prolog: "prolog", pro: "prolog",
  yml: "yaml", yaml: "yaml", toml: "ini", ini: "ini", cfg: "ini",
  editorconfig: "ini", properties: "ini", ndjson: "json", jsonl: "json",
  sql: "sql", graphql: "graphql", gql: "graphql",
  proto: "protobuf", cmake: "cmake", nginx: "nginx",
  mk: "makefile", diff: "diff", patch: "diff",
  md: "markdown", markdown: "markdown",
  // XML/JSON-derived data formats reuse the xml/json grammars (CPE-094/206/207/208/211)
  geojson: "json", gpx: "xml", kml: "xml", musicxml: "xml", plist: "xml",
  // Jsonnet is a JSON superset — reuse the json grammar (CPE-193)
  jsonnet: "json", libsonnet: "json",
  // markup / doc formats (CPE-073/075/188)
  tex: "latex", adoc: "asciidoc", asciidoc: "asciidoc", mdx: "markdown",
};

const LANG_BY_FILENAME: Record<string, string> = {
  dockerfile: "dockerfile", containerfile: "dockerfile",
  makefile: "makefile", gnumakefile: "makefile",
  "cmakelists.txt": "cmake",
  gemfile: "ruby", rakefile: "ruby",
  ".gitconfig": "ini", ".editorconfig": "ini", ".npmrc": "ini", ".yarnrc": "ini",
  ".env": "ini",
  ".bashrc": "bash", ".zshrc": "bash", ".bash_profile": "bash", ".profile": "bash",
};

/** The language for an extension if we have a loader for it, else null. */
export function languageForExt(ext: string): string | null {
  const lang = LANG_BY_EXT[ext];
  return lang && LOADERS[lang] ? lang : null;
}

/** Resolve a language from a full file name (special names first, then ext). */
export function languageForName(name: string): string | null {
  const lower = name.toLowerCase();
  const byName = LANG_BY_FILENAME[lower];
  if (byName && LOADERS[byName]) return byName;
  const ext = lower.includes(".") ? lower.slice(lower.lastIndexOf(".") + 1) : "";
  return languageForExt(ext);
}

/** Register a language grammar on demand (idempotent). */
export async function ensureLanguage(lang: string): Promise<void> {
  if (hljs.getLanguage(lang) || !LOADERS[lang]) return;
  const mod = await LOADERS[lang]();
  if (!hljs.getLanguage(lang)) hljs.registerLanguage(lang, mod.default);
}

/** Ensure the grammar for a file name is loaded; resolves true if one applies. */
export async function ensureLanguageForName(name: string): Promise<boolean> {
  const lang = languageForName(name);
  if (!lang) return false;
  await ensureLanguage(lang);
  return true;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/**
 * Highlight code for the given file name, returning HTML safe for {@html}.
 * Only highlights if the grammar is already registered (call
 * ensureLanguageForName first); otherwise returns escaped plain text.
 */
export function highlightForFile(code: string, name: string): string {
  const lang = languageForName(name);
  if (lang && hljs.getLanguage(lang)) {
    return hljs.highlight(code, { language: lang }).value;
  }
  return escapeHtml(code);
}
