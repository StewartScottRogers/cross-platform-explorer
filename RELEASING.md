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

**Why the script has to do this rather than a build check catching it.** Neither build passes
`--locked`, so a stale lockfile version is silently rewritten at build time and never fails anything;
`npm ci` does not check the `version` field either (it checks the dependency graph), which is how
`package-lock.json` sat three releases behind — `0.57.64` against `0.57.67` — through many green CI
runs. Adding `--locked` to the Rust builds would make that drift fail loudly on its own; see
CPE-1853's Work Log for the measured recommendation.

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
$job = gh run view $runId --repo StewartScottRogers/cross-platform-explorer --json jobs `
  --jq '.jobs[] | select(.name=="verify-published-manifest-sidecar")' | ConvertFrom-Json
if (-not $job -or $job.conclusion -ne "success") {
  throw "verify-published-manifest-sidecar did not pass (conclusion: $($job.conclusion)) -- do not publish"
}
```

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
