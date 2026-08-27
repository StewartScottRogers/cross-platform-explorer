// CPE-1918: guard for the runbooks' copy-pasteable `gh --jq` snippets.
//
// `RELEASING.md` and `.claude/commands/run.md` tell a human (or an agent) to paste a `gh ... --jq ...`
// command into Windows PowerShell before publishing a release. PS 5.1 strips `"` when it marshals an
// argument into a native exe's argv, so a `--jq` filter carrying a string literal reaches jq unquoted
// and dies inside jq's parser instead of producing the check's crafted "do not publish" message.
// Measured on PS 5.1.26100.9168 with `node -e "console.log(JSON.stringify(process.argv.slice(1)))"`:
//
//   --jq '.jobs[] | select(.name=="x")'      -> jq gets  .jobs[] | select(.name==x)      (broken)
//   --jq ".jobs[] | select(.name==\"x\")"    -> jq gets  .jobs[] | select(.name==" x\)   (broken, differently)
//   --jq ".jobs[] | select(.name==`"x`")"    -> jq gets  .jobs[] | select(.name==x)      (broken)
//   --jq='.jobs[] | select(.name=="x")'      -> jq gets  .jobs[] | select(.name==x)      (broken)
//   --jq '.jobs'                             -> jq gets  .jobs                           (fine)
//
// The second form was the one this repo believed was correct — it sat two lines away from the bug and
// the ticket cited it as the fix — which is why this needs a test rather than a convention.
//
// Two further shapes DO survive: `'…\"x\"…'` and `'…""x""…'` (single-quoted, backslash- or
// doubled-escaped inner). The rule enforced here is deliberately stricter than strictly necessary —
// **no `"` anywhere in a `--jq` / `-q` argument inside a PowerShell block** — because the difference
// between the four broken shapes and those two is one character in a place no reader can check by eye,
// and this exact class has already regressed once. Use `--jq` only to pluck the sub-tree and match in
// PowerShell (`ConvertFrom-Json` + `Where-Object { $_.field -ceq 'literal' }`). RELEASING.md's
// "PowerShell and `gh --jq`" section says the same thing, including that it is deliberately strict.
//
// The `--jq '…"x"…'` form is CORRECT in `.github/workflows/**`, whose steps run under bash on Linux
// runners. The shell a snippet targets is the whole difference, so the second test asserts every
// fenced block containing a `gh` command line names a specific shell — not merely *some* info string,
// because a PowerShell snippet mislabelled ```console is skipped by the first test AND satisfies a
// naive "has a tag" check, making it doubly invisible.
//
// KNOWN GAPS, measured and deliberately left open (CPE-1918 review round 2). Each is a shape this
// guard does NOT catch. They are recorded here rather than fixed because none exists in the tree
// today and each fix risks more than it buys; if one ever appears, fix it and delete its line.
//
//   1. Blockquoted fences evade: `> ```powershell` is not seen at all. Not hypothetical — RELEASING.md
//      already uses blockquote callouts, including the `$jobs` note right beside the fixed snippet.
//      Stripping a `> ` prefix per line is easy; doing it without breaking a fence that interleaves
//      with a quote boundary is not, so it is left.
//   2. `--jq $q`, where the filter was assigned to a variable on an earlier line, is a miss: the
//      quotes live on the assignment, not the flag. This is row 4 of the measured table above — the
//      doc names a broken shape this guard cannot pin. The doc is the control for that one.
//   3. `Select-String … -q "z"` would be a FALSE red: PowerShell accepts `-q` as a unique prefix of
//      `-Quiet`. None in the tree today; if one appears, exempt `Select-String` rather than loosening
//      the flag matcher.
//   4. Tilde fences (`~~~powershell`) were a gap and are now handled — a fence opens on three or more
//      backticks OR tildes and closes only on the SAME character, so a tilde block is not closed by a
//      backtick line.
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

export type Block = { lang: string; lines: string[]; startLine: number };

/**
 * Every fenced block in `md`, with its info string ("" when untagged) and the 1-based line number of
 * its first content line.
 *
 * Handles **indented** fences (a fence inside a list item — `run.md` is a numbered-step document, and
 * re-parsing the real files found SEVEN such blocks the column-0-only version had never seen), fences
 * of **more than three** markers (a closing fence must be at least as long as its opener, which is how
 * a block can contain a ``` line of its own), and **tilde** fences. A block closes only on the same
 * marker character it opened with, so a `~~~` block is not terminated by a ``` line.
 */
export function fencedBlocks(md: string): Block[] {
  const lines = md.replace(/^﻿/, "").split(/\r?\n/);
  const blocks: Block[] = [];
  let open: (Block & { ticks: number; char: string }) | null = null;
  for (let i = 0; i < lines.length; i++) {
    const fence = /^\s*(([`~])\2{2,})\s*(\S*)\s*$/.exec(lines[i]);
    if (open) {
      // A closing fence is bare and at least as long as the opener; anything else is content.
      if (fence && fence[3] === "" && fence[2] === open.char && fence[1].length >= open.ticks) {
        const { ticks: _ticks, char: _char, ...block } = open;
        blocks.push(block);
        open = null;
      } else {
        open.lines.push(lines[i]);
      }
      continue;
    }
    if (fence) {
      open = {
        lang: fence[3].toLowerCase(),
        lines: [],
        startLine: i + 2,
        ticks: fence[1].length,
        char: fence[2],
      };
    }
  }
  if (open) {
    const { ticks: _ticks, char: _char, ...block } = open;
    blocks.push(block);
  }
  return blocks;
}

/** Info strings that mean "Windows PowerShell". */
export const POWERSHELL_LANGS = new Set(["powershell", "pwsh", "ps1", "ps"]);
/** Info strings that name a POSIX shell. */
export const POSIX_SHELL_LANGS = new Set(["bash", "sh", "zsh"]);
/** The tags that actually name a shell. `console`, `text`, `shell` and "" do not. */
const SHELL_LANGS = new Set([...POWERSHELL_LANGS, ...POSIX_SHELL_LANGS]);

/** A `gh` invocation, at the start of a line or after a pipe/`=`/`(` — enough to spot a shell snippet. */
const GH_COMMAND = /(^|[|(=]\s*|^\s+)gh\s+(api|run|release|pr|issue|workflow|auth|repo|search)\b/m;

/**
 * Physical lines folded into logical ones: PowerShell continues a command with a trailing backtick,
 * so `--jq` and its filter can sit on **different** physical lines — the most realistic reintroduction
 * shape, since the shipped snippets already break immediately *before* `--jq`. Full-line `#` comments
 * are dropped first: they may legitimately quote the broken form to explain it, and one of them ends
 * in a backtick that would otherwise swallow the following line.
 *
 * Each entry is `[firstPhysicalLineIndex, joinedText]`, 0-based within the block.
 */
export function logicalLines(lines: string[]): Array<[number, string]> {
  const out: Array<[number, string]> = [];
  let start = -1;
  let buf = "";
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].trimStart().startsWith("#")) continue;
    if (start === -1) start = i;
    const cont = /^(.*?)\s*`$/.exec(lines[i]);
    if (cont) {
      buf += cont[1] + " ";
      continue;
    }
    out.push([start, buf + lines[i]]);
    start = -1;
    buf = "";
  }
  if (start !== -1) out.push([start, buf.trimEnd()]);
  return out;
}

/**
 * The value of a `--jq` / `-q` flag: the quoted token when the argument opens with a quote, otherwise
 * the rest of the line. Truncating at the first closing quote keeps a clean `--jq '.jobs' | ConvertFrom-Json`
 * from being flagged for the quotes in a later, unrelated part of the same line.
 */
function jqArgument(tail: string): string {
  const q = tail[0];
  if (q === "'" || q === '"') {
    const end = tail.indexOf(q, 1);
    return end === -1 ? tail : tail.slice(0, end + 1);
  }
  return tail;
}

/**
 * Offending `--jq` / `-q` arguments on one logical line — those containing a `"`.
 *
 * Accepts every spelling `gh`'s flag parser does: `--jq X`, `--jq=X`, `-q X`, `-q=X` and `-qX`. The
 * lookahead in the separator group is what stops `-quiet` from being read as `-q` + `uiet`.
 */
export function badJqArgs(line: string): string[] {
  const hits: string[] = [];
  const re = /(?:^|\s)(--jq|-q)(?:=|\s+|(?=['"]))(\S.*)$/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(line)) !== null) {
    const arg = jqArgument(m[2].replace(/\s*`$/, ""));
    if (arg.includes('"')) hits.push(`${m[1]} ${arg}`);
  }
  return hits;
}

describe("runbook `gh --jq` snippets are PowerShell-safe (CPE-1918)", () => {
  const files = runbookFiles();

  it("finds the runbooks it is supposed to be guarding", () => {
    const rel = files.map((f) => relative(ROOT, f).replace(/\\/g, "/"));
    expect(rel).toContain("RELEASING.md");
    expect(rel).toContain(".claude/commands/run.md");
  });

  it("no PowerShell-fenced `--jq` / `-q` argument contains a double quote", () => {
    const offences: string[] = [];
    for (const file of files) {
      for (const block of fencedBlocks(readFileSync(file, "utf8"))) {
        if (!POWERSHELL_LANGS.has(block.lang)) continue;
        for (const [i, line] of logicalLines(block.lines)) {
          for (const bad of badJqArgs(line)) {
            offences.push(`${relative(ROOT, file)}:${block.startLine + i}  ${bad.trim()}`);
          }
        }
      }
    }
    expect(offences).toEqual([]);
  });

  it("every fenced block containing a `gh` command names a specific shell", () => {
    const unnamed: string[] = [];
    for (const file of files) {
      for (const block of fencedBlocks(readFileSync(file, "utf8"))) {
        if (SHELL_LANGS.has(block.lang)) continue;
        if (GH_COMMAND.test(block.lines.join("\n"))) {
          unnamed.push(`${relative(ROOT, file)}:${block.startLine}  lang=${block.lang || "(none)"}`);
        }
      }
    }
    expect(unnamed).toEqual([]);
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

  it("allows a quote-free filter, including one followed by a pipeline", () => {
    expect(badJqArgs(`  --jq '.jobs'`)).toEqual([]);
    expect(badJqArgs(`gh release view v1 --json assets --jq '.assets[].name'`)).toEqual([]);
    expect(badJqArgs(`$jobs = gh run view 1 --json jobs --jq '.jobs' | ConvertFrom-Json`)).toEqual([]);
  });

  it("ignores lines with no jq flag at all", () => {
    expect(badJqArgs(`$job = $jobs | Where-Object { $_.name -ceq "verify-published-manifest" }`)).toEqual([]);
  });

  it("does not read `-quiet` as the `-q` flag", () => {
    expect(badJqArgs(`hdiutil detach -quiet "/Volumes/Cross-Platform Explorer"`)).toEqual([]);
  });
});

// Every fixture below is an evasion this guard shipped with and no longer has. A guard's known
// evasions belong in its own test table, not in a review transcript.
describe("evasions this guard used to have (CPE-1918 review)", () => {
  it("F1: a filter on the PowerShell continuation line is still caught", () => {
    const block = [
      "$jobs = gh run view $runId --repo o/r --json jobs --jq `",
      `  '.jobs[] | select(.name=="verify-published-manifest-sidecar")' | ConvertFrom-Json`,
    ];
    const joined = logicalLines(block);
    expect(joined).toHaveLength(1);
    expect(joined[0][0]).toBe(0); // reported against the line the command starts on
    expect(badJqArgs(joined[0][1])).toHaveLength(1);
  });

  it("F2: the `--jq=FILTER` equals form is caught, as are `-q=` and `-qX`", () => {
    expect(badJqArgs(`gh run view 1 --json jobs --jq='.jobs[] | select(.name=="x")'`)).toHaveLength(1);
    expect(badJqArgs(`gh run view 1 --json jobs -q='.jobs[] | select(.name=="x")'`)).toHaveLength(1);
    expect(badJqArgs(`gh run view 1 --json jobs -q'.jobs[] | select(.name=="x")'`)).toHaveLength(1);
  });

  it("F3: a PowerShell snippet mislabelled ```console does not count as naming a shell", () => {
    for (const lang of ["console", "text", "txt", "shell", ""]) {
      expect(SHELL_LANGS.has(lang)).toBe(false);
    }
    for (const lang of ["powershell", "pwsh", "ps1", "ps", "bash", "sh", "zsh"]) {
      expect(SHELL_LANGS.has(lang)).toBe(true);
    }
  });

  it("F4a: an indented fence inside a numbered list item is still scanned", () => {
    const md = [
      "1. step",
      "",
      "   ```powershell",
      `   gh run view 1 --json jobs --jq '.jobs[] | select(.name=="x")'`,
      "   ```",
      "",
    ].join("\n");
    const blocks = fencedBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].lang).toBe("powershell");
    expect(badJqArgs(logicalLines(blocks[0].lines)[0][1])).toHaveLength(1);
  });

  it("F4b: a four-backtick fence parses its language, and may contain a ``` line", () => {
    const md = ["````powershell", "```", `gh run view 1 --json jobs --jq '.jobs[] | select(.name=="x")'`, "````", ""].join(
      "\n",
    );
    const blocks = fencedBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].lang).toBe("powershell");
    expect(blocks[0].lines).toHaveLength(2);
  });

  it("F4c: a tilde fence is scanned, and is not closed by a backtick line", () => {
    const md = ["~~~powershell", "```", `gh run view 1 --json jobs --jq '.jobs[] | select(.name=="x")'`, "~~~", ""].join(
      "\n",
    );
    const blocks = fencedBlocks(md);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].lang).toBe("powershell");
    expect(blocks[0].lines).toHaveLength(2);
    // The inner "```" ends in a backtick, so the joiner folds it into the next line — one logical
    // line, and the bad filter is still caught.
    const joined = logicalLines(blocks[0].lines);
    expect(joined).toHaveLength(1);
    expect(badJqArgs(joined[0][1])).toHaveLength(1);
  });

  it("a two-marker line is not a fence", () => {
    expect(fencedBlocks(["``", "not a fence", "``", ""].join("\n"))).toEqual([]);
  });

  it("comment lines may still quote the broken form while explaining it", () => {
    const block = [
      "# a filter like `select(.name==\"x\")` reaches jq unquoted, so do the match in PowerShell:",
      "$jobs = gh run view 1 --json jobs --jq '.jobs' | ConvertFrom-Json",
    ];
    expect(logicalLines(block).flatMap(([, l]) => badJqArgs(l))).toEqual([]);
  });
});
