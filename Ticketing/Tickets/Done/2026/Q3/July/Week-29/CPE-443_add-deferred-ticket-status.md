---
id: CPE-443
title: "Add a Deferred ticket status alongside Blocked"
type: Task
status: Done
priority: Medium
component: Docs
tags: [ready]
estimate: 1h
created: 2026-07-15
closed: 2026-07-15
---

## Summary
Add a new ticket status/folder **Deferred**, a sibling of **Blocked**. Blocked already means
"deferred on an *external* gate we can't clear by working". Deferred fills the missing category:
work that is doable and *not* externally gated, but that we have **deliberately postponed** — usually
waiting on **internal prerequisite work** (another ticket in this repo), or consciously deprioritized
to revisit later. Unlike Blocked, a Deferred ticket can be picked up at any time.

## Motivation
Several tickets (e.g. CPE-432/433/440) have their headlessly-verifiable cores done but a remaining
tail that waits on *other internal work* (the repos sidecar host connection + UI). That isn't
"Blocked" (no external gate) and isn't cleanly "Backlog/ready" either. Deferred is the right home.

## Acceptance Criteria
- [x] New `Tickets/Deferred/` folder with a `wiki.md` defining Deferred and how it differs from Blocked.
- [x] `status: Deferred` added to the frontmatter enum; folder table + status lifecycle updated in
      `Tickets/wiki.md`, `CLAUDE.md`, and `.claude/commands/ticketing-setup.md`.
- [x] `/ticketing-list` shows a **Deferred** section alongside Blocked (visibility; not in the Work
      menu, but explicitly pickable) with a *deferred-on / revisit-when* note.
- [x] `/ticketing-work` can move a ticket to `Deferred/` (internal-prereq / deprioritized) and can
      pick one back up without an external gate to clear.
- [x] Blocked's wiki cross-references Deferred so the two are never confused.

## Resolution
Added the **Deferred** status as a sibling of Blocked. Semantics: Deferred = a postponement *we*
chose (usually an internal prerequisite ticket, or deprioritization) — doable and not externally
gated, so pickable at any time; Blocked = an *external* gate that working can't clear. Changes:
- **`Tickets/Deferred/`** — new folder + `wiki.md` (definition, required *deferred-on / revisit-when*
  Work Log note, and a Deferred-vs-Blocked comparison table).
- **`Tickets/wiki.md`** — folder-structure list, `status:` enum (`… | Deferred | …`), and the status
  lifecycle diagram now include Deferred, plus a "Blocked vs Deferred" paragraph.
- **`Tickets/Blocked/wiki.md`** — cross-references Deferred with the "can working alone unblock it?"
  decision rule.
- **`CLAUDE.md`** — the folder/status table gains a Deferred row; the "showing open tickets" section
  now mandates a **Deferred** table (with a *deferred-on / revisit-when* note) alongside Blocked, and
  responds to "all tickets" too.
- **`.claude/commands/ticketing-list.md`** — always renders a Deferred table; explains Deferred is
  pickable via [3] One though not in Work-all/subset.
- **`.claude/commands/ticketing-work.md`** — pick-up search includes `Deferred/`; a new edge case
  moves not-externally-gated, postponed work to `Deferred/` (status `Deferred`, deferred-on/revisit
  note) rather than leaving a half-done ticket in Backlog.
- **`.claude/commands/ticketing-setup.md`** — folder scaffold, CLAUDE.md template table, and the
  wiki.md lifecycle guidance include Deferred so a fresh setup reproduces it.

Docs/skills only — no code build affected. Natural first candidates for `Deferred/` are CPE-432/433/440,
whose cores are done but whose tails wait on the internal repos-host/UI work (offered separately).

## Work Log
2026-07-15 — Picked up. Estimate 1h. Plan: add the Deferred folder + wiki, then thread the status
through wiki.md, CLAUDE.md, and the three ticketing skills (list/work/setup). Semantics: Deferred =
our-choice/internal-prereq postponement (pickable), distinct from Blocked = external gate (not).
2026-07-15 — Done. Added Deferred folder+wiki and threaded the status through Tickets/wiki.md,
Blocked/wiki.md, CLAUDE.md, and ticketing-list/work/setup. All ACs met.
