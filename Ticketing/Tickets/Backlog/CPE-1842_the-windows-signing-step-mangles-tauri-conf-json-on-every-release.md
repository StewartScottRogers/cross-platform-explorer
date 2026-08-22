---
id: CPE-1842
title: the Windows signing step misdecodes and BOMs tauri.conf.json on every real release build
type: bug
priority: High
status: Backlog
tags: ready
estimate: S
created: 2026-08-21
closed:
---

## Problem

`.github/workflows/release.yml:97` and `.github/workflows/release-sidecar.yml:465` both do, under
`shell: pwsh`, on the Windows code-signing step:

```powershell
$j = Get-Content $conf -Raw | ConvertFrom-Json
...
($j | ConvertTo-Json -Depth 40) | Set-Content $conf -Encoding utf8
```

against `src-tauri/tauri.conf.json`. That is the identical hazard CPE-1834 just fixed in
`scripts/release.ps1`, in both halves:

- **Bare `Get-Content -Raw`** misdecodes BOM-less UTF-8 as the system ANSI code page.
- **`Set-Content -Encoding utf8`** on PowerShell writes a **BOM** — the trap CPE-1834's ticket names
  explicitly, and which the repo's own mojibake guard has a dedicated `bom` check for.

**And unlike CPE-1834's case, this one is live.** `src-tauri/tauri.conf.json:39` currently contains a
real non-ASCII em dash:

```
"description": "Cross-Platform Explorer — window geometry..."
```

So this runs, on that character, on **every real Windows release build**, today.

## Why it matters

This is the code-signing step of the release pipeline — the least-watched code in the repo, executing
against a file that is one of the five that must stay version-synchronised. A mangled or BOM-prefixed
`tauri.conf.json` either fails the build confusingly or ships a subtly wrong manifest.

Note the round-trip coincidence that saves `scripts/release.ps1` does **not** apply here: that one read
and wrote with the same lossy codec, so the bytes survived by accident. This step reads with the lossy
codec and writes with an explicit UTF-8 encoder, which is precisely the write-only-fix shape CPE-1834
measured as producing **double-encoded garbage** (`price — €5` → 21 bytes of mojibake for 12 bytes of
input).

## Acceptance criteria

- [ ] Both sites read and write with an explicit BOM-less UTF-8 encoding. CPE-1834's landed fix is the
      precedent — `[System.IO.File]::ReadAllText` / `WriteAllText` with
      `New-Object System.Text.UTF8Encoding($false)` — verified byte-for-byte rather than trusted from a
      flag name.
- [ ] Verify what actually ships today before fixing, so the severity is recorded rather than assumed:
      run the current step against a copy of the real `tauri.conf.json` and report the bytes around that
      em dash, and whether a BOM appears. If the released manifest has been mangled or BOM'd, say so.
- [ ] `ConvertTo-Json -Depth 40` reformats the whole file. Check what the resulting diff looks like and
      whether key order, indentation or escaping change — a signing step that rewrites the manifest
      wholesale is its own problem, separate from the encoding.
- [ ] The mojibake guard catches the old output if the fix is removed — that is the durable net, the same
      confirmation CPE-1834 was required to give.
- [ ] Sweep the rest of both workflows for any other `Get-Content`/`Set-Content`/`Out-File` on a repo
      file, and fix or justify each. A partial sweep presented as complete is this repo's most-repeated
      defect.

## Notes

Found by the independent Reviewer during CPE-1834, while checking whether that ticket's `scripts/*.ps1`
sweep should have been wider. CPE-1834 correctly scoped itself to the glob its own acceptance criteria
named, so this is not a gap in that PR — but the ticket's Notes did mention `.github/workflows/*.yml`,
which is how it was found.

One generalisation from that review worth carrying: CP1252's decode table on this machine is a **total
bijection over all 256 byte values**, so misdecode-then-reencode is an identity transform for *any*
bytes, not only the characters anyone happened to test. That is why the bare/bare pipeline survives and
why any mixed pipeline does not.

Related: CPE-1834 (the same fix in `scripts/release.ps1`, merged), CPE-1841 (that script's unscoped
version regex), CPE-1788 (the guard that catches this class).
