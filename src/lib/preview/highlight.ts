/**
 * Syntax highlighting for code previews. Uses the highlight.js "common" bundle
 * (~36 popular languages registered in one import), plus an extension→language
 * map. Output is HTML with hljs-* token spans; highlight.js HTML-escapes the
 * source, so the result is safe to inject with {@html}.
 */
import hljs from "highlight.js/lib/common";

const LANG_BY_EXT: Record<string, string> = {
  // JS/TS/web
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
  ts: "typescript", tsx: "typescript",
  json: "json", css: "css", scss: "scss", less: "less",
  html: "xml", xml: "xml", svelte: "xml",
  // systems / general
  rs: "rust", go: "go", c: "c",
  h: "cpp", hpp: "cpp", hh: "cpp", cpp: "cpp", cc: "cpp", cxx: "cpp",
  cs: "csharp", m: "objectivec", mm: "objectivec",
  java: "java", kt: "kotlin", kts: "kotlin", swift: "swift",
  vb: "vbnet",
  // scripting
  py: "python", rb: "ruby", php: "php", pl: "perl", pm: "perl",
  lua: "lua", r: "r", sh: "bash", bash: "bash",
  // data / config / misc
  yml: "yaml", yaml: "yaml", toml: "ini", ini: "ini", cfg: "ini",
  sql: "sql", graphql: "graphql", gql: "graphql", wat: "wasm",
  mk: "makefile", diff: "diff", patch: "diff",
  md: "markdown", markdown: "markdown",
};

/** The highlight.js language for an extension, or null if we don't highlight it. */
export function languageForExt(ext: string): string | null {
  const lang = LANG_BY_EXT[ext];
  return lang && hljs.getLanguage(lang) ? lang : null;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/**
 * Highlight code for the given file extension, returning HTML safe for {@html}.
 * Falls back to escaped plain text when the extension has no mapped/registered
 * language, so callers can always inject the result.
 */
export function highlightCode(code: string, ext: string): string {
  const lang = languageForExt(ext);
  if (lang) {
    return hljs.highlight(code, { language: lang }).value;
  }
  return escapeHtml(code);
}
