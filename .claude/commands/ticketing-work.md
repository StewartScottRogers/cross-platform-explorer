Work the cross-platform-explorer ticket with ID $ARGUMENTS through its full lifecycle:
pick up -> implement -> close.

## Picking Up

1. Find the file. Check Tickets/Backlog/CPE-$ARGUMENTS_*.md first, then Blocked/, then Deferred/, then Doing/, then Done/ recursively.
2. If found in Done/: ask the user whether to reopen before proceeding.
   If found in Blocked/: read the Work Log's blocked-on / unblock note first. Confirm the blocker has
   actually cleared before picking up; if it has not, tell the user what is still gating it and stop.
   If found in Deferred/: read the Work Log's deferred-on / revisit note. No external gate blocks it —
   picking it up *is* the decision to un-defer it; just note that in the Work Log and proceed (if it
   was deferred on an internal prereq, sanity-check that prereq is far enough along).
3. Read the ticket fully — Summary, type, component, and every Acceptance Criterion —
   before touching any code.
4. Review the `estimate:` frontmatter field:
   - If missing: generate one now using the same bands as ticketing-new (15m / 30m / 1h / 1-2h / 2-3h / 3-4h / 4h+)
     based on the full ticket content. Add it to the frontmatter.
   - If present but the full ticket reveals it is significantly off: update it and note the revision.
   Log the estimate (and any revision) as the first Work Log entry.
5. Move the file from Backlog/ to Tickets/Doing/.
6. Update frontmatter: status -> "In Progress".
7. Append Work Log entry: "YYYY-MM-DD — Picked up. Estimate: {estimate}. [one-line plan]."

## Doing the Work

8. Implement the fix or feature. Follow:
   - Tickets/wiki.md — ticket workflow rules
   - CLAUDE.md — project architecture and conventions (how frontend <-> Rust connect, versioning, guardrails)
   - RELEASING.md — if the work affects release/versioning/updater behaviour
9. After every significant discovery, decision, or completed action — append a Work Log entry:
      YYYY-MM-DD — <what was found, decided, or done>
    Do this continuously throughout the work, not only at the end.
10. As each Acceptance Criterion is satisfied, change its checkbox from [ ] to [x].
11. Verify your change: run `npm run check` (svelte-check + tsc). For Rust changes, note that
    `cargo` may not be installed locally — CI type-checks and builds the Rust side on push.

## Closing

12. Verify every Acceptance Criterion is checked [x]. If any remain, complete them first.
13. Write the Resolution section: what changed, which files, why, any tradeoffs.
14. Determine the destination path inside Done/ using depth-detection:

    Inspect Done/ for existing subdirectory structure and match today's date:

    - No subdirectories in Done/           -> place in  Done/
    - Year dirs exist (e.g. Done/2026/)    -> place in  Done/YYYY/
    - Quarter dirs exist inside year       -> place in  Done/YYYY/QN/
    - Month dirs exist inside quarter      -> place in  Done/YYYY/QN/MonthName/
    - Week dirs exist inside month         -> place in  Done/YYYY/QN/MonthName/Week-NN/

    Quarter boundaries: Q1 Jan-Mar, Q2 Apr-Jun, Q3 Jul-Sep, Q4 Oct-Dec.
    Week: ISO week number, zero-padded (Week-03, Week-24).
    Create the destination directory if it does not exist.

15. Move the file from Doing/ to the destination path.
16. Update frontmatter: status -> "Done", closed -> today's date (YYYY-MM-DD).
17. Report a concise summary: what changed, which files were affected.

## Post-Close Menu

After closing, render an action menu following the rules in menu-render.md.
Check whether any tickets remain in Tickets/Backlog/ — omit [1] if the queue is empty.

**Queue has tickets remaining** — HORIZONTAL:
```
┌─ Next Step ─────────────────────────┐
│  [1] Next ticket  [2] Tasks         │
├─────────────────────────────────────┤
│  [3] Dismiss                        │
└─────────────────────────────────────┘
```

**Queue is empty** — HORIZONTAL:
```
┌─ Next Step ──────────┐
│  [1] Tasks           │
├──────────────────────┤
│  [2] Dismiss         │
└──────────────────────┘
```

### [1] Next ticket  *(queue has tickets)*
Run the Resequence analysis on the remaining Backlog tickets and invoke
/ticketing-work for the top-ranked result.

### [1] Tasks  *(queue empty)* / [2] Tasks  *(queue has tickets)*
Invoke /ticketing-list — shows the open queue with its action menu.

### [3] Dismiss / [2] Dismiss
Exit without action.

## Edge Cases

- If the work reveals the ticket is not actionable: explain to the user, ask whether to
  mark it Won't Fix or Duplicate, write a Resolution explaining the decision, move to Done/
  using the same depth-detection logic above.
- If a blocker is found mid-work: log it in the Work Log, describe it to the user, wait
  for direction before continuing.
- If the blocker is an **external gate** the user confirms cannot be cleared by working (a third-party
  or owner action, a paid GitHub plan for Pages, code-signing certificates, a date, an external signoff):
  first do any engineering that *can* be done without it (scaffolding, gating, guarded stubs), then
  **move the ticket to `Blocked/`** — set `status: Blocked` and record in the Work Log *what it is
  blocked on* and *what will unblock it* (prefer a "Next Actions — Owner" checklist so the unblock is
  turnkey). Do NOT close it as Won't Fix — it is deferred, not declined. (Returning a ticket to
  `Backlog/` instead is also fine when no partial work was needed.)
- If the remaining work is not externally gated but should be **postponed by choice** — its
  headlessly-verifiable core is done and the tail waits on an **internal prerequisite ticket**, or it
  is deliberately deprioritized — land the completable part, then **move the ticket to `Deferred/`** —
  set `status: Deferred` and record in the Work Log *deferred-on* (the prereq ticket / reason) and
  *revisit-when*. Distinct from Blocked: nothing external stops it, so it stays pickable at any time.
  Prefer `Deferred/` over leaving a half-done ticket sitting in `Backlog/` marked "Open".
