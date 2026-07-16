# Releasing & maintaining (desktop runbook)

This is the operating manual for managing **cross-platform-explorer** from the
Cowork desktop app. In a desktop session you can just say what you want in plain
language ("cut a release 0.2.0", "check the build", "what needs updating") and
Claude follows the steps below. The `gh` CLI on this machine is already
authenticated as `StewartScottRogers`.

## Cut a new release

A release is triggered by pushing a `vX.Y.Z` tag. The version must match in
three files, so use the helper script — it edits all three, commits, tags, and
pushes in one go:

```powershell
cd Z:\repos\cross-platform-explorer
./scripts/release.ps1 -Version 0.2.0
```

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

## Check build / CI status

```powershell
cd Z:\repos\cross-platform-explorer
gh run list --limit 5
gh run watch          # live-follow the most recent run
gh run view --log-failed   # show logs of failed steps
```

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

## Status dashboard

`STATUS.html` (gitignored) is a local dashboard refreshed by the scheduled task
`cpe-daily-status`. Open it any time; run the task manually to refresh on demand.
