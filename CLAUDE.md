# CLAUDE.md

Guidance for AI assistants (and humans) maintaining this repository.

## Purpose (read first)

See [PURPOSE.md](PURPOSE.md) for the app's guiding purpose statement and its
design tiebreaker. This app is a general cross-platform file explorer.

Modes are additive views layered over the explorer.
[AGENT-WATCH.md](AGENT-WATCH.md) describes the planned Agent Watch mode — a live
view of an AI coding agent's filesystem activity.

**Precedence:** inside Agent Watch, visibility outranks the explorer's
fast/small/predictable tiebreaker. If showing what the agent is doing costs
speed, size, or simplicity, pay the cost. The single hard constraint: with the
mode switched off, the plain explorer must remain fast, small, and predictable.

## What this is

A Tauri v2 desktop file explorer. Frontend is Svelte + TypeScript in `src/`.
Backend is Rust in `src-tauri/`. The app auto-updates via the Tauri updater
plugin, and CI builds/signs releases through GitHub Actions.

## Common commands

- `npm install` â€” install frontend deps
- `npm run tauri dev` â€” run the app with hot reload
- `npm run tauri build` â€” build local installers
- `npm run check` â€” type-check Svelte + TS
- `npm run tauri icon <png>` â€” regenerate app icons

## How the pieces connect

- The frontend calls Rust via `invoke("command_name", args)`.
- Rust commands live in `src-tauri/src/lib.rs`, annotated with `#[tauri::command]`
  and registered in the `generate_handler!` macro inside `run()`.
- **Adding a backend command:** write the `#[tauri::command]` fn, add it to
  `generate_handler![]`, then call it from Svelte with `invoke`.
- **Permissions:** any plugin capability the frontend uses must be listed in
  `src-tauri/capabilities/default.json`, or the call is denied at runtime.

## Versioning â€” keep three files in sync

When releasing, bump the version in ALL of:

1. `package.json`
2. `src-tauri/Cargo.toml`
3. `src-tauri/tauri.conf.json`

Then tag `vX.Y.Z` and push â€” CI does the rest.

## Guardrails

- Never commit signing keys (`updater.key`, `*.key`, `.env`). See `.gitignore`.
- The updater `pubkey` and `endpoints` in `tauri.conf.json` must be filled in for
  auto-updates to work (see README "Auto-updates").
- Filesystem commands skip entries they can't read rather than failing the whole
  listing â€” preserve that behavior when editing `list_dir`.

## Docs

- Tauri v2: https://v2.tauri.app
- Updater plugin: https://v2.tauri.app/plugin/updater/
- tauri-action: https://github.com/tauri-apps/tauri-action

## Managing this project â€” two surfaces

This repo is managed from **both** the Claude Code CLI and the Claude desktop (Cowork) app.
Both operate on the same files, so either can be used interchangeably.

### CLI (Claude Code)

Launch it by double-clicking **`RunClaude.cmd`** in the repo root (or run `claude` in this
directory). That starts a Claude Code session scoped to this repo with the slash commands in
`.claude/commands/` available:

| Command | Purpose |
|---------|---------|
| `/ticketing-list` | List the open ticket queue with an action menu |
| `/ticketing-new` | File a ticket interactively (auto-intercepts units of work; routes epics to the Epics queue) |
| `/ticketing-work CPE-NNN` | Pick up and work a ticket through to Done (redirects epics to `/ticketing-epic`) |
| `/ticketing-epic` | Manage epics — `list` / `activate CPE-NNN` / `close CPE-NNN`; decomposes an epic just-in-time |
| `/ticketing-organize` | Reorganise `Done/` when it grows large |
| `/ticketing-setup` | (Re)bootstrap the ticket system |
| `/skills-organise` | Manage the slash commands as named feature sets |
| `/run` | Publish the latest release (if draft), then install and launch it |
| `/remove` | Uninstall the application from this machine |

### Trigger words: "Run" and "Remove"

When the user says **"Run"**, execute `.claude/commands/run.md`:

1. Find the **latest** release, drafts included.
2. If it is still a draft, **publish it first** (`gh release edit <tag> --draft=false`) â€” but only
   after confirming the draft actually carries installer assets. A draft with no assets means the
   release build failed or is still running; publishing it would create an empty public release.
   In that case stop and report, rather than publishing.
3. Download the right installer for the current OS, install silently, verify the install, launch.

If **no release exists at all**, `/run` stops and says so â€” it never installs nothing and calls it
success.

When the user says **"Remove"**, execute `.claude/commands/remove.md` â€” close the app, uninstall it
silently, and verify it is gone. "Remove" means uninstall the **installed application**, never the
source repo or the user's files; if that is ambiguous in context, ask first.

Both commands act on the built app â€” they never touch this working tree.

`RunClaude.cmd` passes `--dangerously-skip-permissions` for an uninterrupted local session; it is
path-independent (`%~dp0`) so it works wherever the repo lives.

### Desktop (Cowork)

The desktop app manages releases and monitoring:

- **`RELEASING.md`** â€” runbook; say "cut a release 0.2.0", "check the build", "what needs updating".
- **`scripts/release.ps1`** â€” one-command version bump + tag + push.
- **`STATUS.html`** â€” local dashboard (gitignored), refreshed by a scheduled task.
- **Scheduled tasks** â€” `cpe-daily-status` (CI + dashboard refresh + notify) and
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
| `Tickets/Epics/`   | Umbrella trackers — a **separate queue**, decomposed just-in-time (`Proposed` = dormant brief, `In Progress` = activated) |
| `Tickets/Backlog/` | Open â€” ready to work |
| `Tickets/Doing/`   | In Progress â€” one at a time |
| `Tickets/Blocked/` | Deferred on an **external** gate — not workable until it clears |
| `Tickets/Deferred/`| Postponed by **our** choice / an internal prereq — pickable anytime |
| `Tickets/Done/`    | Closed |

IDs are sequential: `CPE-NNN`. To work a ticket: `/ticketing-work CPE-NNN`. To file one
interactively: `/ticketing-new`. See `Tickets/wiki.md` for full workflow rules.

**Epics** are handled specially: they live in `Tickets/Epics/` and are **not** researched, planned, or
sub-ticketed until *activated* with `/ticketing-epic activate CPE-NNN`. A dormant epic is just a brief;
`/ticketing-work` never builds one directly. See `Tickets/wiki.md` → "Epics" and the `ticketing-epic` skill.

### Showing open tickets â€” ALWAYS include Blocked and Deferred

When the user asks to see "open tickets", "the tickets", "tasks", or "all tickets", ALWAYS show the
Backlog table **plus** the Blocked and Deferred tables — never just the Backlog:

1. **Open** â€” all `Tickets/Backlog/CPE-*.md`, as a table of ID, title, type, priority, tags, estimate.
   `tags` is the ticket's disposition (`ready`, `big-design`, `resource-blocked` + qualifier, etc.);
   the controlled vocabulary lives in `Tickets/wiki.md` ("Disposition Tags").
2. **Blocked** — all `Tickets/Blocked/CPE-*.md`, as a table of ID, title, tags, and a one-line
   *blocked-on / unblocks-when* note read from the ticket's Notes or Work Log.
3. **Deferred** — all `Tickets/Deferred/CPE-*.md`, as a table of ID, title, tags, and a one-line
   *deferred-on / revisit-when* note. These are postponed by our choice (often an internal prereq),
   not externally gated, so they remain pickable.
4. **Epics** — all `Tickets/Epics/CPE-*.md`, as a table of ID, title, status (`Proposed`/`In Progress`),
   tags, and a one-line goal (plus `X of Y children Done` for an activated epic). This is the separate
   epic queue; epics are decomposed via `/ticketing-epic`, not worked by `/ticketing-work`.

Blocked, Deferred, and Epic tickets are all outstanding work, so omitting them misrepresents the queue.
If a section is empty, say "none blocked" / "none deferred" / "no epics" rather than dropping it. Also
surface anything sitting in `Tickets/Doing/` so stalled work-in-progress is never silently lost.
