---
id: CPE-1852
title: a failure on the third manifest leaves a half-bumped tree while the message says "refusing to write"
type: bug
priority: Medium
status: Doing
tags: ready
estimate: S
created: 2026-08-22
closed:
---

## Problem

`scripts/release.ps1`'s `Update-ManifestVersion` writes each manifest as it goes (`:203`). CPE-1841 added
a loud guard that refuses to write when a manifest does not carry exactly one version key — but the guard
fires **per file**, after earlier files are already on disk.

Measured by the independent Reviewer during CPE-1841:

```
package.json        -> 9.9.9   (written)
tauri.conf.json     -> 9.9.9   (written)
Cargo.toml          -> 0.1.0   (guard fired)
exit 1, "expected exactly one ... found 0. Refusing to write ..."
```

The message is true of the file it names and **reads as though nothing was written**. Two of the five
version-synchronised files are now bumped, on disk, uncommitted.

## Why Medium, not High

This is strictly **better** than what CPE-1841 replaced: the old script wrote all three and reported
success having changed nothing. The abort happens before any `git` call, so no tag and no push, and the
per-file `path: old -> new` lines do disclose what landed to a reader who looks.

But it lands squarely in the hazard CLAUDE.md already records by name — a dirty working tree after a
release operation reads as unrelated noise and gets committed by accident or discarded along with real
work. That is exactly how `package-lock.json` ended up three releases behind.

## Acceptance criteria

- [ ] Either validate all manifests **before writing any**, then write all of them; or name the
      already-written files in the failure so the message stops implying nothing changed. Prefer
      validate-all-then-write-all — it makes the operation atomic in the way the message already claims.
- [ ] The failure message must not say "refusing to write" while files have been written. Whatever wording
      results has to be true of the whole run, not of one file.
- [ ] A test stages a manifest set where the **third** file fails and asserts the first two are unchanged
      on disk. Red-proof it against the current behaviour — it must fail today.
- [ ] Check the same shape for `-BumpOnly` and for the full release path; they share the writer.
- [ ] Preserve everything CPE-1841 measured: exactly `1 1` per file on `git diff --numstat`, CRLF intact,
      trailing newline intact, no BOM added, BOM preserved where one was already present.

## Notes

Found by the independent Reviewer during CPE-1841, which correctly did not absorb it — that ticket's scope
was the unscoped version regex, and this is a separate transactional property.

Read CPE-1841's Work Log first. It carries two things worth not re-deriving: the `return , $hits` trap
(the comma operator hands the caller a one-element array wrapping the whole list, so an "exactly one match"
guard reads as satisfied regardless of the real count), and the measured byte-level round-trip that any
change here must not regress.

Related: CPE-1841 (the guard that fires), CPE-1853 (the two lockfiles this script still does not touch).

## Work Log

**2026-08-22 — fixed, tested, PR opened.** Branch `cpe-1852-atomic-manifest-bump`, off `origin/main`
`0891b1c4`. Worked in an isolated worktree under `.claude/worktrees/cpe-1852`.

### Host, stated up front

**There is no `pwsh` on this machine.** `which pwsh` → not found (the transient dotnet-tool shim CPE-1841
recorded is gone); `powershell -NoProfile -Command '$PSVersionTable.PSVersion'` → **5.1.26100.9168**.
So **every local measurement below ran under Windows PowerShell 5.1**, and the test harness's
`findPowerShellHost()` (which prefers `pwsh`, falls back to `powershell`) resolved to `powershell` here.
The reproducible PowerShell 7 evidence is CI's `Frontend — type-check and test` leg on `ubuntu-latest`,
where `pwsh` is the only host — that leg runs this very suite. No tool was installed or removed.

### The fix — validate all, then write all

`Update-ManifestVersion` did read → validate → **write**, once per manifest, in a straight line. Split in
two:

- **`New-ManifestVersionPlan`** — read, BOM-sniff, locate, count, splice, re-check through the same
  locator, and `return` a plan hashtable `@{ Path; Old; New; Text; Encoding }`. **Touches no disk.**
- **`Write-ManifestVersionPlan`** — writes one already-validated plan and prints its `path: old -> new`.

The call site builds `$plans = @(plan1; plan2; plan3)` and only then loops the writes. A throw from any
plan happens with nothing written, which is what makes the operation atomic in the way the message
already claimed. A hashtable (not a `PSCustomObject`) is returned deliberately: PowerShell emits it as
one object, so the caller's array subexpression sees exactly three elements.

### The message now true of the whole run

Was: `... found 0. Refusing to write -- a manifest that no longer matches must fail ...`
Now: `... found 0. No manifest was written -- every manifest is validated before any is written, so the
working tree is exactly as it was. A manifest that no longer matches must fail the release loudly, not be
written back unchanged and reported as bumped.`

The post-splice self-check message likewise changed `Nothing was written.` → `No manifest was written.`,
and it is now a *splice* check (it runs before the write, not after it), so it is named one.

**The residual I/O case is reported honestly rather than pretended away.** Once every plan validates, a
write can still fail on a locked or read-only file. No pure-PowerShell scheme makes three
`WriteAllText` calls atomic, so the write loop catches and reports `PARTIAL BUMP. Already written at
vX.Y.Z: <paths>` with a revert instruction — true of the run, and it names the files. That path is
untested (see "Not verified").

### Red-proof, against the current behaviour, before the fix

New `describe` block "release.ps1 leaves no half-bumped tree when a LATER manifest fails (CPE-1852)",
6 tests, run against the **unfixed** script:

```
Tests  5 failed | 26 passed (31)
```

The five that red are exactly the atomicity claims — third-manifest bytes, no phantom `old -> new` lines,
whole-run wording, second-manifest position, and the full (non-`-BumpOnly`) path. The sixth ("still
writes all three together on the success path") correctly stayed green: it is the companion arm that
stops "write nothing, ever" from being a passing fix. After the fix: **31 passed / 31**.

The failing-run stdout captured during the red proof is the ticket's exact reproduction:

```
Releasing v9.9.9...
  ...\package.json: 0.1.0 -> 9.9.9
  ...\src-tauri\tauri.conf.json: 0.1.0 -> 9.9.9
release.ps1: expected exactly one version key inside [package] in ...\Cargo.toml, found 0. Refusing to write ...
```

### `-BumpOnly` and the full release path — both covered

`runBump` grew a `bumpOnly?: boolean` option (default `true`, so every existing case is unchanged). One
test drives the script **without** `-BumpOnly` over a fixture whose third manifest fails, asserting the
first two are byte-identical and that nothing git-shaped ran. Safe by construction: validation aborts
before the first `git` call, and the scratch tree is an OS temp directory that is not a repo at all, so a
regression reaching `git add` would die there rather than commit anything. That is the point of the test —
the atomicity is a property of the shared writer, not of the dry-run switch.

### Measured on the REAL manifests (throwaway copies, never the tracked files)

Copied the worktree's actual `package.json` / `src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml` and
`scripts/release.ps1` into a scratch git repo **inside the worktree**
(`.claude/worktrees/cpe-1852/.scratch-cpe1852/`, deleted afterwards), so no tracked file was ever written
by PowerShell.

Success path, `0.57.68 -> 9.9.9`, Windows PowerShell 5.1:

```
git diff --numstat
1       1       package.json
1       1       src-tauri/Cargo.toml
1       1       src-tauri/tauri.conf.json
```

Byte level, all three: `loneLF=0` (CRLF intact) / `bom=False` / trailing `\r\n` intact.

Failure path on the same real manifests (`[package]`'s version line deleted, so Cargo.toml fails):

| script | package.json md5 | tauri.conf.json md5 | message |
|---|---|---|---|
| before (`origin/main`) | `1d81e357…` → **`32b3d78b…`** | `65cb42a2…` → **`96d9261d…`** | "Refusing to write" |
| after (this branch) | `1d81e357…` → `1d81e357…` | `65cb42a2…` → `65cb42a2…` | "No manifest was written" |

The old script printed both `old -> new` lines and left `numstat` showing all three files modified. The
new one prints neither and leaves only the file I had deliberately broken.

`scripts/release.ps1` itself after editing (Edit tool, no `sed -i`, no PowerShell write): `loneLF=0`,
`bom=False`, trailing `\r\n` intact.

### CPE-1841's measurements — all preserved

The BOM-preserve path, the `$hadBom` sniff, the `return $hits` (no unary comma) contract, and every
locator are untouched; the read/BOM logic simply moved inside the plan function. All 25 pre-existing tests
in the file still pass, including the six BOM cases and the six loud-failure cases.

### Docs

`RELEASING.md` now states the all-or-nothing property next to the loud-failure paragraph, so the runbook
says the tree is clean after an abort — which is now true and previously was not.

### Gates (all local runs on Windows PowerShell 5.1 / Node)

- `npx vitest run` (full): **324 files / 4317 tests passed**, 0 failed.
- `npm run check`: **0 errors, 0 warnings**.
- `src/lib/releaseVersionBump.test.ts` alone: **31 passed** (was 25; +6).
- No Rust touched, so no cargo gate applies.

### Not verified

- **PowerShell 7 locally** — impossible here, there is no `pwsh` on this machine and installing one is
  exactly the machine-global change that made CPE-1841's numbers unreproducible. CI's ubuntu leg carries
  it; the suite reds loudly rather than skipping if a runner ever loses its host.
- **The partial-write I/O path** — the `catch` that reports `PARTIAL BUMP` needs a `WriteAllText` to fail
  after validation (locked / read-only / full disk). Not staged; it is reported-correctly-by-construction,
  not tested.
- **The git half** of the script (`add`/`commit`/`tag`/`push`) is untouched and never exercised on a real
  repo. No release was cut.
