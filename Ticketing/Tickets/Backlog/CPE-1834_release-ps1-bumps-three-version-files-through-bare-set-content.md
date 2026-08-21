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
