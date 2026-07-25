---
id: CPE-1043
title: "--open <dir> launch argument (open the explorer at a folder)"
type: feature
component: Multiple
priority: medium
status: Done
tags: ready
created: 2026-07-25
estimate: 2h
---

## Summary
A command-line argument that launches the app **opened at a specified directory** — e.g.
`cross-platform-explorer.exe --open "Z:\repos\cross-platform-explorer\samples"`. Lets a caller (a human,
a script, or an assistant pointing the user at a folder) start the explorer already in the right place. An
explicit `--open` folder takes precedence over last-session restore for that launch.

- New CLI arg `open` (declared in `tauri.conf.json` `plugins.cli.args`, alongside the geometry flags).
- Pure resolver `cpe_server::launch::resolve_open_dir(raw, is_dir)` — returns the trimmed path only when it
  names an existing directory (filesystem check injected, so it's unit-testable). Empty/missing → None.
- Command `startup_dir() -> Option<String>` — reads the `--open` CLI match, resolves it (absolutizing a relative value against CWD; no canonicalize, to avoid Windows \? prefixes),
  returns the folder (or None). Registered in both `generate_handler!` and `collect_commands!`; bindings
  regenerated.
- Frontend: at boot, if `startup_dir()` returns a folder, open it (overriding session restore); else the
  existing default (restore last session / Home). Guarded so the jsdom test env (no Tauri) is unaffected.

## Future (noted, not this ticket)
Extend to opening a **specific file** in a caller-specified viewer/editor
(`--open-with <file> --app <editor>`), so a resource can be surfaced in a chosen tool.

## Acceptance Criteria
- [ ] `resolve_open_dir` unit-tested: None/empty → None; existing dir → Some(trimmed); non-existing → None.
- [ ] Launching with `--open <dir>` opens the explorer at that folder; an invalid/missing value falls back
      to the normal startup (no crash).
- [ ] `startup_dir` registered in both macros + present in regenerated bindings; frontend wiring guarded
      for the non-Tauri test env.
- [ ] `cargo test -p cpe-server` + clippy clean both modes; `npm run check` + `npx vitest run` green.

## Work Log
2026-07-25 — Filed at the user's request (point the app at the just-created `samples/` folder). Builds on
the existing CPE-598/600 geometry CLI-args plumbing.

2026-07-25 (attended) — **DONE, merged PR #360.** `--open <dir>` launch arg: pure resolver
`cpe_server::launch::resolve_open_dir` + `open` CLI arg + `startup_dir` command (absolutizes a relative
value, no canonicalize) + Tauri-gated frontend startup wiring (open-arg > session restore > Home).
Independently reviewed (APPROVE; relative-path fix applied). 4 launch tests + 954 frontend tests green.
