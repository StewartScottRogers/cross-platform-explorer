# Run — Publish, Download, Install, and Launch

Publish the latest release if it is still a draft, then download it, install it, and launch it.
Triggered when the user says **"Run"** (or `/run`).

Then present an action menu following the rules in menu-render.md.

Repo: `StewartScottRogers/cross-platform-explorer` (public). The `gh` CLI is authenticated.

---

## Step 1 — Find the Latest Release, and Publish It If It Is a Draft

`/run` always installs the **latest** release. If that release is still an unpublished draft, publish
it first — do not dead-end.

**1a. Find the latest release (drafts included):**
```powershell
gh release list --repo StewartScottRogers/cross-platform-explorer --limit 1 --json tagName,isDraft,isPrerelease
```

**If there are NO releases at all**, STOP. Do not install anything. Report:

> No release exists yet. Cut one with `./scripts/release.ps1 -Version X.Y.Z`, wait for CI to build
> the installers, then say "Run" again.

Render the "No Release" menu (below) and stop.

**1b. If the latest release is a draft, check it actually has installer assets BEFORE publishing:**
```powershell
gh release view <TAG> --repo StewartScottRogers/cross-platform-explorer --json assets --jq '.assets[].name'
```

This guard matters. A draft with no assets means the release build failed or is still running —
publishing it would create an empty public release with nothing to download. If the asset list is
empty or is missing an installer for the current OS:

- If a release run is still **in progress** (`gh run list --limit 3`), say so and offer to wait.
- If the run **failed**, STOP, report it, and point at `gh run view --log-failed`.
- Either way, do NOT publish, and do NOT install.

**1b-ii. Confirm the manifest was verified before publishing (CPE-1872, CPE-1908).** Having installer
assets is not the same as those assets being SAFE to publish. Both release channels have a job that
re-checks the manifest exactly as it now sits on the draft — every platform's minisign signature
against the configured pubkey, that every platform's `url` actually points at this repo's own release
rather than a foreign host or the wrong tag serving a same-named asset, and (CPE-1908) that every
platform's asset is actually from the channel this tag claims to be. Publishing without checking this
job's result would let an unverified, url-spoofed, or channel-mixed `latest.json` go live to the real
auto-updater.

**`/run`'s "latest release" is channel-agnostic and is very often the SIDECAR channel** (CPE-1908 round
2, Reviewer): `gh release list --limit 1` in step 1a returns whichever release is newest regardless of
which workflow built it, and this project's shipping strategy is sidecar-only (RELEASING.md), so most
tags reaching this step end in `-sidecar`. The two channels' verify job lives in a DIFFERENT workflow
under a DIFFERENT name, and — because `release-sidecar.yml` is `workflow_dispatch`-only — its runs
can't be found the same way `release.yml`'s tag-triggered runs can: a `workflow_dispatch` run's
`headBranch` is always the dispatched REF (e.g. `main`), never the tag, so copying the plain-channel
lookup's `select(.headBranch=="<TAG>")` would silently match the wrong run (or none). Branch on the tag
suffix and use the right lookup for each:

```powershell
if ("<TAG>".EndsWith("-sidecar")) {
  $workflow = "release-sidecar.yml"
  $jobName = "verify-published-manifest-sidecar"
  # workflow_dispatch runs have no tag-bearing headBranch to match on -- release-sidecar.yml sets
  # `run-name: "Release (sidecar) ${{ inputs.tag }}"` (CPE-1908) specifically so the tag shows up in
  # displayTitle instead. EXACT match, not `contains` (CPE-1908 round 3, R2-4 -- a security-relevant
  # fix on the publish path): `contains("<TAG>")` also matches an honestly-dispatched run for a
  # DIFFERENT, decoy tag that merely contains this one as a substring (e.g. a tampered
  # "v1.2.3-sidecar-decoy" run's displayTitle "Release (sidecar) v1.2.3-sidecar-decoy" contains
  # "v1.2.3-sidecar"), so a same-day decoy dispatch could get matched, read as `success`, and this
  # step would then wave an UNVERIFIED draft through to `gh release edit --draft=false`. Assumes
  # you're checking shortly after dispatch; if another sidecar dispatch for the SAME tag raced yours,
  # resolve by createdAt too rather than trusting "most recent" alone.
  # CPE-1918: the match is done in PowerShell, NOT in a `--jq` filter. Windows PowerShell 5.1 strips
  # `"` when marshalling an argument to a native exe's argv, and BOTH escapes people reach for fail
  # (`'…"x"…'` arrives unquoted; `"…\"x\"…"` arrives corrupted as `" x\)`), so any `--jq` selector
  # carrying a string literal is broken here. `-ceq` is exact AND case-sensitive, which is what jq's
  # `==` was doing and what the decoy-tag reasoning above requires. See RELEASING.md, "PowerShell and
  # `gh --jq`", before rewriting this.
  $runs = gh run list --repo StewartScottRogers/cross-platform-explorer --workflow=$workflow `
    --json databaseId,displayTitle | ConvertFrom-Json
  $runId = ($runs | Where-Object { $_.displayTitle -ceq "Release (sidecar) <TAG>" } |
    Select-Object -First 1).databaseId
} else {
  $workflow = "release.yml"
  $jobName = "verify-published-manifest"
  # Same CPE-1918 rule, and the exact match matters just as much here: `release.yml` runs exist for
  # BOTH `v0.57.69` and `v0.57.69-sidecar`, and the former is a substring of the latter.
  $runs = gh run list --repo StewartScottRogers/cross-platform-explorer --workflow=$workflow `
    --json databaseId,headBranch | ConvertFrom-Json
  $runId = ($runs | Where-Object { $_.headBranch -ceq "<TAG>" } | Select-Object -First 1).databaseId
}
if (-not $runId) { throw "no $workflow run found for tag <TAG> -- do not publish" }
# This is fail-closed and correct, not a broken release, if it's the SIDECAR branch above: every
# sidecar run dispatched before `release-sidecar.yml` gained its `run-name:` (CPE-1908) has
# `displayTitle` equal to the plain workflow name, not "Release (sidecar) <TAG>", so it can never
# match here and this throws for every such pre-existing draft. If you hit this on a draft that
# predates `run-name:`, don't read it as broken: dispatch a fresh `release-sidecar.yml` run for the
# tag (so its `displayTitle` carries the tag), or verify the job by hand per RELEASING.md instead.

# CPE-1918 again: `--jq` only plucks the sub-tree (no `"` in the filter); the name match is
# PowerShell's. `$jobs` is assigned BEFORE being piped on purpose -- in PS 5.1 `ConvertFrom-Json`
# emits a JSON array as ONE pipeline object, so `… | ConvertFrom-Json | Where-Object { $_.name -ceq …}`
# compares the whole array, finds the comparison truthy, and lets EVERY job through the filter.
$jobs = gh run view $runId --repo StewartScottRogers/cross-platform-explorer --json jobs `
  --jq '.jobs' | ConvertFrom-Json
$verifyJob = $jobs | Where-Object { $_.name -ceq $jobName }
if (-not $verifyJob) { throw "no $jobName job found on run $runId -- do not publish" }

# ONLY `success` may proceed to 1c. Anything else -- `failure`, `cancelled`, `skipped`, or the job
# missing entirely -- means STOP.
#
# An earlier draft of this check also accepted `skipped`, on the reasoning that a missing
# TAURI_SIGNING_PRIVATE_KEY makes the job skip itself the way the OS-code-signing steps do. The
# round-3 security audit showed that reasoning is wrong twice over, and the second way is dangerous:
#
#   1. The signing-key guard is at STEP level (`if: steps.sig.outputs.has == 'true'`), not job level.
#      With no key the job still runs checkout/toolchain/cache/detect, its two gated steps skip, and
#      the JOB conclusion is `success` -- never `skipped`.
#   2. So the only way this job reports `skipped` is its job-level `if: ${{ !cancelled() }}` being
#      false -- i.e. THE RUN WAS CANCELLED. And a run cancelled mid-matrix is precisely the case
#      where completed legs have already uploaded installers and a merged latest.json to the draft
#      while the verify gate never ran. Accepting `skipped` therefore let the publish through in the
#      exact scenario the gate exists to catch. Same reasoning applies to both channels' jobs.
if ($verifyJob.conclusion -ne "success") {
  throw "$jobName did not pass (conclusion: $($verifyJob.conclusion)) -- STOP, do not publish this draft"
}
"$jobName ($workflow): $($verifyJob.conclusion) -- OK to publish"
```

If this check throws, STOP — report it plainly and do NOT run 1c. This is exactly the gap CPE-1872's
round-3 security audit found for the plain channel (and CPE-1908 closed for the sidecar channel, which
this step now actually reaches most of the time): a partial matrix failure could leave a
fully-populated, unverified draft, and nothing upstream of this step would have caught it.

**1c. Publish the draft:**
```powershell
gh release edit <TAG> --repo StewartScottRogers/cross-platform-explorer --draft=false
```

Confirm it is now public:
```powershell
gh release view <TAG> --repo StewartScottRogers/cross-platform-explorer --json tagName,isDraft
```

Report: "Published <TAG>." — then continue.

**1d. If the latest release was already published**, still verify it carries an installer for THIS
OS before continuing. A release can be published while some platform assets are **still uploading** —
"the release exists" is NOT the same as "my platform's installer exists" (this is exactly what bit
CPE-024). If the installer for this OS is missing:

- Check `gh run list` for an in-progress Release build. If one is running, say so and offer to wait.
- Do NOT proceed to download.

Note the `tagName` and asset names for the next step.

---

## Step 2 — Pick the Right Asset for This OS

Detect the platform, then match the asset:

| OS | Preferred asset pattern | Fallback |
|----|------------------------|----------|
| Windows | `*_x64-setup.exe` (NSIS) | `*_x64_en-US.msi` |
| macOS | `*_universal.dmg` | `*_x64.dmg` / `*_aarch64.dmg` |
| Linux | `*_amd64.AppImage` | `*_amd64.deb` |

If no asset matches the current OS, say so plainly and stop — do not install the wrong artifact.

---

## Step 3 — Download

Clear the directory first — a stale installer from a previous version left lying around is how
CPE-024 ended up launching the wrong build.

```powershell
$tmp = Join-Path $env:TEMP "cpe-install"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
Remove-Item "$tmp\*" -Force -ErrorAction SilentlyContinue

gh release download <TAG> --repo StewartScottRogers/cross-platform-explorer --pattern "<PATTERN>" --dir $tmp --clobber
if ($LASTEXITCODE -ne 0) { throw "download failed (exit $LASTEXITCODE)" }

$installer = Get-ChildItem "$tmp\*-setup.exe" | Select-Object -First 1
if (-not $installer) { throw "no installer found after download — aborting" }
"downloaded: $($installer.Name)"
```

**Both guards are mandatory.** Never hand a possibly-null path to `Start-Process`: it exits with an
empty exit code, which reads as success, and the flow marches on to launch whatever was already
installed. Report the file name and size.

---

## Step 4 — Install

Always install **silently**, and always report the exit code.

**Windows (NSIS `.exe`):**

**CRITICAL — kill ALL processes first (CPE-483).** Before installing, terminate every
`cross-platform-explorer` AND `ai-console` process, **including any `ai-console --session-daemon`**
(these survive the app by design and hold `sidecars\ai-console.exe` **file-locked**). If you skip
this, NSIS **silently skips replacing the locked sidecar** — you get a new host running a STALE
sidecar, and the registry `DisplayVersion` lies (it only reflects the host exe). Also wipe the daemon
port dir.
```powershell
Get-Process | Where-Object { $_.ProcessName -match 'ai-console|cross-platform' } | Stop-Process -Force -EA SilentlyContinue
Start-Sleep -Seconds 2
Remove-Item (Join-Path $env:TEMP 'cpe-ai-console') -Recurse -Force -EA SilentlyContinue
$installer = Get-ChildItem "$env:TEMP\cpe-install\*-setup.exe" | Select-Object -First 1
$p = Start-Process -FilePath $installer.FullName -ArgumentList "/S" -Wait -PassThru
"exit code: $($p.ExitCode)"
```
After install, VERIFY the sidecar actually updated (not just the host): the timestamps must match.
```powershell
$d = "$env:LOCALAPPDATA\Cross-Platform Explorer (Sidecar)"
Get-Item "$d\cross-platform-explorer.exe","$d\sidecars\ai-console.exe" | Select-Object Name, LastWriteTime
```
A sidecar `LastWriteTime` lagging the host means it was locked and NOT replaced — kill processes and reinstall.

**Windows (MSI fallback):**
```powershell
$msi = Get-ChildItem "$env:TEMP\cpe-install\*.msi" | Select-Object -First 1
$p = Start-Process msiexec.exe -ArgumentList "/i `"$($msi.FullName)`" /quiet /norestart" -Wait -PassThru
"exit code: $($p.ExitCode)"
```

**macOS (`.dmg`):**
```bash
hdiutil attach -nobrowse -quiet "<dmg>"
cp -R "/Volumes/Cross-Platform Explorer/Cross-Platform Explorer.app" /Applications/
hdiutil detach -quiet "/Volumes/Cross-Platform Explorer"
xattr -dr com.apple.quarantine "/Applications/Cross-Platform Explorer.app"   # unsigned build (see CPE-002)
```

**Linux (`.AppImage`):**
```bash
mkdir -p ~/.local/bin
mv "<AppImage>" ~/.local/bin/cross-platform-explorer.AppImage
chmod +x ~/.local/bin/cross-platform-explorer.AppImage
```

A non-zero exit code is a FAILURE — report it, do not claim success, and do not launch.

**Note:** the app is not yet OS-code-signed (CPE-002 is Blocked on certificates), so Windows
SmartScreen or macOS Gatekeeper may warn. Tell the user this is expected rather than treating it as
a bug.

---

## Step 5 — Verify the Install

Confirm the app is actually registered before claiming success.

**Windows:**
```powershell
Get-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*,
                 HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\* -EA SilentlyContinue |
  Where-Object { $_.DisplayName -like "*Cross-Platform Explorer*" } |
  Select-Object DisplayName, DisplayVersion, InstallLocation, UninstallString
```

If nothing is found, the install did NOT succeed — report that honestly.

**Assert the version matches the release you just downloaded.** This is the check that would have
caught CPE-024:

```powershell
$v = (Get-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\* -EA SilentlyContinue |
      Where-Object { $_.DisplayName -like "*Cross-Platform Explorer*" }).DisplayVersion
if ($v -ne "<VERSION>") { throw "version mismatch: installed $v, expected <VERSION>" }
```

A mismatch means the install silently no-opped and the old build is still there. Fail — do not launch.

---

## Step 6 — Launch

**Windows:** run the executable from the `InstallLocation` found in Step 5.

Do NOT assume the exe is named after `productName`. Tauri names the Windows binary after the **Cargo
package name**, so it is `cross-platform-explorer.exe`, not `Cross-Platform Explorer.exe`. Glob for
it and exclude the uninstaller:

```powershell
$dir = "<InstallLocation>"   # from Step 5, strip surrounding quotes
$exe = Get-ChildItem "$dir\*.exe" | Where-Object { $_.Name -ne "uninstall.exe" } | Select-Object -First 1
Start-Process $exe.FullName
```
**macOS:** `open -a "Cross-Platform Explorer"`
**Linux:** `~/.local/bin/cross-platform-explorer.AppImage &`

Confirm it is actually up — a started process is not proof the app rendered:
```powershell
Start-Sleep -Seconds 4
Get-Process | Where-Object { $_.ProcessName -like "*cross-platform*" } |
  Select-Object Id, MainWindowTitle, Responding
```
A live process **with a MainWindowTitle** and `Responding = True` is success. A process that exited,
or one with an empty window title, is a FAILURE — report it rather than claiming the app launched.

Report: "Installed Cross-Platform Explorer <version> and launched it."

---

## Step 7 — Render the Action Menu

**Installed successfully** — HORIZONTAL:
```
┌─ App Running ────────────────────┐
│  [1] Remove  [2] Reinstall       │
├──────────────────────────────────┤
│  [3] Dismiss                     │
└──────────────────────────────────┘
```

**No release exists at all** — HORIZONTAL:
```
┌─ No Release ─────────────────────┐
│  [1] Cut release  [2] Tasks      │
├──────────────────────────────────┤
│  [3] Dismiss                     │
└──────────────────────────────────┘
```

**Draft has no installer assets (build failed / still running)** — HORIZONTAL:
```
┌─ Build Incomplete ───────────────┐
│  [1] Watch CI  [2] Detail        │
├──────────────────────────────────┤
│  [3] Dismiss                     │
└──────────────────────────────────┘
```

**Install failed** — HORIZONTAL:
```
┌─ Install Failed ─────────────────┐
│  [1] Retry  [2] Detail           │
├──────────────────────────────────┤
│  [3] Dismiss                     │
└──────────────────────────────────┘
```

---

## Actions

### [1] Remove  *(installed)*
Invoke /remove — uninstalls the application.

### [2] Reinstall  *(installed)*
Invoke /remove, then re-run this skill from Step 1.

### [1] Cut release  *(no release)*
Ask for a version, then run `./scripts/release.ps1 -Version X.Y.Z` and follow RELEASING.md.

### [2] Tasks  *(no release)*
Invoke /ticketing-list.

### [1] Watch CI  *(build incomplete)*
Run `gh run watch` to follow the release build; when it goes green, re-run this skill from Step 1.

### [2] Detail  *(build incomplete)*
Show `gh run view --log-failed` for the release run and list the assets currently on the draft.

### [1] Retry  *(failed)*
Re-run from Step 3.

### [2] Detail  *(failed)*
Show the full installer output, exit code, and the downloaded file path.

### [3] Dismiss
Exit without action.

---

## Menu Extension Point

Follows menu-render.md rules. To add an option, add it to the relevant rendered menu block, add its
action handler, and update the changelog in menu-render.md.
