/**
 * Syntax highlighting for code previews. Starts from the highlight.js "common"
 * bundle (~36 popular languages) and registers additional grammars for the many
 * languages we preview. Output is HTML with hljs-* token spans; highlight.js
 * HTML-escapes the source, so the result is safe to inject with {@html}.
 */
import hljs from "highlight.js/lib/common";
import scala from "highlight.js/lib/languages/scala";
import julia from "highlight.js/lib/languages/julia";
import dart from "highlight.js/lib/languages/dart";
import haskell from "highlight.js/lib/languages/haskell";
import elixir from "highlight.js/lib/languages/elixir";
import erlang from "highlight.js/lib/languages/erlang";
import clojure from "highlight.js/lib/languages/clojure";
import fsharp from "highlight.js/lib/languages/fsharp";
import ocaml from "highlight.js/lib/languages/ocaml";
import groovy from "highlight.js/lib/languages/groovy";
import fortran from "highlight.js/lib/languages/fortran";
import delphi from "highlight.js/lib/languages/delphi";
import ada from "highlight.js/lib/languages/ada";
import x86asm from "highlight.js/lib/languages/x86asm";
import nim from "highlight.js/lib/languages/nim";
import crystal from "highlight.js/lib/languages/crystal";
import dlang from "highlight.js/lib/languages/d";
import scheme from "highlight.js/lib/languages/scheme";
import lisp from "highlight.js/lib/languages/lisp";
import tcl from "highlight.js/lib/languages/tcl";
import verilog from "highlight.js/lib/languages/verilog";
import vhdl from "highlight.js/lib/languages/vhdl";
import powershell from "highlight.js/lib/languages/powershell";
import awk from "highlight.js/lib/languages/awk";
import cmake from "highlight.js/lib/languages/cmake";
import dockerfile from "highlight.js/lib/languages/dockerfile";
import protobuf from "highlight.js/lib/languages/protobuf";
import llvm from "highlight.js/lib/languages/llvm";
import prolog from "highlight.js/lib/languages/prolog";
import elm from "highlight.js/lib/languages/elm";
import reasonml from "highlight.js/lib/languages/reasonml";
import haxe from "highlight.js/lib/languages/haxe";
import nix from "highlight.js/lib/languages/nix";
import stylus from "highlight.js/lib/languages/stylus";
import handlebars from "highlight.js/lib/languages/handlebars";
import twig from "highlight.js/lib/languages/twig";
import nginx from "highlight.js/lib/languages/nginx";

const EXTRA: Record<string, unknown> = {
  scala, julia, dart, haskell, elixir, erlang, clojure, fsharp, ocaml, groovy,
  fortran, delphi, ada, x86asm, nim, crystal, d: dlang, scheme, lisp, tcl,
  verilog, vhdl, powershell, awk, cmake, dockerfile, protobuf, llvm, prolog,
  elm, reasonml, haxe, nix, stylus, handlebars, twig, nginx,
};
for (const [name, lang] of Object.entries(EXTRA)) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  hljs.registerLanguage(name, lang as any);
}

const LANG_BY_EXT: Record<string, string> = {
  // JS/TS/web
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
  ts: "typescript", tsx: "typescript",
  json: "json", json5: "json", jsonc: "json",
  css: "css", scss: "scss", less: "less", styl: "stylus",
  html: "xml", xml: "xml", svelte: "xml", xsl: "xml", xslt: "xml",
  hbs: "handlebars", twig: "twig",
  // systems / general
  rs: "rust", ron: "rust", go: "go", c: "c",
  h: "cpp", hpp: "cpp", hh: "cpp", cpp: "cpp", cc: "cpp", cxx: "cpp",
  cs: "csharp", m: "objectivec", mm: "objectivec",
  java: "java", kt: "kotlin", kts: "kotlin", swift: "swift",
  vb: "vbnet", d: "d", nim: "nim", cr: "crystal",
  scala: "scala", dart: "dart", hx: "haxe", nix: "nix",
  // functional
  hs: "haskell", ex: "elixir", exs: "elixir", erl: "erlang",
  clj: "clojure", cljs: "clojure", edn: "clojure",
  fs: "fsharp", fsx: "fsharp", ml: "ocaml", mli: "ocaml",
  elm: "elm", re: "reasonml", rei: "reasonml",
  rkt: "scheme", scm: "scheme", ss: "scheme", lisp: "lisp", lsp: "lisp",
  jl: "julia",
  // scripting
  py: "python", rb: "ruby", php: "php", pl: "perl", pm: "perl",
  lua: "lua", r: "r", sh: "bash", bash: "bash", tcl: "tcl", awk: "awk",
  groovy: "groovy", gradle: "groovy",
  ps1: "powershell", psm1: "powershell", psd1: "powershell",
  // hardware / low-level
  v: "verilog", sv: "verilog", vhd: "vhdl", vhdl: "vhdl",
  asm: "x86asm", s: "x86asm", ll: "llvm", wat: "wasm",
  // classic
  f90: "fortran", f95: "fortran", pas: "delphi", adb: "ada", ads: "ada",
  prolog: "prolog", pro: "prolog",
  // data / config / build / misc
  yml: "yaml", yaml: "yaml", toml: "ini", ini: "ini", cfg: "ini",
  editorconfig: "ini", sql: "sql", graphql: "graphql", gql: "graphql",
  proto: "protobuf", cmake: "cmake", nginx: "nginx",
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
