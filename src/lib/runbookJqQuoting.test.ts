// CPE-1918: guard for the runbooks' copy-pasteable `gh --jq` snippets.
//
// `RELEASING.md` and `.claude/commands/run.md` tell a human (or an agent) to paste a `gh ... --jq ...`
// command into Windows PowerShell. PS 5.1 strips `"` when it marshals an argument into a native exe's
// argv, so a `--jq` filter carrying a string literal reaches jq unquoted and dies inside jq's parser
// instead of producing the check's crafted "do not publish" message. Measured on PS 5.1 with
// `node -e "console.log(JSON.stringify(process.argv.slice(1)))"`:
//
//   --jq '.jobs[] | select(.name=="x")'      -> jq gets  .jobs[] | select(.name==x)      (broken)
//   --jq ".jobs[] | select(.name==\"x\")"    -> jq gets  .jobs[] | select(.name==" x\)   (broken, differently)
//   --jq ".jobs[] | select(.name==`"x`")"    -> jq gets  .jobs[] | select(.name==x)      (broken)
//   --jq '.jobs'                             -> jq gets  .jobs                           (fine)
//
// The middle form was the one this repo believed was correct — it sat two lines away from the bug and
// was cited as the fix — which is exactly why this needs a test rather than a convention. The rule the
// snippets now follow, and that this file pins: **a `--jq` / `-q` argument inside a PowerShell-fenced
// block must contain no `"` at all.** Use `--jq` only to pluck the sub-tree and do string matching in
// PowerShell (`ConvertFrom-Json` + `Where-Object { $_.field -ceq 'literal' }`).
//
// The same `--jq '…"x"…'` form is CORRECT in `.github/workflows/**`, whose steps run under bash on
// Linux runners. The shell a snippet targets is the whole difference, so the second test asserts every
// fenced block containing a `gh` command line is language-tagged.
import { describe, it, expect } from "vitest";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = process.cwd();

/** Runbooks whose fenced snippets are meant to be run verbatim by a human or an agent. */
function runbookFiles(): string[] {
  const files: string[] = [];
  for (const f of ["RELEASING.md", "CLAUDE.md", "README.md"]) {
    const p = join(ROOT, f);
    if (existsSync(p)) files.push(p);
  }
  for (const dir of [join(ROOT, ".claude", "commands"), join(ROOT, "docs")]) {
    if (existsSync(dir)) walk(dir, files);
  }
  return files;
}

function walk(dir: string, out: string[]): void {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (name.endsWith(".md")) out.push(p);
  }
}

type Block = { lang: string; lines: string[]; startLine: number };

/** Every ``` fenced block in `md`, with its language tag ("" when untagged) and 1-based start line. */
export function fencedBlocks(md: string): Block[] {
  const lines = md.replace(/^﻿/, "").split(/\r?\n/);
  const blocks: Block[] = [];
  let open: Block | null = null;
  for (let i = 0; i < lines.length; i++) {
    const fence = /^```(\S*)\s*$/.exec(lines[i]);
    if (!fence) {
      if (open) open.lines.push(lines[i]);
      continue;
    }
    if (open) {
      blocks.push(open);
      open = null;
    } else {
      open = { lang: fence[1].toLowerCase(), lines: [], startLine: i + 2 };
    }
  }
  if (open) blocks.push(open);
  return blocks;
}

const POWERSHELL = new Set(["powershell", "pwsh", "ps1", "ps"]);

/** A `gh` invocation, at the start of a line or after a pipe/`=`/`(` — enough to spot a shell snippet. */
const GH_COMMAND = /(^|[|(=]\s*|^\s+)gh\s+(api|run|release|pr|issue|workflow|auth|repo|search)\b/m;

/**
 * Offending `--jq` / `-q` arguments on one line: the flag, then the rest of the line, when that rest
 * contains a `"`. Deliberately greedy-to-end-of-line — a jq filter is one argument and PowerShell's
 * backtick line-continuation means the rest of the physical line IS the argument.
 */
export function badJqArgs(line: string): string[] {
  const hits: string[] = [];
  const re = /(?:^|\s)(--jq|-q)\s+(\S.*)$/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(line)) !== null) {
    const tail = m[2].replace(/\s*`$/, ""); // drop a trailing PowerShell line-continuation backtick
    if (tail.includes('"')) hits.push(`${m[1]} ${tail}`);
  }
  return hits;
}

describe("runbook `gh --jq` snippets are PowerShell-safe (CPE-1918)", () => {
  const files = runbookFiles();

  it("finds the runbooks it is supposed to be guarding", () => {
    expect(files.map((f) => relative(ROOT, f))).toContain("RELEASING.md");
    expect(files.map((f) => relative(ROOT, f).replace(/\\/g, "/"))).toContain(".claude/commands/run.md");
  });

  it("no PowerShell-fenced `--jq` / `-q` argument contains a double quote", () => {
    const offences: string[] = [];
    for (const file of files) {
      const md = readFileSync(file, "utf8");
      for (const block of fencedBlocks(md)) {
        if (!POWERSHELL.has(block.lang)) continue;
        block.lines.forEach((line, i) => {
          if (line.trimStart().startsWith("#")) return; // a comment may quote the broken form to explain it
          for (const bad of badJqArgs(line)) {
            offences.push(`${relative(ROOT, file)}:${block.startLine + i}  ${bad.trim()}`);
          }
        });
      }
    }
    expect(offences).toEqual([]);
  });

  it("every fenced block containing a `gh` command names the shell it targets", () => {
    const untagged: string[] = [];
    for (const file of files) {
      const md = readFileSync(file, "utf8");
      for (const block of fencedBlocks(md)) {
        if (block.lang) continue;
        if (GH_COMMAND.test(block.lines.join("\n"))) {
          untagged.push(`${relative(ROOT, file)}:${block.startLine}`);
        }
      }
    }
    expect(untagged).toEqual([]);
  });
});

describe("badJqArgs recognises the shapes measured on PowerShell 5.1", () => {
  it("flags the single-quoted filter with an embedded string literal", () => {
    expect(badJqArgs(`  --jq '.jobs[] | select(.name=="verify-published-manifest-sidecar")'`)).toHaveLength(1);
  });

  it("flags the backslash-escaped form that was wrongly believed to be the fix", () => {
    expect(badJqArgs(`  --jq ".jobs[] | select(.name==\\"$jobName\\")"`)).toHaveLength(1);
  });

  it("flags `-q` as well as `--jq`", () => {
    expect(badJqArgs(`gh api repos/o/r/releases -q '.[] | select(.tag_name=="v1")'`)).toHaveLength(1);
  });

  it("allows a quote-free filter, including one ending in a continuation backtick", () => {
    expect(badJqArgs(`  --jq '.jobs'`)).toEqual([]);
    expect(badJqArgs(`gh release view v1 --json assets --jq '.assets[].name'`)).toEqual([]);
    expect(badJqArgs(`  --limit 1 --json databaseId --jq '.[0].databaseId' \``)).toEqual([]);
  });

  it("ignores lines with no jq flag at all", () => {
    expect(badJqArgs(`$job = $jobs | Where-Object { $_.name -ceq "verify-published-manifest" }`)).toEqual([]);
  });
});
