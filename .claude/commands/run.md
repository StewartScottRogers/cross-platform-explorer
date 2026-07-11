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

**1c. Publish the draft:**
```powershell
gh release edit <TAG> --repo StewartScottRogers/cross-platform-explorer --draft=false
```

Confirm it is now public:
```powershell
gh release view <TAG> --repo StewartScottRogers/cross-platform-explorer --json tagName,isDraft
```

Report: "Published <TAG>." — then continue.

**1d. If the latest release was already published**, say so in one line and continue.

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

```powershell
$tmp = Join-Path $env:TEMP "cpe-install"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
gh release download <TAG> --repo StewartScottRogers/cross-platform-explorer --pattern "<PATTERN>" --dir $tmp --clobber
```

Report the file name and size.

---

## Step 4 — Install

Always install **silently**, and always report the exit code.

**Windows (NSIS `.exe`):**
```powershell
$installer = Get-ChildItem "$env:TEMP\cpe-install\*-setup.exe" | Select-Object -First 1
$p = Start-Process -FilePath $installer.FullName -ArgumentList "/S" -Wait -PassThru
"exit code: $($p.ExitCode)"
```

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

---

## Step 6 — Launch

**Windows:** run the executable from the `InstallLocation` found in Step 5.
```powershell
Start-Process "<InstallLocation>\Cross-Platform Explorer.exe"
```
**macOS:** `open -a "Cross-Platform Explorer"`
**Linux:** `~/.local/bin/cross-platform-explorer.AppImage &`

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
