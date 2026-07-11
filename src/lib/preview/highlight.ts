/**
 * Syntax highlighting for code previews. Uses highlight.js core with a curated
 * set of registered languages so the bundle only carries what we actually use
 * (the full auto-detect build is large). Output is HTML with `hljs-*` token
 * spans; highlight.js HTML-escapes the source text, so the result is safe to
 * inject with `{@html}`.
 */
import hljs from "highlight.js/lib/core";
import javascript from "highlight.js/lib/languages/javascript";
import typescript from "highlight.js/lib/languages/typescript";
import json from "highlight.js/lib/languages/json";
import css from "highlight.js/lib/languages/css";
import xml from "highlight.js/lib/languages/xml";
import rust from "highlight.js/lib/languages/rust";
import python from "highlight.js/lib/languages/python";
import bash from "highlight.js/lib/languages/bash";
import yaml from "highlight.js/lib/languages/yaml";
import ini from "highlight.js/lib/languages/ini";

hljs.registerLanguage("javascript", javascript);
hljs.registerLanguage("typescript", typescript);
hljs.registerLanguage("json", json);
hljs.registerLanguage("css", css);
hljs.registerLanguage("xml", xml);
hljs.registerLanguage("rust", rust);
hljs.registerLanguage("python", python);
hljs.registerLanguage("bash", bash);
hljs.registerLanguage("yaml", yaml);
hljs.registerLanguage("ini", ini);

const LANG_BY_EXT: Record<string, string> = {
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
  ts: "typescript", tsx: "typescript",
  json: "json",
  css: "css",
  html: "xml", xml: "xml", svelte: "xml",
  rs: "rust",
  py: "python",
  sh: "bash", bash: "bash",
  yml: "yaml", yaml: "yaml",
  toml: "ini", ini: "ini", cfg: "ini",
};

/** The highlight.js language for an extension, or null if we don't highlight it. */
export function languageForExt(ext: string): string | null {
  return LANG_BY_EXT[ext] ?? null;
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

/**
 * Highlight `code` for the given file extension, returning HTML safe for
 * `{@html}`. Falls back to escaped plain text when the extension has no mapped
 * language, so callers can always inject the result.
 */
export function highlightCode(code: string, ext: string): string {
  const lang = languageForExt(ext);
  if (lang && hljs.getLanguage(lang)) {
    return hljs.highlight(code, { language: lang }).value;
  }
  return escapeHtml(code);
}
