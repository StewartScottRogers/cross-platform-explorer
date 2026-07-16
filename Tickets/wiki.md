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
  Epics/         <- umbrella trackers, decomposed just-in-time (SEPARATE queue — see "Epics" below)
  Backlog/       <- open tickets waiting to be worked
  Doing/         <- ticket the agent is currently working (one at a time)
  Blocked/       <- tickets deferred on an EXTERNAL gate (can't be worked until it clears)
  Deferred/      <- tickets WE postponed by choice / on an internal prereq (pickable anytime)
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
status: Proposed | Open | In Progress | Blocked | Deferred | Done | Won't Fix | Duplicate
priority: Low | Medium | High | Critical
component: Frontend | Backend | Updater | CI | Packaging | Docs | Multiple
tags: [<disposition tag>, ...]   # at least one — see Disposition Tags below
estimate: 15m | 30m | 1h | 1-2h | 2-3h | 3-4h | 4h+
created: YYYY-MM-DD
closed: YYYY-MM-DD
epic: CPE-NNN                     # optional — present on a child ticket, naming its parent epic
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

### Disposition Tags

`tags:` is a **controlled vocabulary** describing a ticket's *disposition* — why it is or isn't
workable now — orthogonal to status (folder), priority, type, and component. **Every ticket carries
at least one** disposition tag, and it is shown as a **Tags** column whenever tickets are listed.
Keep tags current: when the situation changes (a prereq lands, a decision is made), retag.

| Tag | Meaning |
|-----|---------|
| `ready` | Actionable now with resources on hand — no blocker. Mutually exclusive with the blocked/prereq/decision tags below. |
| `big-design` | Substantial; needs a design pass (decisions baked into the design) before coding. |
| `needs-decision` | Blocked on a product/UX decision from the user — record the open question in Notes. |
| `needs-prereq` | Depends on another unbuilt ticket/feature — name it in Notes. |
| `epic` | Umbrella tracker, not a single unit of work; lives in `Tickets/Epics/` (a separate queue), decomposed just-in-time, closes when its children do. See "Epics" below. |
| `resource-blocked` | Needs something the agent can't access in this environment. **Always pair with a qualifier below.** |

Qualifiers for `resource-blocked` (add alongside it):

| Qualifier | Requires |
|-----------|----------|
| `needs-macos-linux` | A macOS/Linux machine to build or verify. |
| `needs-cert` | Purchased / identity-verified certificates. |
| `needs-reference` | An external reference repo or data source. |
| `needs-device` | Specific hardware / a physical device. |
| `needs-heavy-dep` | A non-pure-Rust / native / bundle-heavy dependency that can't be validated headlessly here. |

Rules:
- Exactly one *primary* disposition (`ready` · `big-design` · `needs-decision` · `needs-prereq` ·
  `epic` · `resource-blocked`); qualifiers are additive.
- `resource-blocked` MUST carry ≥1 qualifier so the listing says *what* is needed.
- New primary/qualifier tags are added here first, then used — don't coin ad-hoc tags in tickets.

---

## Status Lifecycle

```
Backlog/ (Open) -> Doing/ (In Progress) -> Done/ (Done | Won't Fix | Duplicate)
                        |
                        +-> Blocked/ (Blocked)   <- EXTERNAL gate; returns to Backlog/ when cleared
                        |
                        +-> Deferred/ (Deferred) <- OUR choice / internal prereq; pick up anytime
```

Only one ticket in Doing/ at a time under normal circumstances.
To reopen: move from Done/ back to Backlog/, set `status: Open`, add a Work Log note.

**Blocked vs Deferred** — both are non-terminal side states, but they differ by *cause*:
`Blocked/` is an **external** gate we can't clear by working (certs, macOS/Linux hardware, a paid
plan, a third party, a date) — not pickable until it clears. `Deferred/` is a **deliberate
postponement** by us — usually waiting on an *internal* prerequisite ticket, or deprioritized to
revisit later — and it stays pickable at any time (`/ticketing-work` un-defers it). Never close
either as Won't Fix; they are postponed, not declined. See each folder's `wiki.md`.

---

## Epics (a separate queue, decomposed just-in-time)

An **epic** is a headline goal too big for one unit of work (a Mega-Feature, or anything that will
clearly spawn many child tickets). Epics are managed by the **`ticketing-epic`** skill and live in
their own queue, **`Tickets/Epics/`** — never in `Backlog/`.

**The core rule: no research, planning, or sub-ticketing until an epic is *activated*.** A dormant
epic is a one-page brief — goal, rough scope, open questions, maybe an epic-level Definition of Done —
and nothing more. Up-front breakdown rots as scope drifts and clutters the backlog with speculative
work. Pulling an epic from the queue IS the decision to invest in planning it.

Lifecycle:

| Stage | Folder / status | What exists |
|-------|-----------------|-------------|
| **Proposed** | `Epics/`, `status: Proposed` | Just the brief. No children, no research. |
| **Active** | `Epics/`, `status: In Progress` | Activated: decisions resolved, child tickets created in `Backlog/` (each with `epic: CPE-NNN`). |
| **Done** | `Done/`, `status: Done` | All children Done + the epic's Definition of Done met. |

- `status: Proposed` is **epics-only** — it marks a dormant, not-yet-decomposed brief.
- **`activate`** (in `ticketing-epic`) is the *only* place an epic is decomposed: research → resolve
  `needs-decision` questions with the user → create `epic:`-linked children in `Backlog/` → set the
  epic `In Progress`.
- Epics are **never** put in `Doing/` and **never** built by `/ticketing-work` (it redirects to
  `ticketing-epic activate`). Only an epic's *children* are worked, as ordinary Backlog tickets.
- Every child carries an `epic: CPE-NNN` frontmatter field so progress is countable and the epic
  closes exactly when its children (and DoD) do.

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
