// CPE-1936: fenced-code-block extraction for the runbooks, lifted out of `runbookJqQuoting.test.ts`
// (CPE-1918) so a second guard reading `.claude/commands/run.md` gets the already-reviewed parser
// instead of a second hand-rolled one that could disagree with the first on an edge case — the same
// reason `shellScriptLines.ts` was extracted at CPE-1849, and the shape CPE-1950 asks for when a copy
// is about to be made.
//
// Why a guard needs it at all: CLAUDE.md's CPE-1933 rule is "anchor on code, never on prose". A
// runbook is mostly prose that QUOTES the very literals a guard wants to check, so a guard that greps
// the whole `.md` will happily match the commentary instead of the command. Pulling the fenced blocks
// out first — and then looking only inside the block whose info string names the right shell — is what
// makes that anchoring real.

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
