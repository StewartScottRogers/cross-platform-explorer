---
id: CPE-1898
title: backup asserts containment on the destination but trusts the plan for the source — a junction in the source tree copies out-of-tree bytes in
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-26
---

## Summary

CPE-1889 made the backup engine prove where bytes **land**. Nothing proves where they **came from**.

Staged directly against the public `apply_backup_plan`: with a junction at `src/keys` pointing at a
directory outside the source root, the engine copied `PRIVATE KEY MATERIAL` into the destination and
reported `ok=true`. Identical before and after CPE-1889 — that ticket only ever addressed the write
leg.

**This is not reachable through the app today**, and the reason matters: `compare::scan_tree`
(`crates/server/src/compare.rs:38-62`) uses `DirEntry::metadata()`, which reports a junction as
neither a directory nor a file, so the plan never names a path underneath a source junction. The
protection is real, but it is **incidental** — a property of a different layer, with nothing tying it
to the engine.

"Add a follow-links option to the scanner" is an entirely plausible future feature. The day someone
adds it, a backup quietly becomes an exfiltration primitive against any destination the attacker can
read. The engine should not depend on a neighbouring module's accidental behaviour for that.

## Acceptance criteria

- [ ] Assert source containment in `apply_backup_plan_walk` itself, so the engine holds the property
      regardless of what produces its plan.
- [ ] Pin the incidental protection with a test at the `compare::scan_tree` layer too, asserting that a
      junction in the source tree is not descended — so if someone later adds follow-links, they meet a
      red test that explains the consequence rather than a silent behaviour change.
- [ ] Red-proof both: neutralise each guard independently and confirm the harm test goes red naming the
      out-of-tree bytes. Assert on the **destination's contents** (the private-key bytes arriving),
      never on the returned `Result`.
- [ ] Decide explicitly whether a *legitimate* source junction — a user who deliberately links a folder
      into their backup source because they want it backed up — should be refused, followed, or
      followed-with-a-notice. Record the decision at the site. Refusing outright may be wrong here;
      unlike the destination case, the user chose the source.

## Notes

Filed 2026-08-26 by CPE-1889's independent Security Auditor (attack A9), which staged it inside its own
worktree and cleaned up. Defence-in-depth gap, not a live bug — priority reflects that.

Related: **CPE-1889** (the destination half, merged), **CPE-1896** (the residual write-leg race),
**CPE-1194** (the trash-then-restore behaviour), and the wider resolve-before-write family
(CPE-1744/1759 archive, CPE-1742 transfer, CPE-1750 copilot, CPE-1623 batch media) — all of which the
same audit confirmed do check before `create_dir_all`.
