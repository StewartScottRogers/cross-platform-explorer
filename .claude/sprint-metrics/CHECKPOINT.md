# Sprint Checkpoint

## RUN 2026-08-11 (CLI resume, BATCHED "up to 50") — 43/50 — CLEAN HAND-OFF
**State:** `main` clean and pushed, **every PR merged, zero of ours open** (only the pre-existing Gource
PR #738, which awaits the user). Worktrees pruned to 2 (one is locked by a dead agent process — harmless,
prune on resume). Sub-agent budget ~133/200 → handing off for headroom. Lock released, wakeups cancelled.
**Zero escaped defects across all 21 merges.**

### FIRST ACTION ON RESUME
Nothing is blocked or half-built. Pick up **CPE-1642** or **CPE-1647** first — both are High, both are
security-shaped, and they overlap each other's design space (see below). Then work down the ready list.

### Merged this session (21)
Agent Watch "off means off" (CPE-1606) · smart-folder notice, now correct + 12 locales (CPE-1614) · File
Health honours unreadable archives (CPE-1603) · `shred_paths` confirm gate (CPE-1611) · co-located agent
sessions both visible (CPE-1625) · docs depth pass (CPE-1619) · **same-file canonicalisation, the High
data-loss fix (CPE-1613)** · .NET metadata tab (CPE-1615) · notebook viewer (CPE-1616) · 45 notices → 12
languages (CPE-1627) · durable checkpoint-failure records (CPE-1600) · pause-vs-end metrics (CPE-1626) ·
log viewer (CPE-1618) · preview-pane screenshot harness (CPE-1629) · **batch-media output containment, High
(CPE-1623)** · onDestroy teardown (CPE-1633) · CPE-1635 (premise disproven — hardening + reusable harness
shipped) · vault_create confirm gate (CPE-1630) · bounded log window (CPE-1637) · YAML/TOML viewer
(CPE-1617) · syntax highlighting (CPE-1631).

### READY QUEUE — ordered, nothing blocked (20 tickets)
1. **CPE-1642 (High)** — Batch Media containment still pattern-matches *link shapes*; symlink **chains** and a
   hard-link check that **fails open under file contention** both escape, demonstrated. The durable fix is to
   resolve the output's true filesystem identity once (volume serial + file index / dev+ino) and compare
   identities. **Design with CPE-1624** (TOCTOU per-write re-check + ADS colons), same files.
2. **CPE-1647 (High)** — `vault_unlock` takes `session_dir` straight off the IPC boundary with **no
   containment check**, so `lock` → `wipe_session_dir` can shred an arbitrary directory. Fix is containment
   (mirror `create_vault`'s existing `resolves_inside`), **not** a confirm flag. **Design with CPE-1645.**
3. **CPE-1645 (High)** — Locking a vault **silently destroys edits made while unlocked**; nothing ever
   re-encrypts, despite `src/docs/20-vaults.md` promising "re-seal". Decide the product behaviour first.
4. **CPE-1624** — batch-media per-write re-check (TOCTOU) + ADS: a colon **anywhere** in a rename template
   writes a hidden NTFS stream onto a same-named file, reachable from the UI.
5. **CPE-1632** — the contrast guard's blind spots (white-on-solid-danger 2.88:1; `--text-faint` 3.45:1).
   The deliverable is **the guard**, not two colours — two failures have now been found by eye.
6. **CPE-1634** — 62 templated `showNotice` calls still untranslated, + a raw literal hiding in a multi-line
   ternary, + the regrowth guard is defeatable by embedding its own marker in the string.
7. **CPE-1636 / 1638 / 1644** — log viewer follow-ups: prose false-positives; filtering hides a stack
   trace's own frames; UTF-16 decode + endianness label + unbounded page cache + stale "Back to latest".
8. **CPE-1641** — a crashed agent session is recorded but **never shown**; its duration is measured from
   deck-close, so History overstates it.
9. **CPE-1643 / 1646 / 1648 / 1639 / 1640 / 1620 / 1621 / 1622 / 1628(Deferred) / 1518(needs NAS)**.

### Owed to the USER (async, non-blocking)
- **Visual/taste glance** on everything shipped: syntax highlighting in all four themes, YAML/TOML tree,
  notebook viewer, log viewer + its filter chips, .NET metadata tab, checkpoint-failure rows.
- **`main` has no branch protection**, so the GUI gate isn't enforced at the merge button. Repo setting.
- A critic's cleanup **closed the user's Chrome session** (rule now recorded); a stray reviewer scratch file
  was committed and removed.
- Gource PR #738 still open, pre-existing.

### Lessons (full version in history.md — read its tail at kickoff)
- **Ask for a number, not an impression** — coverage %, files-that-parse, byte-level proof.
- **Real inputs, never the committed fixture** — it was written by the same author as the code.
- **A negative control or it didn't happen.**
- **A test written by reading the code can only confirm the code** — derive expectations from a spec or
  reference implementation. One test here encoded the bug as its expected value.
- **"We don't know" must never look like "it's fine"** — hit four separate ways.
- **Approve the code, don't approve the record** — a worker overstated a bug as "95% of the stylesheet dead";
  triangulation showed one rule. Correct the record in place.
- **Headless Chrome's `--window-size` lies** (clamps to ~500px, rescales the screenshot) — it produced a
  *false defect report*. Use an iframe, verify the width from inside. **Never `taskkill /IM chrome.exe`.**

### To RESUME: fresh session → "resume the sprint" → start at CPE-1642/1647, batch count continues at 43/50.
