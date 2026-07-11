# Remove — Uninstall the Application

Uninstall Cross-Platform Explorer from this machine.
Triggered when the user says **"Remove"** (or `/remove`).

Then present an action menu following the rules in menu-render.md.

This removes the **installed application**. It does NOT touch the source repository, the git history,
or any of the user's own files. If the user's intent is ambiguous (e.g. they might mean deleting the
repo), ask before doing anything.

---

## Step 1 — Find the Installation

**Windows:**
```powershell
Get-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*,
                 HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\* -EA SilentlyContinue |
  Where-Object { $_.DisplayName -like "*Cross-Platform Explorer*" } |
  Select-Object DisplayName, DisplayVersion, UninstallString, InstallLocation
```

**macOS:** check for `/Applications/Cross-Platform Explorer.app`
**Linux:** check for `~/.local/bin/cross-platform-explorer.AppImage`, else `dpkg -l | grep cross-platform-explorer`

**If nothing is installed**, say so plainly — "Cross-Platform Explorer is not installed; nothing to
remove." — render the "Not Installed" menu, and stop. Do not invent an uninstall.

---

## Step 2 — Close the App if Running

The uninstaller will fail or leave files behind if the app is running.

**Windows:**
```powershell
Get-Process -Name "Cross-Platform Explorer" -EA SilentlyContinue | Stop-Process -Force
```
**macOS:** `osascript -e 'quit app "Cross-Platform Explorer"'`
**Linux:** `pkill -f cross-platform-explorer` (best effort)

---

## Step 3 — Uninstall

Always uninstall **silently**, and always report the exit code.

**Windows (NSIS):** the `UninstallString` points at `Uninstall.exe`. Run it with `/S`:
```powershell
$u = "<UninstallString>"   # e.g. C:\Users\...\Cross-Platform Explorer\uninstall.exe
$p = Start-Process -FilePath $u -ArgumentList "/S" -Wait -PassThru
"exit code: $($p.ExitCode)"
```

**Windows (MSI):** if the `UninstallString` is an `msiexec` call, use the product code:
```powershell
$p = Start-Process msiexec.exe -ArgumentList "/x {PRODUCT-CODE} /quiet /norestart" -Wait -PassThru
"exit code: $($p.ExitCode)"
```

**macOS:**
```bash
rm -rf "/Applications/Cross-Platform Explorer.app"
```

**Linux:**
```bash
rm -f ~/.local/bin/cross-platform-explorer.AppImage      # AppImage
sudo dpkg -r cross-platform-explorer                     # .deb install
```

A non-zero exit code is a FAILURE — report it and do not claim the app was removed.

---

## Step 4 — Verify Removal

Re-run the Step 1 detection. It must now return **nothing**.

If the registry entry (or `.app` bundle) is still present, the uninstall did not complete — say so,
and show what is left behind rather than reporting success.

---

## Step 5 — Offer to Clean Leftover User Data

Tauri apps keep per-user data outside the install directory; uninstalling does not remove it.
Report what exists and ASK before deleting — never delete user data unprompted.

- **Windows:** `%APPDATA%\com.example.crossplatformexplorer` and `%LOCALAPPDATA%\com.example.crossplatformexplorer`
- **macOS:** `~/Library/Application Support/com.example.crossplatformexplorer`
- **Linux:** `~/.local/share/com.example.crossplatformexplorer` and `~/.config/com.example.crossplatformexplorer`

Also remove the downloaded installer cache if present: `%TEMP%\cpe-install`.

---

## Step 6 — Render the Action Menu

**Removed successfully** — HORIZONTAL:
```
┌─ App Removed ────────────────────┐
│  [1] Reinstall  [2] Clean data   │
├──────────────────────────────────┤
│  [3] Dismiss                     │
└──────────────────────────────────┘
```

**Not installed** — HORIZONTAL:
```
┌─ Not Installed ──────┐
│  [1] Install         │
├──────────────────────┤
│  [2] Dismiss         │
└──────────────────────┘
```

**Removal failed** — HORIZONTAL:
```
┌─ Removal Failed ─────────────────┐
│  [1] Retry  [2] Detail           │
├──────────────────────────────────┤
│  [3] Dismiss                     │
└──────────────────────────────────┘
```

---

## Actions

### [1] Reinstall  *(removed)*
Invoke /run — downloads and installs the latest release again.

### [2] Clean data  *(removed)*
Delete the per-user data directories listed in Step 5, after confirming each with the user.

### [1] Install  *(not installed)*
Invoke /run.

### [1] Retry  *(failed)*
Re-run from Step 2 (close the app first — a running process is the most common cause).

### [2] Detail  *(failed)*
Show the uninstaller output, exit code, and everything still present on disk / in the registry.

### [2] / [3] Dismiss
Exit without action.

---

## Menu Extension Point

Follows menu-render.md rules. To add an option, add it to the relevant rendered menu block, add its
action handler, and update the changelog in menu-render.md.
