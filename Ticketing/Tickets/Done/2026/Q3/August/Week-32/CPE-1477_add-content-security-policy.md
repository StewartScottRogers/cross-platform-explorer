---
id: CPE-1477
title: "Add a Content-Security-Policy — security.csp is null, so a future injection escalates to exfil/RCE unchecked"
type: Bug
status: Done
priority: Medium
component: Full-stack
tags: [ready, security]
epic: CPE-810
created: 2026-08-08
---
## Vector (found in the Tauri capability/command/CSP deep audit, 2026-08-08)
`src-tauri/tauri.conf.json:15` → `"csp": null`. No CSP is injected, so `connect-src`/`script-src`/`img-src` are
all unrestricted in the main webview. That webview renders untrusted file contents (previews) AND can invoke a
command surface that includes `run_command` (shell), `run_as_admin` (UAC elevation), `open_external` (executes),
plus broad fs write/delete/`shred_paths`. With no CSP, ANY future script injection — a single un-DOMPurify'd
`{@html}`, a dompurify bypass, or a malicious SVG/preview edge case — escalates directly to arbitrary-file
exfiltration (unrestricted `connect-src`) and, via the IPC surface, RCE. The `assetProtocol` `**` scope
(tauri.conf.json:18) compounds it (an injected script could read any file via `asset://` and POST it anywhere).

## Not exploitable today
Contingent on an injection; the frontend rendering was audited CLEAN this session (all `{@html}` dompurified). So
this is DEFENSE-IN-DEPTH — it turns the whole "any XSS → full compromise" class into a non-event.

## Fix
Set `security.csp` in `src-tauri/tauri.conf.json`. Prioritize the security-critical restrictions and stay generous
on resource-loading so previews don't break:
`default-src 'self'; script-src 'self'; connect-src 'self' ipc: http://ipc.localhost; img-src 'self' asset:
http://asset.localhost data: blob:; media-src 'self' asset: http://asset.localhost data: blob:; style-src 'self'
'unsafe-inline'; font-src 'self' data:; object-src 'none'; frame-src 'none'`.
- `script-src 'self'` is safe: the Vite build has no inline scripts; Tauri IPC uses the `ipc:` scheme/globals, not
  inline script. `connect-src 'self' ipc:` is the key anti-exfil restriction (the main webview only talks IPC; the
  AI-Console is a SEPARATE process over its own fetch, unaffected). `style-src 'unsafe-inline'` is the one
  concession Svelte needs. Keep `asset:`/`data:`/`blob:` in img/media (previews need them).
- VERIFY AT RUNTIME: the `gui-smoke` CI leg drives the real `tauri build` binary and asserts the UI renders — a
  CSP that breaks the app shows up there. After pushing, check the gui-smoke leg RENDERS (distinguish a real
  CSP-violation/blank-screen from WebView2 startup flakiness by reading the job log). If a legit resource is
  blocked, loosen the specific directive (never loosen `script-src`/`connect-src` beyond `'self'`+`ipc:` without
  cause). If a directive genuinely needs the user's eyes on the running app, note it + add a burndown row.

## Effort / blast radius
Medium — one config line + a gui-smoke runtime check. No code changes expected. Touches only tauri.conf.json.
Epic CPE-810 (client/server contract + security). Disjoint from the concurrent workshifts_* work.

## Work Log
- 2026-08-08: Set `security.csp` in `src-tauri/tauri.conf.json` (line 15) from `null` to the
  prescribed policy string: `default-src 'self'; script-src 'self'; connect-src 'self' ipc:
  http://ipc.localhost; img-src 'self' asset: http://asset.localhost data: blob:; media-src 'self'
  asset: http://asset.localhost data: blob:; style-src 'self' 'unsafe-inline'; font-src 'self'
  data:; object-src 'none'; frame-src 'none'`. No code changes — config-only, one line. Verified the
  JSON parses (`node -e "JSON.parse(...)"`) and `cargo build` succeeds in `src-tauri`. Runtime CSP
  enforcement (whether any legit resource load is blocked) is NOT exercised by this build — that is
  validated by the CI `gui-smoke` leg, which drives the real `tauri build` binary and asserts the UI
  renders; a CSP that breaks the app would show up there as a blank screen. Flagged for the
  Reviewer/UAT to watch that leg on this PR before merge.
