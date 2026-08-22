---
id: CPE-1842
title: the Windows signing step's codec is a host default, not a guarantee
type: bug
priority: Medium
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

> **Corrected 2026-08-21 — the paragraph above is disproven; read it as the original hypothesis, not
> as fact.** Measured on both hosts (full four-way byte table in the Work Log, independently
> reproduced byte-for-byte by UAT and by the Reviewer): GitHub Actions `shell: pwsh` is **PowerShell
> 7**, which defaults to BOM-less UTF-8 on both the read and the write, so the em dash round-trips
> intact and **no BOM is written**. Nothing mangled has ever shipped. The corruption is real only
> under **Windows PowerShell 5.1**, which is not the host CI uses. This is a **latent portability
> defect** — the pipeline survives on an unstated runtime default rather than on anything the
> workflow states — not a shipped corruption. The title, priority and the "Why it matters" section
> below were revised to match; the filename slug was `git mv`d for the same reason.

## Why it matters

This is the code-signing step of the release pipeline — the least-watched code in the repo, executing
against a file that is one of the five that must stay version-synchronised. A mangled or BOM-prefixed
`tauri.conf.json` either fails the build confusingly or ships a subtly wrong manifest.

> **Corrected 2026-08-21 — the defensible stake is release AVAILABILITY, not user-visible text.** The
> UAT traced the em dash end to end. It is the **only** non-ASCII byte in the file and it sits in
> `plugins.cli.description`, not in bundle metadata. The `bundle` keys actually present are
> `active` / `targets` / `icon` / `resources` / `createUpdaterArtifacts` — there is no `publisher`,
> `shortDescription`, `longDescription` or `copyright` — so installer UI, Add/Remove Programs and
> VERSIONINFO all fall back to `productName` (ASCII) and `Cargo.toml`'s `description` (ASCII). The
> updater manifest's notes come from the release body, not from this file. And
> `tauri-plugin-cli-2.4.1/src/parser.rs:87-96` feeds `description` into clap's `.about()`, which the
> plugin stuffs into `matches.args["help"]` and **never prints**; the app reads only `open` and
> `test-mode`, and `main.rs:2` is `windows_subsystem = "windows"`, so a release build has no console
> to print to anyway. **Net: the mangled string is parsed into a clap field and discarded — zero
> user-visible surface.** What is genuinely at stake is that a 5.1 host **breaks the build, loudly**,
> and costs an hour to diagnose. That is a Medium. Do not lean on "the exact shape the mojibake guard
> exists to catch" as though text were reaching a user; it never was.

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

## Work Log

**2026-08-21** — Fixed both sites (`.github/workflows/release.yml` and
`.github/workflows/release-sidecar.yml`, the `Set up Windows code signing (CPE-1131 — self-signed)`
step in each) and added a workflow-source guard test. Measured first, as the acceptance criteria
require.

### Measurement — what actually ships today

The ticket's severity claim needs one correction, and it is the most important finding here.
**GitHub Actions `shell: pwsh` is PowerShell 7, not Windows PowerShell 5.1**, and PowerShell **v6 and
higher** (per Microsoft's `about_Character_Encoding`) changed both defaults this bug depends on: the
default input/output encoding became BOM-less UTF-8, and `-Encoding utf8` became an alias for
`utf8NoBOM`. PowerShell 7 was not installed on this machine, so it was installed as a `dotnet tool`
(**7.6.5 — the identical version the runner images ship**) purely to measure the real runner
behaviour, then removed again.

Ran the **verbatim current step body** (only the `Import-PfxCertificate` call replaced by a literal
thumbprint) against a scratch copy of the real `src-tauri/tauri.conf.json` — the tracked file was
never touched — under both hosts. Bytes around the live em dash in `plugins.cli.description`
(`"Cross-Platform Explorer — window geometry…"`), which is `E2 80 94` in the original:

| variant | length | BOM | em-dash bytes |
|---|---|---|---|
| original manifest | 3169 | none | `E2 80 94` |
| **old step, Windows PowerShell 5.1** (5.1.26100.9168) | 7409 | **`EF BB BF`** | **`C3 A2 E2 82 AC E2 80 9D`** |
| **old step, PowerShell 7.6.5** (what CI actually runs) | 3846 | none | `E2 80 94` |
| new step, Windows PowerShell 5.1 | 7399 | none | `E2 80 94` |
| new step, PowerShell 7.6.5 | 3844 | none | `E2 80 94` |

So: **nothing mangled or BOM'd has shipped.** On the real runner the old code round-trips the em dash
byte-for-byte and writes no BOM. The 5.1 row is exactly the double-encoded-em-dash mojibake the ticket
predicted — 8 bytes for 3, on top of a `EF BB BF` prefix — but that host is not the one CI
uses. This is therefore a **latent portability defect, not a shipped corruption**: the pipeline is
surviving on a runtime default rather than on anything the workflow states. It becomes the 5.1 row the
moment the step is changed to `shell: powershell`, run on an older image, or copied into a new step by
someone who assumes the idiom is safe. Recording that honestly rather than inheriting the ticket's
"this runs on every real Windows release build, today" framing, which is true of the *code path* but
not of the *corruption*.

(For completeness: `[System.Text.Encoding]::Default` resolves to `Windows-1252` under 5.1 on this
machine and `utf-8` under 7 — the same divergence CPE-1834 measured.)

### The fix

Both sites now do, matching CPE-1834's landed precedent exactly:

    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    $conf = 'src-tauri/tauri.conf.json'
    $confPath = (Resolve-Path $conf).Path
    $j = [System.IO.File]::ReadAllText($confPath, $utf8NoBom) | ConvertFrom-Json
    ...
    [System.IO.File]::WriteAllText($confPath, ($j | ConvertTo-Json -Depth 40), $utf8NoBom)

`Resolve-Path` is load-bearing, not cosmetic, and this is now **measured rather than argued**: the
Reviewer demonstrated that after a `Set-Location`, `(Get-Location).Path` and
`[System.IO.Directory]::GetCurrentDirectory()` diverge, at which point the relative-path `ReadAllText`
**throws** and the `Resolve-Path` form succeeds. It is also not a new failure mode: GitHub Actions
prepends `$ErrorActionPreference = 'stop'` to `pwsh` steps, and the old code already handed the same
bare relative `$conf` to `Get-Content`, so `Resolve-Path` succeeds exactly when the old code did and
fails exactly when it did.

The fix itself is verified byte-for-byte on both hosts (table above), not trusted from the flag name.
Each workflow's edit is 13 added / 2 removed lines — a targeted diff, confirmed with
`git diff --numstat`, not a whole-file rewrite.

### The two halves fail in OPPOSITE directions — why the guard must assert both

This is the finding that most justifies the guard's shape, and the first pass missed it. The UAT built
a probe replicating exactly what Tauri does with this file (`tauri-utils-2.9.3/src/config/parse.rs:282`
and `:352` — `fs::read_to_string` then `serde_json::from_str`, with **no BOM strip anywhere in that
file**) and ran the **half-fixed** variants under 5.1:

| variant | output | Tauri's verdict |
|---|---|---|
| read **OLD** / write **NEW** | 7216 B, no BOM, em dash = `C3 A2 E2 82 AC E2 80 9D` | **parses clean** and ships the mojibake — **FAIL-SILENT** |
| read **NEW** / write **OLD** | 7216 B, BOM `EF BB BF`, em dash intact | build **breaks** at config parse — **FAIL-LOUD** |

On 5.1 the two halves always fire together, so **the BOM masks the mojibake**: you get a red release
rather than a corrupt one. Which means **a half-fix would be strictly worse than no fix at all** —
fixing only the write half strips the loud failure and leaves the silent one, which is precisely the
double-encoding shape CPE-1834 measured and warned about. That is the real, previously unstated reason
the guard's last test insists on `$utf8NoBom` at **both** the `ReadAllText` and the `WriteAllText`
call: it reads as hygiene and is actually load-bearing. Now recorded in the test file's own header
comment as well, so the next reader does not have to re-derive it.

For completeness the UAT also measured `>` redirection, which nobody had tested: 5.1 writes UTF-16LE
(`FF FE`) and Tauri reports "stream did not contain valid UTF-8". Also fail-loud.

### `ConvertTo-Json -Depth 40` reformats the whole file — reported, scoped out

Confirmed and quantified. Under PowerShell 7 the rewrite is **semantically inert**: a full structural
comparison of the original against the rewritten manifest shows the only key-level difference is the
intended `bundle.windows` addition, and **key order is preserved throughout**. What changes is
whitespace only — `ConvertTo-Json` re-indents at 2 spaces and explodes every inline array/object onto
its own line, so the 3169-byte manifest becomes 3846 bytes: `"scope": ["**"]` becomes three lines, and
each of the eleven single-line CLI `args` entries becomes a five-line block.

The UAT proved the inertness more strongly than the structural walk above did: it **parsed** each
output and compared it against `orig + expected-patch`, and **OLD/pwsh7, NEW/pwsh7 and NEW/5.1 all
compare EQUAL**, with identical top-level key order. Only OLD/5.1 differs — and it differs because of
the corruption, not the reformat.

> **Correction to an earlier claim in this log.** The first pass attributed the 22 `\uXXXX` escapes in
> the 5.1 output to the old code, as though the fix removed them. It does not. They are `'` ×20 (the
> CSP's `'self'`) and `>` ×2 (the `>` in `longDescription`), they are a **Windows PowerShell 5.1
> `ConvertTo-Json` behaviour**, and they appear under the **fixed** code on 5.1 too. The fix neither
> causes nor removes them, and they are semantically free — `'` and `>` are just JSON
> spellings of the same two ASCII characters. (The other 5.1-only cosmetic, `"version":  "0.57.68"`
> double-spaced, is likewise a 5.1 formatter artifact, not an old-code artifact.)

**Scoped out, deliberately.** The rewritten file is runner-only: it is patched inside the checkout on
an ephemeral runner, consumed immediately by `tauri-action`, and never committed or uploaded. Nothing
downstream reads its formatting, and the whitespace churn is invisible to every consumer. Removing the
wholesale rewrite would mean either a surgical JSON text edit (fragile) or moving the whole
`bundle.windows` patch out of the file and into `tauri-action`'s `--config` overlay mechanism — which
is a real design change to how signing is configured, touches both workflows' build steps, and is well
outside an "S" bug fix about encoding. Not filing a follow-up: with the encoding hazard closed there is
no remaining defect here, only cosmetics on a file nobody keeps.

### The durable net

**The mojibake guard is not the net for this bug, and it is worth saying plainly.**
`src/lib/mojibakeGuard.ts`'s repo-wide scan walks `git ls-files` — tracked files only. The corrupted
manifest never becomes a tracked file: it exists for the length of one runner job. The guard would
never see it.

Confirmed the guard *detects* the corruption when handed the real measured bytes (fed the actual
scratch outputs from the table above through the guard's own exported functions, in a scratch vitest
file that was deleted before commit): on the 5.1 old output `hasLeadingBom` → `true` and `findMojibake`
→ one offender at line 48, its `match` being the three-character double-encoded em dash. On all three
good outputs (original, new-5.1, new-7) `hasLeadingBom` → `false`, `findMojibake` → `[]`,
`findFirstInvalidUtf8Byte` → `null`, `detectUtf16Bom` → `null`. So
the guard's *detector* is correct for this shape — it simply is not positioned to run on this file.

The net that actually holds is therefore a new **workflow-source** guard,
`src/lib/workflowPwshFileEncoding.test.ts` (5 tests). It parses every file in `.github/workflows/` with
the in-repo bounded-subset YAML parser (`src/lib/preview/yaml.ts`, the approach
`ciAptGetHardening.test.ts` established) and asserts:

1. every workflow parses (an unparseable one fails loudly rather than scanning nothing);
2. both signing steps are found by name, and both mention `src-tauri/tauri.conf.json` (the scan is not
   silently looking at zero steps);
3. **no** step's `run:` body contains `Get-Content` / `Set-Content` / `Add-Content` / `Out-File`,
   except one allowlisted line with a recorded reason;
4. every allowlist entry still matches a real line (no stale exemptions);
5. both signing steps contain `New-Object System.Text.UTF8Encoding($false)` and pass `$utf8NoBom` to
   both the `ReadAllText` and the `WriteAllText` call — so a future edit cannot leave one half explicit
   and the other on a default, which is precisely the mixed-pipeline shape this ticket's Notes flag as
   the worst case.

One thing the first draft of that test got wrong, worth recording because it is the trap
`ciAptGetHardening.test.ts` warns about, arriving from an unexpected direction: parsing the YAML does
*not* remove the problem of comments matching, because a `run:` block scalar keeps its own **shell**
comments verbatim, and this very fix adds comments naming `Get-Content` and `Set-Content`. The first
run produced four "offenders" that were all comment lines from the fix's own explanation. The test now
strips full-line `#` comments from each `run:` body before scanning (deliberately not trailing
comments — a `#` mid-line can be inside a string, and a false positive there is the safe direction to
err).

### Red-proof

Two single-line production reverts, each observed red, each restored:

1. `.github/workflows/release.yml:115` — replaced
   `[System.IO.File]::WriteAllText($confPath, ($j | ConvertTo-Json -Depth 40), $utf8NoBom)` with the
   original `($j | ConvertTo-Json -Depth 40) | Set-Content $conf -Encoding utf8`.
   → 2 of 5 tests failed: the cmdlet scan reported
   `release.yml [release / Set up Windows code signing…] run line 26`, and the explicit-encoder
   assertion reported the missing `[System.IO.File]::WriteAllText(`.
2. `.github/workflows/release-sidecar.yml` (the read half) — replaced
   `$j = [System.IO.File]::ReadAllText($confPath, $utf8NoBom) | ConvertFrom-Json` with the original
   `$j = Get-Content $conf -Raw | ConvertFrom-Json`.
   → 2 of 5 tests failed, naming `run line 18` and the missing `[System.IO.File]::ReadAllText(`.

Both halves are independently covered; neither is a test that only passes because the other is present.

### Sweep — every `Get-Content` / `Set-Content` / `Add-Content` / `Out-File` in the workflows

Searched all six files in `.github/workflows/`, not just the two named, and also enumerated every
`shell: pwsh` step and every `run:` step with no explicit `shell:` (which defaults to pwsh on a
Windows runner). Complete list of hits:

| file:line | occurrence | disposition |
|---|---|---|
| `.github/workflows/release.yml:97` (pre-fix) | `$j = Get-Content $conf -Raw \| ConvertFrom-Json` | **fixed** → `ReadAllText` + `$utf8NoBom` |
| `.github/workflows/release.yml:104` (pre-fix) | `($j \| ConvertTo-Json -Depth 40) \| Set-Content $conf -Encoding utf8` | **fixed** → `WriteAllText` + `$utf8NoBom` |
| `.github/workflows/release-sidecar.yml:465` (pre-fix) | `$j = Get-Content $conf -Raw \| ConvertFrom-Json` | **fixed** → `ReadAllText` + `$utf8NoBom` |
| `.github/workflows/release-sidecar.yml:472` (pre-fix) | `($j \| ConvertTo-Json -Depth 40) \| Set-Content $conf -Encoding utf8` | **fixed** → `WriteAllText` + `$utf8NoBom` |
| `.github/workflows/gui-smoke.yml:191` | `$PWD.Path \| Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append` | **justified, left alone** — **ASCII-only payload to a runner-managed file, so no encoding can alter its content.** Recorded in the new test's `ALLOWED_LINES` with that reason, and covered by test 4 so the exemption cannot go stale. |

> **Correction to this exemption's rationale.** The first pass justified it partly by claiming "the
> Actions runner parses `GITHUB_PATH` itself and tolerates the BOM." That claim is **unverified and
> has been removed** — it was also the wrong *shape* of argument to make inside a guard whose thesis
> is "do not rely on unstated defaults." Nor is it true that no BOM can appear: the Reviewer measured
> `Out-File -Encoding utf8 -Append` on both hosts and found that appending to a **non-empty** file
> writes no BOM on either, but appending to an **empty** file — which is exactly what the runner hands
> this step — writes `EF BB BF` on 5.1 and none on 7.6.5. So a BOM genuinely *can* be produced there,
> and what makes it safe today is the very same unstated pwsh-7 default this ticket exists to stop
> depending on (that step carries no `shell:` key, and `gui-smoke.yml:114` is
> `runs-on: windows-latest`, whose default shell is pwsh). The exemption now rests solely on the
> payload being ASCII, which is self-sufficient and true regardless of host.

No other hit exists in any of the six workflows. Also checked, and clear:

- **Only two `shell: pwsh` steps exist in the entire workflow directory** — the two signing steps. Every
  other PowerShell-capable step is either explicitly `shell: bash` or contains no text I/O.
- `.github/workflows/release.yml:91` and `release-sidecar.yml:460` use
  `[IO.File]::WriteAllBytes($pfx, …)` for the `.pfx` — a **byte** write with no codec involved, and to
  `$env:RUNNER_TEMP`, not the repo. Correct as-is.
- Every `echo … >> "$GITHUB_OUTPUT"` in both files (`release.yml:78,80,165,191`;
  `release-sidecar.yml:89,447,449`) is inside a `shell: bash` step writing a runner file, not a repo
  file. Not the hazard.
- `release-sidecar.yml:63` `cat > "$notes_file" <<'EOF'` writes a `mktemp` file under `shell: bash`.
  Not a repo file, no PowerShell codec.
- The `Stage pristine sidecar copies` (`release-sidecar.yml:171`) and `Stage native deps`
  (`release-sidecar.yml:234`) steps are both `shell: bash` and move/copy **binaries** (`cp`, `mv`,
  `unzip`) into the untracked `native-deps/` and `target/` trees. No text re-encoding.

### Independent mutation-testing of the guard (Reviewer + UAT)

Both reviews attacked the guard rather than reading it, and it survived mutations beyond the two
red-proofs above:

- **Swapping the write half to `Set-Content -Encoding utf8NoBOM`** — correct on pwsh 7, a *parameter
  error* on 5.1 — still **reds**, because the scan bans the cmdlet family outright rather than
  inspecting the `-Encoding` argument. That coarseness is a feature here.
- **A third `pwsh` step added to `ci.yml`** — a workflow the guard has no special knowledge of — was
  flagged on both risky lines. The scan really is generic across `.github/workflows/`, not hard-wired
  to the two release files.
- **Comment-stripping is anchored correctly**: an injected *executable* line carrying a `#` inside a
  double-quoted string, alongside a risky write, still fired. Anchoring to the first non-space
  character is the only correct place to strip.

Known gaps, recorded rather than bodged (all now in the test file's header comment too):

- `>` / `>>` redirection is not matched. Real gap — UTF-16LE-with-BOM on 5.1 — but **fail-loud** (Tauri
  reports "stream did not contain valid UTF-8"), and no `pwsh` step uses it today; every `>>` in these
  workflows is inside a `shell: bash` step.
- `gc` / `sc` aliases are not matched. **The only silent gap.** Adding `sc` to the regex would collide
  with `sc.exe`, so the coordinator is filing it separately; scope deliberately not widened here.
- `shell: pwsh` → `shell: powershell` with the code left fixed goes **green**, and that is correct
  rather than a miss: the post-fix code measured fully correct under 5.1 as well (NEW/5.1 row above),
  so it is host-independent. Guarding the code rather than the host is the right axis.

### Gates

- `npx vitest run src/lib/workflowPwshFileEncoding.test.ts` — **5/5 passed** (and 2/5 → red on each of
  the two single-line reverts above).
- `npx vitest run` (full frontend suite) — **321 files / 4263 tests passed**, 0 failed. Includes
  `src/lib/mojibakeGuard.test.ts`'s repo-wide scan, which is what proves the two edited workflow files
  are still clean BOM-less UTF-8 after being edited.
- `npm run check` — **0 errors, 0 warnings**.
- YAML syntax: all six workflow files parse under **PyYAML** (`release.yml` 2 jobs,
  `release-sidecar.yml` 2 jobs, plus `ci.yml` 6, `gui-smoke.yml` 4, `model-snapshot.yml` 1,
  `ffmpeg-pin-freshness.yml` 1) **and** under the in-repo `parseYaml`, via the new test's first case —
  two independent parsers.
- No Rust touched, so no clippy/cargo gate.

### Not verified

- **The fix has not been observed executing on a real GitHub Actions Windows runner.** The signing step
  only runs when `WINDOWS_CERT_PFX_BASE64` is set on a `windows-latest` runner and a version tag is
  pushed, so CI on this PR will not execute it, and the step was exercised locally with a literal
  thumbprint rather than a real imported certificate.

  **The PowerShell-version half of this caveat is retired** — it was over-cautious. The first pass
  hedged that the locally installed pwsh was "the same family, not the identical build." It is the
  **identical version**: `actions/runner-images` `Windows2025-Readme.md` (image 20260818.232.1) lists
  PowerShell **7.6.5**, exactly what was installed, and `Windows2022-Readme.md` lists 7.6.5 too, so it
  is label-independent. Microsoft documents the .NET-global-tool install as one of five peer
  installation methods for the same product, differing only in deployment. There is no version gap to
  reason about.
- The em dash is the only non-ASCII character currently in `tauri.conf.json`; behaviour was measured on
  that character, not on a wider corpus. CPE-1834 already measured the euro sign through the same code
  shape.

(`Resolve-Path` was previously listed here. It has been **promoted to measured** — see "The fix" above:
the Reviewer demonstrated the divergence and the throw empirically, and established that it introduces
no new failure mode.)

Everything written during measurement went to a `.scratch-cpe1842/` directory inside this worktree
(untracked, so `mojibakeGuard`'s `git ls-files` walk could never see it) and was deleted before commit;
the real `src-tauri/tauri.conf.json` was never written to. The locally installed `dotnet tool`
PowerShell was uninstalled afterwards — reproduce the pwsh 7 rows with
`dotnet tool install --global PowerShell`.
