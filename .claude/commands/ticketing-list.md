List all open cross-platform-explorer tickets in Tickets/Backlog/ as a markdown table,
then present an action menu following the rules in menu-render.md.

---

## Step 1 — Display the Table

1. Glob Tickets/Backlog/CPE-*.md
2. For each file read frontmatter: id, title, type, priority, estimate
3. Sort by ID ascending
4. Output the table:

| ID | Title | Type | Priority | Estimate |
|----|-------|------|----------|----------|

### Also surface (read-only context, not part of the workable queue)

After the Backlog table, glob `Tickets/Doing/CPE-*.md` and `Tickets/Blocked/CPE-*.md`. If either has
tickets, show a short second table for each under its own heading:

- **In `Doing/` (in-flight / stalled)** — so abandoned work-in-progress can be restarted.
- **Blocked (gated on an external dependency)** — for each, read the Work Log and give a one-line
  *blocked-on / unblocks-when* note. These are **not** offered in the action menu's Work options
  (working them won't clear the gate); they are shown so nothing silently disappears.

The action menu below operates on the **Backlog** queue only. If the Backlog is empty but `Blocked/`
has tickets, say so explicitly ("nothing workable now; N blocked on external dependencies") rather
than implying there is no outstanding work.

---

## Step 2 — Render the Action Menu

Apply the GROUPED layout (groups are natural here). Omit "Work all" and "Work subset"
if the queue is empty. Omit "Resequence" if there is only 1 ticket.

```
┌─ Ticket Actions ─────────────────────┐
│  Work:  [1] All  [2] Subset  [3] One │
│  View:  [4] Resequence               │
├──────────────────────────────────────┤
│         [5] Dismiss                  │
└──────────────────────────────────────┘
```

Wait for the user's selection, then execute the chosen action below.

---

## Actions

### [1] All — Work every ticket in recommended sequence

1. Run the Resequence analysis (see [4] below) silently to determine order.
2. Confirm the sequence with the user in one line:
   "Working N tickets in this order: CPE-NNN, CPE-NNN, … — proceed?"
3. On confirmation, invoke /ticketing-work for each ticket in sequence.
   Between tickets, report which ticket is next before starting it.
4. If a blocker is hit mid-ticket, pause and surface it to the user before continuing.

### [2] Subset — Work a chosen set of tickets

1. Ask: "Which tickets? Enter IDs or row numbers (e.g. 1 3 5 or CPE-017 CPE-019):"
2. Resolve the selection to a list of ticket IDs.
3. Run the Resequence analysis on that subset only to determine the optimal order.
4. Confirm: "Working N tickets in this order: CPE-NNN, CPE-NNN — proceed?"
5. On confirmation, invoke /ticketing-work for each in the determined order.

### [3] One — Work a single ticket now

1. Ask: "Which ticket? Enter an ID or row number:"
2. Invoke /ticketing-work for that ticket.

### [4] Resequence — Recommend optimal completion order

Analyse the full set of open tickets and produce a ranked list. Apply these factors
in order of weight:

1. **Explicit dependencies** — read each ticket's Notes section for references to other
   CPE-IDs (e.g. "should come after CPE-018"). Tickets that are depended upon rank higher.
2. **Priority** — Critical > High > Medium > Low within the same dependency tier.
3. **Quick-win unblocking** — a low-estimate ticket (<= 30m) that is a dependency of a
   high-estimate ticket rises above higher-priority tickets it doesn't block.
4. **Component clustering** — tickets touching the same component(s) are grouped
   together to minimise context-switching.
5. **Defects before features** — within the same priority, defects rank above features.

Output format — one row per ticket, with reasoning:
```
Recommended sequence:
  1  CPE-019  30m   Quick label fix — zero risk, good warm-up; no dependencies
  2  CPE-018  1-2h  Fixes status bar; CPE-021 depends on this for its error channel
  3  CPE-017  1-2h  Navigation defect; independent, same Frontend cluster as CPE-018
  4  CPE-020  30m   Layout tweak; quick, same files touched in CPE-017/018
  5  CPE-021  3-4h  Largest ticket; explicitly depends on CPE-018 being done first
```

After displaying the sequence, show a follow-up menu:

```
┌─ Sequence Actions ─────────────────────────────────────────────┐
│  [1] Work in this order  [2] Work subset  [3] Adjust  [4] Back │
└────────────────────────────────────────────────────────────────┘
```

**[Adjust]** — ask "Move which ticket? (e.g. move CPE-020 to position 2):"
Accept plain-English or ID+position input. Re-display the sequence after each move.
Offer the follow-up menu again.

### [5] Dismiss

Exit without action.

---

## Menu Extension Point

This menu follows the rules in menu-render.md. To add a new option:
1. Decide its group (Work / View / or new group).
2. Add it to the rendered menu block above.
3. Add its action handler in the Actions section.
4. Update the changelog in menu-render.md.
