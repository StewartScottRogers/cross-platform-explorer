---
id: CPE-1834
title: release.ps1 bumps three version files through a bare Set-Content, the exact call that corrupts files here
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

`scripts/release.ps1` lines 23, 29 and 35 write `package.json`, `src-tauri/tauri.conf.json` and
`src-tauri/Cargo.toml` through a bare `Set-Content` with **no `-Encoding` argument** during a version
bump.

That is the precise call this repo has been burned by. With no `-Encoding`, PowerShell 5.1 writes the
system ANSI code page (CP1252 here), which is lossy for any non-ASCII character — measured this same
day: an em dash `E2 80 94` collapses to a single `0x97`, producing a file that is no longer valid
UTF-8. A PowerShell write has BOM'd or re-encoded a repo file before and **once blocked a release**.

It is safe **today** only because those three files happen to contain nothing but ASCII. Nothing
enforces that. A product name, an author field, a description, or a dependency string with a curly
quote, an accented letter or an em dash would be silently mangled — by the release script, on the
release path, at the moment nobody is watching.

## Why it matters

This is the one script whose job is to produce a shippable artifact, and it is the least-exercised
code in the repo. A corrupted `package.json` or `Cargo.toml` fails the build loudly if you are lucky
and produces a subtly wrong manifest if you are not. Two of the three files are also part of the
five-files-in-sync version dance, which already gets missed regularly.

## Acceptance criteria

- [ ] All three writes pass an explicit encoding that is UTF-8 **without** a BOM. Note the trap:
      `Set-Content -Encoding utf8` on PowerShell 5.1 writes a BOM, which is its own problem — the
      repo's mojibake guard has a `bom` check for exactly that. Verify what the chosen call actually
      produces byte-for-byte rather than trusting the flag name.
- [ ] Every other write in `scripts/*.ps1` is audited for the same shape and fixed or explicitly
      justified. Do not fix one line and leave siblings.
- [ ] Red-proof it honestly: put a non-ASCII character into a scratch copy of each file, run the bump,
      and compare bytes before and after. Report the byte sequences.
- [ ] The mojibake guard (`src/lib/mojibakeGuard.ts`, widened by CPE-1788 to catch UTF-16 and ANSI
      rewrites as well as the CP1252 shape) is confirmed to catch the corrupted output if the fix were
      removed — that is the durable regression net.
- [ ] Consider whether the version bump belongs in PowerShell at all. `scripts/release.ps1` is the only
      thing forcing this hazard; a small node or python step would not have it. Record the decision
      either way.

## Notes

Found by the independent Reviewer during the CPE-1788 review, while verifying that nothing in this
repo's own tooling writes UTF-16 without a BOM. It checked `scripts/*.ps1` and `.github/workflows/*.yml`
for that, and found this instead.

Related: CPE-1788 (guard widened to catch UTF-16/ANSI rewrites), CPE-1783 and CPE-1752 (mojibake
already repaired in `dispatch.rs`), CPE-1784 (the `Ticketing/` mojibake and BOM corpus, still open).

## Work Log

**2026-08-21** — Fixed all three writes in `scripts/release.ps1` (`package.json`,
`src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`) by replacing bare `Get-Content -Raw` /
`Set-Content -NoNewline` with `[System.IO.File]::ReadAllText`/`WriteAllText`, both given an explicit
`New-Object System.Text.UTF8Encoding($false)` (UTF-8, BOM suppressed). Verified byte-for-byte on this
machine (PS 5.1, `$PSVersionTable.PSVersion` 5.1.26100.9168) with the harness's
`$PSDefaultParameterValues['Out-File:Encoding']` override cleared first:

- Bare `Set-Content` (no `-Encoding`) on a string containing U+2014 (em dash) + U+20AC (euro) wrote
  `... 6D 6F 97 74 ... 65 20 80 35` — the em dash's 3 UTF-8 bytes (`E2 80 94`) collapsed to one CP1252
  byte `0x97`, and the euro's `E2 82 AC` collapsed to `0x80`. Matches the ticket's cited measurement
  exactly.
- `Set-Content -Encoding utf8` (the AC's named trap) wrote a leading `EF BB BF` BOM in front of otherwise
  correct UTF-8 bytes — confirmed the guard's `bom` check exists for exactly this shape and that the flag
  name does not mean "no BOM."
- `[System.IO.File]::WriteAllText` with `UTF8Encoding($false)` wrote correct UTF-8 (`E2 80 94` /
  `E2 82 AC`) with no BOM — the chosen fix.

Extended the fix to the **read** side too, beyond the ticket's literal `Set-Content` framing. Red-proofing
the full pipeline (read real UTF-8 file bytes → regex-replace version → write) surfaced that bare
`Get-Content -Raw` (no `-Encoding`) *also* misdecodes a BOM-less UTF-8 file as CP1252 on this machine
(`Encoding.Default` resolves to CP1252/1252 here even though the console itself runs UTF-8 — confirmed via
`[System.Text.Encoding]::Default` vs `[Console]::OutputEncoding` diverging). Fixing only `Set-Content` and
leaving `Get-Content` bare would have been a regression for the current, accidentally-safe case: bare
Get-Content + bare Set-Content happen to round-trip existing non-ASCII bytes losslessly on this box (both
sides share the same lossy codec, so misdecode-then-re-encode is an identity transform) — measured this
directly with a scratch package.json/tauri.conf.json/Cargo.toml carrying an injected em dash, which
survived the full old pipeline intact. But patching *only* the write side would have broken that
coincidence and turned survivable content into guaranteed mojibake (`price — €5` → read via bare
Get-Content, written via the new WriteAllText → `70 72 69 63 65 20 C3 A2 E2 82 AC E2 80 9D ...`, double-
encoded and worse than the original bug). That accidental round-trip safety isn't something to design
around anyway — it depends on the OS ANSI code page and PowerShell 5.1's specific defaults (PowerShell 7
defaults differently), so both `Get-Content` and `Set-Content` now go through the same explicit
`ReadAllText`/`WriteAllText` + `UTF8Encoding($false)` pair, which is correct regardless of machine locale
or PowerShell version.

Audited every other write in `scripts/*.ps1`: `scripts/new-sample-sandbox.ps1` (the only other script
in that glob) has no `Set-Content`/`Out-File`/`Add-Content`/`[System.IO]` text write at all — it only uses
`New-Item -ItemType Directory`, `Copy-Item`, and `Remove-Item` (binary-safe, not text re-encoding). No
change needed there; confirmed via `grep -n "Set-Content\|Out-File\|Add-Content\|WriteAllText\|System.IO"`
across both files in `scripts/`.

Confirmed the mojibake guard catches the corruption if this fix were reverted: added a scratch vitest
file exercising `mojibakeGuard.ts`'s exported functions directly (deleted before commit, not part of the
diff) — `simulatePowerShellAnsiRewrite` reproduces the exact bytes measured above byte-for-byte;
`findFirstInvalidUtf8Byte` flags the old bare-`Set-Content` output and does not flag the new
`WriteAllText` output; `findMojibake` flags the read-side-only corruption case described above. All 62
existing tests in `src/lib/mojibakeGuard.test.ts` still pass against the real, now-fixed `release.ps1`
(the tree-wide scan itself asserts no repo file currently carries the signature).

**Decision — keep the version bump in PowerShell, do not move it to node/python.** The ticket's own AC
treats this as "consider and record," not "migrate." The fix above already closes the encoding hazard
completely and portably (explicit BOM-less UTF-8 on both read and write, independent of system locale or
PowerShell 5.1 vs 7 defaults), so the main safety argument for a node/python rewrite — "a different
runtime's defaults wouldn't have this trap" — is now moot; the remaining argument would be cross-platform
portability (a node script would also run release logic on macOS/Linux), which is a real but separate,
larger-scoped concern (this script also drives `git add`/`commit`/`tag`/`push` with Windows-specific
stderr-vs-exit-code handling) that doesn't belong in an "S" bug-fix ticket about one encoding defect.
Splitting the version-bump text logic into a second language/runtime here would add a maintenance seam
(two places implementing the same three regex substitutions, a cross-process handoff) for no longer any
safety benefit. Not filing a follow-up ticket for a language migration — no concrete cross-platform
release requirement exists yet to motivate it.

Gates: `npx vitest run src/lib/mojibakeGuard.test.ts` — 62/62 passed. Full `npx vitest run` — 320 files /
4258 tests passed. `npm run check` — 0 errors, 0 warnings. No Rust touched, so no clippy/cargo gate.

Everything corrupted during red-proofing was written only to a scratch directory
(`.scratch-cpe1834/`, outside git tracking) inside this worktree and removed before finishing; `git status`
shows only `scripts/release.ps1` modified, `git diff --numstat` shows a 19/6-line targeted diff (not a
whole-file rewrite).
