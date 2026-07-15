# Setup AI-Managed Ticket System

Bootstrap the Claude Code ticket management system in the current project.
Fully automated after a single approval step. Safe to run on a project that already has a
partial setup — existing tickets are never touched. This project is a Tauri + Svelte app
tracked with git and GitHub; there is no Visual Studio / MSBuild integration to maintain,
so tickets are plain markdown files versioned in git.

---

## Step 1 — Inspect the Project and Propose Values

Before presenting anything to the user, read the project silently:

- **PROJECT** — `name` in `package.json`, else `productName` in `src-tauri/tauri.conf.json`,
  else the README H1, else the working directory name.
- **APP** — the human product name (`productName` in `src-tauri/tauri.conf.json`).
- **PREFIX** — if `Tickets/` already exists with `{XX}-NNN` files, reuse that prefix.
  Otherwise derive initials from PROJECT (`cross-platform-explorer` -> `CPE`). Two-to-four capitals.
- **COMPONENTS** — the architectural areas of the app. Default for this project:
  `Frontend | Backend | Updater | CI | Packaging | Docs | Multiple`
  (Frontend = Svelte `src/`, Backend = Rust `src-tauri/`, Updater = auto-update pipeline,
  CI = GitHub Actions, Packaging = installers/bundling, Docs = README/CLAUDE/RELEASING).
- **CHECK_CMD** — `npm run check` (svelte-check + tsc).
- **BUILD_CMD** — `npm run build` for the frontend; note `npm run tauri build` needs Rust locally
  (CI builds the full app).

Present all values in one block and wait for approval before creating anything:

```
I inspected the project and propose these settings — reply "ok" to accept all,
or correct any line before I proceed:

  PROJECT    = cross-platform-explorer
  APP        = Cross-Platform Explorer
  PREFIX     = CPE   (ticket IDs will be CPE-001, CPE-002, …)
  COMPONENTS = Frontend | Backend | Updater | CI | Packaging | Docs | Multiple
  CHECK_CMD  = npm run check
  BUILD_CMD  = npm run build
```

Do not create any files until the user approves or corrections are confirmed.

---

## Step 2 — Create Ticket Folder Structure

Skip any folder or file that already exists. Create a `wiki.md` in each subfolder explaining
its purpose, rules, and conventions.

```
Tickets/
  Backlog/wiki.md  <- purpose, how to file, priority guide, how to invoke ticketing-work
  Doing/wiki.md    <- one-at-a-time rule, what the agent updates, how to pause/resume
  Blocked/wiki.md  <- EXTERNAL-gate tickets, what a blocked note must contain
  Deferred/wiki.md <- OUR-choice/internal-prereq postponements (pickable); Blocked-vs-Deferred rule
  Done/wiki.md     <- terminal statuses, what a closed ticket contains, how to reopen
```

---

## Step 3 — Write Tickets/wiki.md

Write the workflow rules (substitute PROJECT / APP / PREFIX / COMPONENTS). Cover: purpose,
folder structure (folder = status), ID scheme (`{PREFIX}-NNN`, sequential, zero-padded),
file naming (`{PREFIX}-NNN_slug.md`, never renamed on move), frontmatter schema, the type and
priority tables, the status lifecycle (Backlog -> Doing -> Done, with Blocked as an external-gate
side state and Deferred as an our-choice/internal-prereq side state), the body sections table, and
the Work Log format. Include a
"When to Auto-File a Ticket" section defining what units of work get intercepted by ticketing-new.

Do NOT include any MSBuild / `.projitems` / Visual Studio instructions — this project has none.

---

## Step 4 — Write Tickets/_template.md

Write a ticket template with the frontmatter schema and the standard body sections
(Summary, Environment, Steps to Reproduce, Expected, Actual, Acceptance Criteria, Resolution,
Work Log, Notes) with guidance comments for which sections apply to which types.

---

## Step 5 — Update CLAUDE.md

Find `CLAUDE.md` in the project root (create if missing). If a "Ticket System" section exists,
replace it; otherwise append:

```markdown
## Ticket System

Tickets live in `Tickets/`. Folder location is the authoritative status:

| Folder | Status |
|--------|--------|
| `Tickets/Backlog/` | Open — ready to work |
| `Tickets/Doing/`   | In Progress — one at a time |
| `Tickets/Blocked/` | Deferred on an **external** gate — not workable until it clears |
| `Tickets/Deferred/`| Postponed by **our** choice / an internal prereq — pickable anytime |
| `Tickets/Done/`    | Closed |

IDs are sequential: `CPE-NNN`. To work a ticket: `/ticketing-work CPE-NNN`.
To file one interactively: `/ticketing-new`. When the user says "tasks", list all
`Tickets/Backlog/CPE-*.md` as a table of ID, title, type, and priority.
```

---

## Step 6 — Install Claude Code Skills

Ensure these files exist in `.claude/commands/` (create any that are missing):
`menu-render.md`, `ticketing-list.md`, `ticketing-new.md`, `ticketing-work.md`,
`ticketing-organize.md`, `ticketing-setup.md`, `skills-organise.md`.

---

## Step 7 — Final Report

Print a concise summary of what was created, then list the available commands:

```
Ticket system installed for cross-platform-explorer.

Commands available now:
  /ticketing-list         — list open tickets
  /ticketing-new          — file a ticket interactively
  /ticketing-work CPE-NNN — pick up and work a ticket
  /ticketing-organize     — reorganise Done/ when it grows large
  /skills-organise        — manage skill feature sets
```
