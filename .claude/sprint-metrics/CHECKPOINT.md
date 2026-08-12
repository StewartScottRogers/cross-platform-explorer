# Sprint Checkpoint

## RUN 2026-08-11 (CLI resume, BATCHED "up to 50") — **COMPLETE at 50/50** — clean end
**State:** `main` clean and pushed, **every one of our PRs merged, none open** (only the pre-existing
Gource PR #738, which awaits the user). Batched run reached its bound; `BATCH-COUNTER` deleted. Lock
released, wakeups cancelled. **Zero escaped defects; 7 blockers caught pre-merge.**

### FIRST ACTION ON A NEW RUN
Nothing is blocked or half-built. The Backlog holds **19** tickets. Suggested order:

1. **CPE-1645 (High)** — locking a vault **silently destroys edits made while unlocked**; nothing ever
   re-encrypts, despite `src/docs/20-vaults.md` promising "re-seal". Decide the product behaviour first.
   **Design with CPE-1654** (refused-lock UX) and **CPE-1653** (link debris) — same files, and 1654's
   message work will be redone if 1645 lands after it.
2. **CPE-1651 (High)** — `delete_permanent` deletes whatever it is handed and trusts the UI to have
   confirmed. This was **step 2 of a working exploit chain** in the PR #838 review. Mirror the
   `shred_paths` (CPE-1611) / `vault_create` (CPE-1630) consent shape. Audit `move_exact` alongside it.
3. **CPE-1624** — batch-media per-write re-check (TOCTOU) + a colon **anywhere** in a rename template
   writing a hidden NTFS stream. **Design with CPE-1652** (reparse tags + the two-census cost cliff) —
   same files, and CPE-1642 just rewrote them.
4. **CPE-1634** — 62 templated `showNotice` calls still untranslated, + a raw literal in a multi-line
   ternary, + the regrowth guard is defeatable by embedding its own marker.
5. **CPE-1655 / 1656 / 1657** — the log-viewer detector, all three at once as ONE design pass (they pull
   in opposite directions: 1655 widens detection, 1657 tightens it, 1656 does both).
6. **CPE-1649** — high-contrast theme solid-fill buttons fail worse than normal dark (filed by the
   CPE-1632 worker; the new guard will find them).
7. **CPE-1639** (needs a real `tauri build` + tauri-driver run), **CPE-1646**, **CPE-1648**, **CPE-1650**,
   **CPE-1640**, **CPE-1658**, **CPE-1628** (Deferred), **CPE-1518** (needs the QNAP NAS).

### Merged this run (7 PRs / 11 tickets)
CPE-1641 crashed sessions in History (#839) · CPE-1620+1622 repo-URL strip + native model picker (#837) ·
**CPE-1647 vault session containment, High (#838)** · CPE-1632 contrast guard app-wide (#841) ·
**CPE-1642 batch-media output identity, High (#840)** · CPE-1636/1638/1644 log viewer follow-ups (#842) ·
**CPE-1621 close-all-consoles, High + CPE-1643 (#843)**.

### Owed to the USER (async, non-blocking)
- **Visual/taste glance** on everything shipped. One concrete pick-list item: the two colour-blind-safe
  agent swatches now read slightly **mustard / steel-blue** rather than vivid orange / sky-blue
  (screenshots were sent; also at `.claude/sprint-metrics/visual-evidence/cpe-1632-{light,dark}.png`).
- **`main` has no branch protection**, so the gauntlet isn't enforced at the merge button. Repo setting.
- Gource PR #738 still open, pre-existing.
- Older queue still standing: hands-on checks of AI search (v0.57.45), tray, archive-drag.

### Lessons (full version in history.md — read its tail at kickoff)
- **Neutralise each guard separately.** Both big security tickets had a guard that no test was pinning;
  one could be deleted outright with the suite green. Disabling guards one at a time found both.
- **A fix can be worse than the bug it fixes.** CPE-1642 round 1 made long paths fail OPEN — base `main`
  refused the case the "fix" allowed. When a fix swaps a mechanism, diff the new mechanism's *reach*
  against the old one's.
- **Test the guard, not just the code** — the contrast guard passed vacuously on `var(--token, #fff)`.
- **Real inputs beat fixtures.** Every UTF-16 fixture was pure ASCII; one emoji broke detection entirely.
- **Refuse "too noisy to measure"** — a proper control showed the batch-media fix is ~19% *faster*.
- **Two independent legs converging is the strongest signal** — reviewer and UAT hit the CPE-1621
  failure-path defect by completely different routes.
- **A Foreman-applied fix is right when the reviewer prescribes an exact, small change** (0 agents).
