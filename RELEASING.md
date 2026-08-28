# Releasing & maintaining (desktop runbook)

This is the operating manual for managing **cross-platform-explorer** from the
Cowork desktop app. In a desktop session you can just say what you want in plain
language ("cut a release 0.2.0", "check the build", "what needs updating") and
Claude follows the steps below. The `gh` CLI on this machine is already
authenticated as `StewartScottRogers`.

## Cut a new release

A release is triggered by pushing a `vX.Y.Z` tag. The version must match in
**five** files (CLAUDE.md's "keep five files in sync" list), so use the helper
script — it edits all five, commits, tags, and pushes in one go:

```powershell
cd Z:\repos\cross-platform-explorer
./scripts/release.ps1 -Version 0.2.0
```

Add `-BumpOnly` to edit the five files and stop there — no commit, no tag, no push — when you
want to read the diff before anything leaves the machine (CPE-1841):

```powershell
./scripts/release.ps1 -Version 0.2.0 -BumpOnly
git diff --numstat   # expect `1  1` for four of them and `2  2` for package-lock.json

# then put them back -- a dry run must not leave the tree dirty
git checkout -- package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml package-lock.json src-tauri/Cargo.lock
```

That last line is not optional housekeeping. Five modified files left behind read as exactly the
"unrelated noise" this file's own five-files-in-sync section warns about — the kind that gets committed
by accident or discarded along with real work.

`package-lock.json` is `2  2` rather than `1  1` because it carries the app version **twice**: the root
object's `"version"` and `packages[""]`'s. Both move together or the run aborts; bumping one and leaving
the other is the specific way that file goes stale (CPE-1853).

The script bumps only each file's own version — `package.json`'s and `tauri.conf.json`'s **top-level**
`"version"`, `Cargo.toml`'s `version` inside **`[package]`**, `package-lock.json`'s two app-version
fields, and `Cargo.lock`'s `version` in the `[[package]]` block **named `cross-platform-explorer`**. A
dependency pin, a nested tool version, or a version number inside a description or URL is left alone —
including a pin whose version happens to equal the app's, which in `Cargo.lock` means ~1000 entries the
bump must not touch. A file that no longer matches at all aborts the release loudly instead of being
written back unchanged and reported as bumped (CPE-1841/CPE-1853; guarded by
`src/lib/releaseVersionBump.test.ts`).

That abort is **all-or-nothing across all five files**: every one of them is read and validated before
any is written, so a `Cargo.lock` that fails the check leaves the other four untouched rather than
already bumped (CPE-1852). If the script does abort, the tree is clean and there is nothing to revert —
the failure message says so, and it is true of the whole run.

**Why the script does this, and what now backs it up between releases.** A stale lockfile version used
to fail nothing: neither build passed `--locked`, so it was silently rewritten at build time, and
`npm ci` does not check the `version` field either (it checks the dependency graph). That is how
`package-lock.json` sat three releases behind — `0.57.64` against `0.57.67` — through many green CI runs.

Both gaps are closed now, by two different mechanisms:

- The Rust lockfiles get `--locked` (CPE-1865) plus the `lockfile-preflight` CI job (CPE-1932) — a stale
  `src-tauri/Cargo.lock` is exit 101.
- npm has **no** `--locked` for these fields — `npm ci` exits 0 on a three-release drift and `npm install`
  silently repairs it, both measured — so `src/lib/appVersionSync.test.ts` (CPE-1904) compares all six
  places on every push, PR and local `npm test`, and names the file, the field, both values and the fix.

The script's own all-five check still matters: it is what makes the *bump* all-or-nothing, and it runs
before anything is committed or tagged. The test is what catches drift introduced **between** releases,
which is where the recorded incident actually came from.

What happens next, automatically:

1. GitHub Actions builds signed installers for Windows, macOS (universal), and
   Linux.
2. A **draft** GitHub Release is created with the installers and `latest.json`.
3. You review the draft and publish it.
4. Installed apps pick up the update on their next launch.

To publish the draft once the build is green:

```powershell
gh release edit v0.2.0 --draft=false
```

## Auto-update channel — the sidecar release is the update stream (CPE-768)

The app the installer/`/run` puts on disk is the **sidecar** build (`Cross-Platform Explorer
(Sidecar)`), so that is the channel auto-update follows. The updater endpoint in
`src-tauri/tauri.conf.json` is:

```
https://github.com/StewartScottRogers/cross-platform-explorer/releases/latest/download/latest.json
```

GitHub's `/releases/latest/` resolves to the newest **non-prerelease, non-draft** release, so for
auto-update to work the sidecar release must be **non-prerelease** and must carry a signed
`latest.json`. That is now wired up:

- `.github/workflows/release-sidecar.yml` — `prerelease: false`, `includeUpdaterJson: true`
  (still `releaseDraft: true` so you review before publishing).
- `src-tauri/tauri.sidecar.conf.json` — `bundle.createUpdaterArtifacts: true`, so `latest.json`
  references the correctly-named `…Sidecar…` installers, signed with the shared updater key.

So the flow is: dispatch **Release (sidecar-enabled)** with a new `vX.Y.Z-sidecar` tag → review the
draft → **publish it** → installed apps pick up the update on their next check. Publishing is what
makes it the `/releases/latest/` the updater sees.

**Before publishing, check `verify-published-manifest-sidecar` passed (CPE-1908).** Exactly the same
reasoning as `run.md`'s plain-channel gate (CPE-1872): having installer assets on the draft is not the
same as those assets being *safe* to publish. This job re-checks the manifest as it actually sits on the
draft — every platform's minisign signature against the configured pubkey, that every platform's `url`
points at this repo's own release rather than a foreign host/wrong tag, **and** (CPE-1908) that every
platform's asset is actually from the **sidecar** channel, not a plain-channel asset that slipped in.

**If you publish via `/run` (saying "Run"), this check now runs automatically** (CPE-1908 round 2):
`/run`'s step 1a always installs the *latest* release regardless of channel, and — because this
project's shipping strategy is sidecar-only — that is very often a `-sidecar` tag. `run.md`'s step
1b-ii branches on the tag suffix and checks `verify-published-manifest-sidecar` on the sidecar path,
the same way it already checked `verify-published-manifest` on the plain path. An earlier draft of
this doc claimed there was no automated flow to wire this into; that was wrong (`/run` *is* the
publish path for this channel in practice) and has been corrected here and in `run.md` itself.

The check below is for publishing **by hand**, outside `/run` — e.g. from this doc's own
dispatch-and-review flow, or the Cowork desktop app:

```powershell
$runId = gh run list --repo StewartScottRogers/cross-platform-explorer --workflow=release-sidecar.yml `
  --limit 1 --json databaseId --jq '.[0].databaseId'
# CPE-1918: the `--jq` filter deliberately contains NO double quotes. Windows PowerShell 5.1 strips
# `"` when it marshals an argument to a native exe's argv, so a filter like
# `select(.name=="verify-published-manifest-sidecar")` reaches jq unquoted and dies with
# `function not defined: sidecar/0` instead of this check's crafted message. Do the name match in
# PowerShell instead — `-ceq` is exact and case-sensitive, matching jq's `==`. See the
# "PowerShell and `gh --jq`" note below before editing any snippet in this file.
$jobs = gh run view $runId --repo StewartScottRogers/cross-platform-explorer --json jobs `
  --jq '.jobs' | ConvertFrom-Json
$job = $jobs | Where-Object { $_.name -ceq 'verify-published-manifest-sidecar' }
if (-not $job -or $job.conclusion -ne "success") {
  throw "verify-published-manifest-sidecar did not pass (conclusion: $($job.conclusion)) -- do not publish"
}
```

> **`$jobs` is assigned first on purpose — it is a real bug, but a *fail-safe* one.** In Windows
> PowerShell 5.1 `ConvertFrom-Json` emits a JSON array as a **single** pipeline object, so
> `… | ConvertFrom-Json | Where-Object { $_.name -ceq '…' }` evaluates `$_.name` against the whole
> array. When the name exists the comparison is non-empty, `Where-Object` reads that as true, and
> `$job` becomes the **entire array**. The gate then answers *"did every job on this run succeed?"*
> instead of *"did the verify job succeed?"* — because `@(…) -ne 'success'` is an array **filter**, so
> `$job.conclusion -ne "success"` is false only when **every** conclusion is `success`.
>
> Measured on real PS 5.1 with controlled job arrays:
>
> | scenario | trap | correct form |
> |---|---|---|
> | target `skipped`, a leg `cancelled` (partial matrix) | matched=4, **THROWS** | matched=1, **THROWS** |
> | target `success`, a sibling `failed` | matched=3, **THROWS** | matched=1, **PASSES** |
> | all `success` | PASSES | PASSES |
> | target absent | THROWS | THROWS |
> | target `failure` | THROWS | THROWS |
>
> So it **can never allow a publish the correct form refuses** — passing requires every conclusion to
> be `success`, which entails the target's own. Its actual failure mode is the opposite: it *refuses* a
> publish the correct form allows (row 2), and its refusal message lists every job's conclusion instead
> of the one that matters — `(conclusion: success success cancelled skipped)` rather than
> `(conclusion: skipped)`. Wrong, and confusing at 2am, but never unsafe. Assigning to `$jobs` and
> piping the variable enumerates it, so exactly one job matches. (Verified, CPE-1918.)

`--limit 1` assumes you check this immediately after dispatching — if another sidecar dispatch races
yours, resolve the run by its `displayTitle`/`createdAt` instead of trusting "most recent" (`run.md`'s
copy of this check matches on `displayTitle` for exactly this reason — `release-sidecar.yml` now sets
`run-name: "Release (sidecar) ${{ inputs.tag }}"` so the tag is actually visible there). A missing or
non-`success` job means STOP: do not `gh release edit --draft=false` this tag.

**What this does *not* close:** nothing on GitHub can force a human to run either check above before
typing `gh release edit <TAG> --draft=false` directly, bypassing both `/run` and this doc's own
instructions — there is no server-side hook on GitHub Releases that can require a workflow conclusion
before a publish. This is the SAME residual gap the plain channel's `run.md` gate has always had: a CI
job can make the failure impossible to *silently* miss (a red run in the Actions tab, not a silent
skip), but it cannot physically stop a manual command that skips checking it. Judged acceptable for the
same reason the plain channel's gap is — the publish step is a deliberate, manual, low-frequency
action — and now that `/run` covers the common case automatically, the realistic remaining exposure is
narrower than it was when this section was first written.

### PowerShell and `gh --jq` — never put a `"` in the filter (CPE-1918)

Every `powershell`-fenced snippet in this repo's runbooks is meant to be **pasted verbatim** into Windows
PowerShell 5.1. That shell mangles double quotes on their way into a native executable's argv, so a
`--jq` filter containing a string literal breaks — and the two escapes everyone reaches for break too.
Measured on PS 5.1 (26100.9168) with `node -e "console.log(JSON.stringify(process.argv.slice(1)))"`:

| Snippet as written | What `jq` actually receives | |
|---|---|---|
| `--jq '.jobs[] \| select(.name=="x")'` | `.jobs[] \| select(.name==x)` | ✗ `function not defined: x/0` |
| `--jq ".jobs[] \| select(.name==\"x\")"` | `.jobs[] \| select(.name==" x\)` | ✗ `invalid escape sequence "\)"` |
| ``--jq ".jobs[] \| select(.name==`"x`")"`` | `.jobs[] \| select(.name==x)` | ✗ same as the first |
| `$q = '…"x"…'; --jq $q` | quotes still stripped | ✗ a variable does not help |
| `--jq='.jobs[] \| select(.name=="x")'` | `.jobs[] \| select(.name==x)` | ✗ the `=` form fails the same way |
| `--jq '.jobs'` | `.jobs` | ✓ no `"` in the argument |
| `--jq '.jobs[] \| select(.name==\"x\")'` | `.jobs[] \| select(.name=="x")` | ✓ but **banned anyway** — see below |
| `--jq '.jobs[] \| select(.name==""x"")'` | `.jobs[] \| select(.name=="x")` | ✓ but **banned anyway** — see below |

Note the second row: the escaped-double-quote form was long assumed correct here and in `run.md`, and
it is **not** — it is broken in a *different* way, which is how the bug survived being "fixed" once.

**The rule: a `--jq` / `-q` argument in a PowerShell snippet must contain no `"` at all.** Use `--jq`
only to pluck the sub-tree (`'.jobs'`, `'.[0].databaseId'`, `'.assets[].name'`) and do any string
matching in PowerShell with `ConvertFrom-Json` + `Where-Object { $_.field -ceq 'literal' }`. Use `-ceq`,
not `-eq`: PowerShell's `-eq` is case-insensitive where jq's `==` is not, and these matches gate a
publish. `src/lib/runbookJqQuoting.test.ts` fails CI if a `"` reappears inside a `--jq` argument in a
PowerShell block.

**That rule is deliberately stricter than strictly necessary, and this clause is here so nobody
"corrects" it back.** The last two rows above are measured, not hypothetical: single quotes with a
*backslash*-escaped or *doubled* inner quote really do reach `jq` intact. They are still banned,
because what separates them from the four broken rows is a single character in a position no reader
can verify by eye — and this exact class already regressed once, when the row-2 form was believed
correct, cited as the fix, and then copied forward by CPE-1908. A reader who discovers the backslash
form working has found a true fact, not a bug in the rule.

The `--jq '…"…"…'` form is *correct* in the one `.github/workflows/**` step that uses it
(`ffmpeg-pin-freshness.yml`, the FFmpeg tag walk) — that step runs under `bash` on `ubuntu-latest`,
where single quotes are honoured. Every other workflow `--jq`/`-q` filter is quote-free anyway, and the
two `shell: pwsh` steps in the repo (`release.yml` and `release-sidecar.yml`, both the Windows
cert-signing step) have no `--jq` at all. The shell the snippet targets is the whole difference, which
is why the guard also fails any fenced block that runs a `gh` command without naming a specific shell —
an info string like ```` ```console ```` is not a shell name, and a PowerShell snippet wearing one
would be skipped by the quote check *and* pass a naive "has a tag" check.

**Gotchas:**
- A sidecar release left as a **prerelease** (or draft) is invisible to `/releases/latest/` — the
  endpoint will fall through to an older release (or 404 if none carries `latest.json`). Don't mark
  sidecar releases prerelease.
- Bump the version in the three manifests (see below) before cutting, or `latest.json` will report the
  same version and every install just says "up to date".
- The plain `release.yml` channel (`vX.Y.Z` tags) also publishes non-prerelease with `latest.json`; if
  you ever cut a plain release it competes with the sidecar release for `/releases/latest/`. The
  shipping strategy is sidecar-only, so avoid cutting plain releases unless you intend to switch
  channels.

## Check build / CI status

```powershell
cd Z:\repos\cross-platform-explorer
gh run list --limit 5
gh run watch          # live-follow the most recent run
gh run view --log-failed   # show logs of failed steps
```

### You are not the alarm — the watchdog is (CPE-1872/CPE-1917)

`gh run list` shows the most recent runs of *every* workflow, which is close to useless for a release
workflow that only fires on a version tag: `release.yml` failed on **every** run for 27 days
(2026-08-04 → 2026-08-23, six tags) and nobody noticed, because the workflow that ships the app people
actually install is `release-sidecar.yml` and nobody watches the other one's Actions tab. `catalog` —
which publishes the signed agent-catalog bundle — was `skipped` on all of those runs, and `skipped`
reads exactly like a job that correctly had nothing to do.

So do not rely on looking. Two automated backstops exist and are the thing to check:

- **`release-pipeline-watchdog.yml`** files/updates a deduped GitHub issue labelled
  `release-pipeline-red` whenever either release workflow finishes with anything other than success.
  **An open issue with that label means a release pipeline is broken right now.**
- **`catalog-freshness.yml`** runs on a schedule and files an issue if the *live* catalog a real
  client would fetch is missing or older than its threshold — the backstop for the ways the bundle can
  stop shipping without any workflow going red at all.

`gh issue list --label release-pipeline-red --state open` is a better health check than `gh run list`.
Both alarms are ratcheted by tests (`src/lib/releaseVerifyWiringGuard.test.ts`,
`src/lib/catalogPublishFreshnessGuard.test.ts`) so they cannot be silently disconnected from what they
watch.

## Check what needs updating

```powershell
cd Z:\repos\cross-platform-explorer
npm outdated                       # frontend deps
npx @tauri-apps/cli info           # Tauri / toolchain versions
# Rust deps (needs cargo; CI has it even if this host doesn't):
#   cargo update --dry-run   (run inside src-tauri)
```

## Manual version bump (if not using the script)

Bump the SAME version in all three, then tag:

- `package.json` → `"version"`
- `src-tauri/Cargo.toml` → `version = "..."`
- `src-tauri/tauri.conf.json` → `"version"`

```powershell
git commit -am "release v0.2.0"
git tag v0.2.0
git push origin main --tags
```

## Verify the sidecar actually updated (CPE-483)

After installing an update, **a stale sidecar can masquerade as up-to-date**: the registry/app version
reflects the *host* exe, not the bundled `sidecars\ai-console.exe`. If a leftover
`ai-console --session-daemon` held that binary file-locked during install, NSIS silently skips it and
you end up with a new host running an **old** sidecar (the "black terminal" saga).

So don't trust the version number alone — verify the timestamps match:

```powershell
$d = "<InstallLocation>"   # e.g. C:\Users\...\Cross-Platform Explorer
Get-Item "$d\cross-platform-explorer.exe","$d\sidecars\ai-console.exe" | Select-Object Name, LastWriteTime
```

A `sidecars\ai-console.exe` `LastWriteTime` lagging the host exe means it was locked and NOT replaced.
Kill **all** `cross-platform-explorer` + `ai-console` processes (incl. `--session-daemon`), clear
`%TEMP%\cpe-ai-console`, and reinstall. The app also self-heals: on startup it reaps orphaned
session-daemons before they can lock the binary (`sidecar/host/src/reaper.rs`), and `/run` + `/remove`
kill-all before touching the installer.

## Signing keys — do not lose these

- Updater signing key: stored as repo secrets `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. A local backup lives in
  `updater.key` / `updater.pw` (both gitignored — never commit them).
- Losing the private key OR password means you can no longer sign updates and
  auto-update breaks for existing installs.
- **Deleting `TAURI_SIGNING_PRIVATE_KEY` no longer produces a green no-op (CPE-1923 finding 4).**
  Both channels' verify jobs (`release.yml`'s `verify-published-manifest` and
  `release-sidecar.yml`'s `verify-published-manifest-sidecar`) gate their real steps on that secret
  being present, so with it unset each job used to run, skip both steps, and conclude `success` — a
  release-integrity gate reporting green over zero verification, indistinguishable in the run
  summary from one that actually checked something. One deleted secret disarmed both. Each
  secret-detection step now **fails the job** when a run that is *cutting a release* finds no key.
  If you see it, the fix is to restore the secret, never to publish the release anyway.
  (The two workflows answer "is this cutting a release?" differently — `release.yml` from
  `github.ref_type == 'tag'`, `release-sidecar.yml` unconditionally, since it is dispatch-only with
  a required tag input — but run a byte-identical script, and a test asserts that equality.)

### What the release gate does and does not prove

`verify-release-artifacts` (run by `verify-published-manifest`) checks the manifest as **published on
the draft release**, plus every asset it names, and refuses on: a signature that does not verify, a
platform it could not fetch, a `url` outside this tag's download prefix, a mixed release channel, a
platform key serving another OS's payload, and — the one an attacker with release-asset write but no
signing key would otherwise walk through — **an artifact belonging to a different release than the
one being shipped**.

**Three of those refusals — release channel, platform/payload mapping, and artifact/release binding
— are decided from the artifact's *signed* name** (the `file:` field of the minisign trusted comment,
which the global signature covers), not from the name it was uploaded under. The distinction is the
whole point, and getting it wrong was this gate's most recent real defect twice over: an asset-write
attacker chooses the upload name freely, so earlier versions of the binding and the mapping check
were each defeated by simply renaming the upload — the old genuinely-signed installer uploaded under
a current-looking filename (a downgrade), and this release's genuine Linux `.deb` uploaded under a
Windows platform key as `..._x64-setup.exe` (denial-of-update). Changing the signed name requires the
signing key, which is exactly the capability that threat model withholds.

The channel and mapping checks also run once over the uploaded names *before* download — that pass is
cheap and gates what gets fetched, but it proves nothing on its own and is not what the gate rests on.

The three checks added for this (channel, platform/payload mapping, artifact/release binding) prefix
their refusals with `PROPERTY FAILED -- <property>`. The older refusals — pubkey pin, manifest-vs-config
version mismatch, tampered artifact, missing manifest — predate that convention and do not carry the
prefix; they still name what went wrong in prose.

**One residual is deliberate and tracked as CPE-1942.** Tauri signs the macOS artifact as
`<productName>.app.tar.gz`, with no version in the signed name either — verified against the real
published `.sig` assets — so there is nothing to bind that one artifact kind against. It is admitted
on its url prefix and signature alone. The exemption is narrow: it applies only to a `darwin-*`
platform whose *signed* name ends `.app.tar.gz`, so other signed bytes cannot claim it by being
renamed. The run prints every exemption it grants, with the signed name.

### OS installer code signing

- **Windows — self-signed (CPE-1131):** release `.exe`/`.msi` installers are Authenticode-signed in CI
  with a self-signed cert, stored as repo secrets `WINDOWS_CERT_PFX_BASE64` + `WINDOWS_CERT_PASSWORD`.
  The signing step in `release.yml` is **conditional** — it skips (unsigned, still green) when the secret
  is absent, so forks/PRs aren't broken. Caveat: self-signing does **not** clear SmartScreen for the
  public — only a CA OV/EV cert does (that's still **CPE-002**). Public cert + trust/rotate instructions:
  [docs/signing/README.md](docs/signing/README.md).
- **macOS (CPE-002, still Blocked):** needs an Apple Developer ID cert + notarization — not self-signable.

## Status dashboard

`STATUS.html` (gitignored) is a local dashboard refreshed by the scheduled task
`cpe-daily-status`. Open it any time; run the task manually to refresh on demand.
