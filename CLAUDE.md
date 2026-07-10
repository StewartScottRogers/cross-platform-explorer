# CLAUDE.md

Guidance for AI assistants (and humans) maintaining this repository.

## What this is

A Tauri v2 desktop file explorer. Frontend is Svelte + TypeScript in `src/`.
Backend is Rust in `src-tauri/`. The app auto-updates via the Tauri updater
plugin, and CI builds/signs releases through GitHub Actions.

## Common commands

- `npm install` — install frontend deps
- `npm run tauri dev` — run the app with hot reload
- `npm run tauri build` — build local installers
- `npm run check` — type-check Svelte + TS
- `npm run tauri icon <png>` — regenerate app icons

## How the pieces connect

- The frontend calls Rust via `invoke("command_name", args)`.
- Rust commands live in `src-tauri/src/lib.rs`, annotated with `#[tauri::command]`
  and registered in the `generate_handler!` macro inside `run()`.
- **Adding a backend command:** write the `#[tauri::command]` fn, add it to
  `generate_handler![]`, then call it from Svelte with `invoke`.
- **Permissions:** any plugin capability the frontend uses must be listed in
  `src-tauri/capabilities/default.json`, or the call is denied at runtime.

## Versioning — keep three files in sync

When releasing, bump the version in ALL of:

1. `package.json`
2. `src-tauri/Cargo.toml`
3. `src-tauri/tauri.conf.json`

Then tag `vX.Y.Z` and push — CI does the rest.

## Guardrails

- Never commit signing keys (`updater.key`, `*.key`, `.env`). See `.gitignore`.
- The updater `pubkey` and `endpoints` in `tauri.conf.json` must be filled in for
  auto-updates to work (see README "Auto-updates").
- Filesystem commands skip entries they can't read rather than failing the whole
  listing — preserve that behavior when editing `list_dir`.

## Docs

- Tauri v2: https://v2.tauri.app
- Updater plugin: https://v2.tauri.app/plugin/updater/
- tauri-action: https://github.com/tauri-apps/tauri-action
