# Sprint Checkpoint

## RUN 2026-08-10/11 (CLI resume, BATCHED "up to 50") — 22/50 — CLEAN HAND-OFF AT THE BUDGET LINE
**State:** `main` clean, everything merged, **zero PRs of ours open** (only the pre-existing Gource PR #738,
which awaits the user). Worktrees pruned to 1 (the repo itself); 39 stale scratch branches deleted. Sub-agent
budget ~130/200 → handing off to a fresh session for headroom. Lock released + wakeup cancelled at hand-off.
**Zero escaped defects** across all 22 merges.

### FIRST ACTION ON RESUME
Nothing is blocked or half-built. Pick up **CPE-1613 first** — it is the only High on the board and it is a
live data-loss path on the user's own platform (see below). Then work down the ready list.

### Merged this session (22 batches)
- **Binary Studio (epic CPE-1562)**: CPE-1581 x86/x64 disasm (#796), CPE-1585 command dispatchers + bindings
  (#800), CPE-1596 hand-rolled ECMA-335 .NET metadata reader (#806), CPE-1597 tabbed Binary Inspector UI (#804).
- **gui-smoke turned from noise into a gate**: CPE-1594 (#801), CPE-1595 triage (#808), CPE-1601 context-menu
  reachability (#808).
- **Safety/data-loss**: CPE-1590 batch-media overwrite confirm (#805), CPE-1599 engine-side refusal (#812),
  CPE-1591 encrypted-zip false-safe (#809), CPE-1602 zip scan verifies declared sizes (#811).
- **Preview**: CPE-1586 font preview (#798), CPE-1593 real cmap coverage + DoS fix (#802), CPE-1598 reservation
  scaling (#807).
- **Docs (epic CPE-1569)**: CPE-1587 Tier-2 depth (#799), CPE-1604 Tags/Smart-folders/Saved-searches split +
  Agent Watch page (#810), CPE-1607 order guard + Pull-label rule (#814).
- **Bugs**: CPE-1577 + CPE-1584 (#797), CPE-1592 Properties architecture (#803), CPE-1605 + CPE-1612 string
  accuracy (#813).

### READY QUEUE — ordered, nothing blocked
1. **CPE-1613 (High)** — `output == input` is compared as **raw strings**, so `IMG_1.JPG` + Convert→jpg is a
   *different string* but the **same file** on Windows/macOS. The new engine guard doesn't fire, AND
   `plan()`'s non-destructive promise has always used the same comparison — so **"write to new files" can
   still overwrite an original**. Fix both call sites with one shared canonicalisation.
2. **CPE-1611 (Medium, raised from Low)** — `shred_paths` backend confirm flag. No trash fallback at all and a
   smaller fix than batch-media's; the original deferral rationale is refuted in the ticket.
3. **CPE-1606 (Medium)** — Agent Watch arms watchers from *every* running session, not from the folder you're
   viewing, so leaving only hides the strip. Contradicts AGENT-WATCH.md's "off means off"; that design doc is
   also stale post-CPE-1099 and should be reconciled either way.
4. **CPE-1614 / CPE-1600 / CPE-1603 / CPE-1518** — blocked-notice conflation + hardcoded English; persistent
   record for failed checkpoints; File-Health archive path must honour `unreadable`; QNAP e2e (needs the NAS).
5. **Epic frontier** — CPE-1562 slices remaining (frontend for .NET metadata now the flag/reader exist),
   CPE-1568 slices 6-8 (notebook/YAML-TOML/log viewers), CPE-1569 slice 10 + 12 (Agent Deck/Workbench/Repos
   depth; screenshot pass — now genuinely possible, artifacts upload).

### Owed to the USER (async, non-blocking)
- **Visual/taste glance** on everything shipped tonight: Binary Inspector tabs, font specimen + glyph grid
  (light **and** dark), context menu + submenu scrolling, batch-media confirm panel, archive-safety banners,
  Trash view. All passed automated checks; only look/feel remains.
- **`main` has no branch protection**, so the new blocking GUI gate isn't enforced at the merge button. Repo
  setting, user's call.
- Gource PR #738 still open, pre-existing.

### Lessons (full version in history.md — read its tail at kickoff)
- **jsdom cannot see layout.** 3,231 tests passed while every submenu was clipped invisible. Verify any
  visual/layout claim in a real browser, and run a **negative control** (reproduce with the old code) before
  believing a green result.
- **Allow a 3rd round when each round finds something NEW; park when findings repeat.** Two tickets earned it.
- **Foreman-apply exactly-prescribed fixes** — did so ~6 times at 0 agents.
- **Tell sub-agents to run everything synchronously.** One stalled awaiting a background notification that
  never comes; restart such an agent with SendMessage.
- **Hold dispatch when the merge queue backs up** — adding PRs lengthens the same CI jam.
- gui-smoke gate: Linux is blocking + ratcheted (baseline now **3** known-failing); Windows leg is
  dispatch/schedule-only; screenshots upload (needs `include-hidden-files: true` — dot-dirs are excluded by
  default).

### To RESUME: fresh session → "resume the sprint" → start at CPE-1613, batch count continues at 22/50.
