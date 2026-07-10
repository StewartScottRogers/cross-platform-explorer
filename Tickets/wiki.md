# cross-platform-explorer Ticket System — Wiki

## Purpose

Single source of truth for bugs, tasks, and feature requests for cross-platform-explorer
(the Cross-Platform Explorer app). Filed by the user, worked by the Claude Code agent using
the rules below. Tickets are plain markdown files versioned in git — there is no external
tracker or IDE project integration to keep in sync.

---

## Folder Structure

```
Tickets/
  wiki.md        <- workflow rules (you are here)
  _template.md   <- copy to Backlog/ to start a new ticket
  Backlog/       <- open tickets waiting to be worked
  Doing/         <- ticket the agent is currently working (one at a time)
  Blocked/       <- tickets deferred on an external gate
  Done/          <- closed tickets — never deleted
```

The folder a ticket lives in IS its status. The `status:` frontmatter field mirrors it.

---

## ID Scheme

Format: `CPE-NNN` (zero-padded three digits — `CPE-001`, `CPE-042`, `CPE-100`).
Sequential. To find the next ID: scan all folders for `CPE-*.md`, read the highest NNN, add 1.

## File Naming

`CPE-NNN_short-kebab-title.md` — the filename never changes when a ticket moves folders.

---

## Ticket Frontmatter

```yaml
---
id: CPE-NNN
title: Human-readable title (sentence case)
type: Bug | Defect | Task | Feature | Test
status: Open | In Progress | Blocked | Done | Won't Fix | Duplicate
priority: Low | Medium | High | Critical
component: Frontend | Backend | Updater | CI | Packaging | Docs | Multiple
estimate: 15m | 30m | 1h | 1-2h | 2-3h | 3-4h | 4h+
created: YYYY-MM-DD
closed: YYYY-MM-DD
---
```

### Components
| Component | Area |
|-----------|------|
| Frontend | Svelte UI in `src/` |
| Backend | Rust / Tauri commands in `src-tauri/` |
| Updater | auto-update pipeline (updater plugin, signing, latest.json) |
| CI | GitHub Actions workflows |
| Packaging | installers, bundling, icons |
| Docs | README, CLAUDE.md, RELEASING.md, website |
| Multiple | spans more than one of the above |

### Types
| Type | When to use |
|------|-------------|
| Bug | Worked before, now broken |
| Defect | Never worked correctly |
| Task | Implementation, refactor, cleanup, infrastructure |
| Feature | New capability |
| Test | Adding or fixing tests |

### Priority
| Priority | Meaning |
|----------|---------|
| Critical | App crashes, data loss, or release/updater pipeline fails |
| High | Core feature broken; workaround is painful or absent |
| Medium | Feature works but behaves incorrectly |
| Low | Cosmetic, minor inconvenience, nice-to-have |

---

## Status Lifecycle

```
Backlog/ (Open) -> Doing/ (In Progress) -> Done/ (Done | Won't Fix | Duplicate)
                        |
                        +-> Blocked/ (Blocked)  <- external gate; returns to Backlog/ when cleared
```

Only one ticket in Doing/ at a time under normal circumstances.
To reopen: move from Done/ back to Backlog/, set `status: Open`, add a Work Log note.

---

## Ticket Body Sections

| Section | Required | Who writes it |
|---------|----------|---------------|
| Summary | Always | User |
| Environment | Bugs/Defects | User |
| Steps to Reproduce | Bugs/Defects | User |
| Expected Behavior | Bugs/Defects | User |
| Actual Behavior | Bugs/Defects | User |
| Acceptance Criteria | Always | User |
| Resolution | On close | Agent |
| Work Log | Throughout | Agent (append-only) |
| Notes | Optional | Either |

**Work Log format** — one line per entry, appended throughout (not just at close):
```
YYYY-MM-DD — Short description of discovery, decision, or action.
```

---

## When to Auto-File a Ticket

`/ticketing-new` intercepts **units of project work** transparently: a feature, a bug/defect fix
(including small live fixes), a refactor, a behavior change, or any multi-file edit. It announces
in one line, files the ticket, then works it to Done.

Do NOT intercept (just do the thing): answering questions, analysis, running build / check / commit /
push / git ops, cutting or publishing a release, managing tickets or the skill system, trivial
one-liners being iterated live, or anything the user says to "just do." If borderline, ask first.
