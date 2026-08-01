---
id: CPE-1171
title: "gui-smoke Linux CI leg — WebKitWebDriver under xvfb (burns down Manual-Test-Burndown row #4)"
type: chore
component: Testing
priority: medium
status: Backlog
tags: ready
created: 2026-07-31
epic: CPE-579
---

## Summary
Manual-Test-Burndown row #4 ("Cross-OS GUI") was `_unfiled_`: the `gui-smoke` headless GUI harness
(CPE-1045) only ran on `windows-latest`, so Linux app *behaviour* (not just backend `cargo test`) was
never driven headlessly. `tauri-driver` supports Linux via `WebKitWebDriver`, so a second CI leg can
run the SAME WebdriverIO suite under `xvfb-run` with no app-code changes. macOS stays attended —
`tauri-driver` has no WKWebView WebDriver.

## Build
- Add a `gui-smoke-linux` job to `.github/workflows/gui-smoke.yml`: `runs-on: ubuntu-latest`, installs
  the Linux WebView build deps + `webkit2gtk-driver` + `xvfb`, builds the frontend, runs
  `tauri build -- --no-bundle`, installs `tauri-driver`, and runs the `gui-smoke` suite's
  `npm ci`/`npm run typecheck`/`npm test` under `xvfb-run`.
- Audit `gui-smoke/wdio.conf.ts` for OS-awareness of the driver/native-binary resolution; update stale
  comments that assumed Windows-only CI.
- Keep the leg `continue-on-error: true` (non-blocking), same posture as the existing Windows leg
  (CPE-1048).
- Update `gui-smoke/README.md` and `MANUAL-TEST-BURNDOWN.md` row #4.

## Acceptance Criteria
- [x] `.github/workflows/gui-smoke.yml` has a second, non-blocking `ubuntu-latest` job running the
      identical `gui-smoke` WebdriverIO suite under `xvfb-run`, installing the Linux WebView build
      deps + `webkit2gtk-driver` + `xvfb` + `tauri-driver`.
- [x] `gui-smoke/wdio.conf.ts` verified/annotated OS-aware for the app-binary + native-driver
      resolution; the Windows leg's behaviour is unchanged (diffed byte-for-byte against `main`).
- [x] `gui-smoke/README.md` "Follow-ups" and `MANUAL-TEST-BURNDOWN.md` row #4 updated.
- [ ] Linux leg runs green on GitHub Actions — **pending**, not verifiable from this offline worktree;
      to be confirmed by the Foreman after merge.

## Work Log
- 2026-07-31 — **Built.** Added the `gui-smoke-linux` job to `.github/workflows/gui-smoke.yml`:
  `ubuntu-latest`, `continue-on-error: true` (non-blocking, same posture as the Windows leg's
  CPE-1048 rationale — a brand-new WebKitGTK-under-Xvfb CI leg has no track record yet). Installs
  the same Linux build-dep set the 3-OS `Backend` job in `ci.yml` already proves
  (`libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`) plus
  `webkit2gtk-driver` (installs `WebKitWebDriver` at `/usr/bin`, already on `PATH`) and `xvfb`;
  builds frontend + `tauri build -- --no-bundle`; installs `tauri-driver`; runs
  `npm ci` / `npm run typecheck` / the smoke suite all under `xvfb-run --auto-servernum` (the whole
  command, not just the app, per a known WebKitGTK "GTK backend must init after a display exists"
  CI ordering trap). Added a defensive `WEBKIT_DISABLE_COMPOSITING_MODE=1` env var, the documented
  last-resort mitigation for WebKitGTK compositor crashes on GPU-less CI — unverified live, noted as
  best-effort.
- **`wdio.conf.ts` audit (no functional changes needed):** `APP_BINARY` and `TAURI_DRIVER_BIN` were
  already `process.platform`-branched before this ticket. Confirmed via `tauri-driver`'s own source
  (`crates/tauri-driver/src/webdriver.rs` / `server.rs`) that it independently resolves the native
  driver per OS — `which::which("msedgedriver.exe")` on Windows vs `which::which("WebKitWebDriver")`
  on Linux, both a plain `PATH` lookup, and builds `ms:edgeOptions` vs `webkitgtk:browserOptions`
  internally — so no capability branching was needed in this repo's `wdio.conf.ts`. The
  Windows-only `webviewOptions: {}` capability field is safe to leave unconditional: it's
  `#[cfg(target_os = "windows")]`-gated in `tauri-driver`'s own struct, so the Linux build simply
  doesn't have that struct field and silently ignores the unknown JSON key (no
  `deny_unknown_fields`). Updated two stale comments that assumed Windows-only CI (the
  `resolveAppDataDir` doc-comment, and the file header) and added audit-trail comments next to
  `APP_BINARY`/`TAURI_DRIVER_BIN` recording this finding for future readers.
- Updated `gui-smoke/README.md`: "CI" section now documents both legs and the exact Linux package
  list; "Prerequisites" #2 points at the Linux install command instead of "see Follow-ups"; the
  Linux-leg Follow-up item is struck through as Done, with an explicit note that it hasn't run live
  on Actions yet.
- Updated `MANUAL-TEST-BURNDOWN.md` row #4: ticket `CPE-1171`, status flipped from `⛰ manual` to
  `🔧 in progress` (not `✅` — the leg is real but unproven on live CI, and non-blocking by design
  like the Windows leg). macOS residual noted as still attended, no automation path.
- **Verify (local, offline):**
  - `cd gui-smoke && npm ci && npm run typecheck` → **green**, no errors.
  - YAML sanity-check via `js-yaml` (already a `gui-smoke` transitive dep, no new install): parsed
    `.github/workflows/gui-smoke.yml` successfully, confirmed both jobs (`gui-smoke`,
    `gui-smoke-linux`) present, `continue-on-error: true` on both, `runs-on: ubuntu-latest` on the
    new job, and its 12 steps in the intended order.
  - `git diff origin/main -- .github/workflows/gui-smoke.yml`: confirmed the existing Windows job's
    steps are **byte-for-byte unchanged** — the diff shows only the updated header comment block and
    a pure append of the new `gui-smoke-linux` job after the Windows job's last line.
  - **NOT verified:** the Linux leg has not executed on GitHub Actions from this worktree (no local
    Actions runner, offline task). This is a real gap in verification — the Foreman/next reviewer
    should watch the first `main`/PR run of `gui-smoke-linux` and confirm it's green (or triage if
    not) before treating Manual-Test-Burndown row #4 as more than "in progress."

## Notes
- Epic CPE-579 (manual-test burndown). macOS is intentionally out of scope — no `tauri-driver`
  WKWebView support exists; row #4's macOS half stays attended until that changes upstream.
- If the Linux leg proves flaky (WebKitGTK compositor crashes under Xvfb are a known class of issue),
  the next step is tightening `WEBKIT_DISABLE_COMPOSITING_MODE`/other WebKitGTK env knobs rather than
  reverting the job — keep it non-blocking and let it collect real signal.
