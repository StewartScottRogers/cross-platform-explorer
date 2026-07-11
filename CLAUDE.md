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

## Managing this project — two surfaces

This repo is managed from **both** the Claude Code CLI and the Claude desktop (Cowork) app.
Both operate on the same files, so either can be used interchangeably.

### CLI (Claude Code)

Launch it by double-clicking **`RunClaude.cmd`** in the repo root (or run `claude` in this
directory). That starts a Claude Code session scoped to this repo with the slash commands in
`.claude/commands/` available:

| Command | Purpose |
|---------|---------|
| `/ticketing-list` | List the open ticket queue with an action menu |
| `/ticketing-new` | File a ticket interactively (auto-intercepts units of work) |
| `/ticketing-work CPE-NNN` | Pick up and work a ticket through to Done |
| `/ticketing-organize` | Reorganise `Done/` when it grows large |
| `/ticketing-setup` | (Re)bootstrap the ticket system |
| `/skills-organise` | Manage the slash commands as named feature sets |

`RunClaude.cmd` passes `--dangerously-skip-permissions` for an uninterrupted local session; it is
path-independent (`%~dp0`) so it works wherever the repo lives.

### Desktop (Cowork)

The desktop app manages releases and monitoring:

- **`RELEASING.md`** — runbook; say "cut a release 0.2.0", "check the build", "what needs updating".
- **`scripts/release.ps1`** — one-command version bump + tag + push.
- **`STATUS.html`** — local dashboard (gitignored), refreshed by a scheduled task.
- **Scheduled tasks** — `cpe-daily-status` (CI + dashboard refresh + notify) and
  `cpe-weekly-deps` (dependency scan).

### Using both together

The ticket system (`Tickets/`, `.claude/commands/`) is committed to git, so tickets filed from the
CLI are visible on the desktop and vice-versa. Release/monitoring lives on the desktop; day-to-day
coding and ticket work happens in the CLI. Nothing is surface-specific except the desktop-only
scheduled tasks and the `gh`-driven release helpers (which also work from a CLI PowerShell session).

## Ticket System

Tickets live in `Tickets/`. Folder location is the authoritative status:

| Folder | Status |
|--------|--------|
| `Tickets/Backlog/` | Open — ready to work |
| `Tickets/Doing/`   | In Progress — one at a time |
| `Tickets/Blocked/` | Deferred on an external gate |
| `Tickets/Done/`    | Closed |

IDs are sequential: `CPE-NNN`. To work a ticket: `/ticketing-work CPE-NNN`. To file one
interactively: `/ticketing-new`. See `Tickets/wiki.md` for full workflow rules.

### Showing open tickets — ALWAYS include Blocked

When the user asks to see "open tickets", "the tickets", or "tasks", ALWAYS show **two** tables —
never just the Backlog:

1. **Open** — all `Tickets/Backlog/CPE-*.md`, as a table of ID, title, type, priority, estimate.
2. **Blocked** — all `Tickets/Blocked/CPE-*.md`, as a table of ID, title, and a one-line
   *blocked-on / unblocks-when* note read from the ticket's Notes or Work Log.

Blocked tickets are outstanding work, so omitting them misrepresents the queue. If `Blocked/` is
empty, say "none blocked" rather than dropping the section. Also surface anything sitting in
`Tickets/Doing/` so stalled work-in-progress is never silently lost.
