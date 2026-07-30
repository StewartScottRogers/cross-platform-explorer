# Workshift Checkpoint

**Written 2026-07-29 ~22:12 local (USMST).** Clean end-of-shift stop — **nothing in flight, tree clean,
worktrees pruned, Backlog empty, all merged + pushed + post-merge CI green.** The three honest-headless slices
this file previously queued are now **DONE**. Resume with a fresh session + "resume the workshift"; this file
+ `history.md` carry full context.

## Latest (2026-07-29 eve): instant-index epic CPE-703 CLOSED
Attended session after the docs shift. User approved the instant-index big-design item; research found the
engine (CPE-831/832/833) already built-but-unwired, so it was enablement + UI: CPE-1137 (commands+state),
CPE-1138 (notify watcher), CPE-1139 (Ctrl+K overlay) — all merged, 3-OS CI green, user-GUI-verified, 0 escaped
defects (2 real defects caught+fixed pre-merge). Epic CPE-703 = Done. **Frontier: the last built-but-unwired
attended big-design item is now shipped — remaining epics are GUI/model-key/cert/Mac gated.** QA follow-up
noted (not filed): a gui-smoke render pin for the Ctrl+K overlay. Full detail in history.md.

## What shipped this shift (2026-07-29) — all merged to `main`, full 2-check + UAT gauntlet, 3-OS CI green
- **CPE-1133** (PR #449) — `read_ogg` reassembles the Vorbis-comment packet across OGG pages (real read-side
  correctness bug the old naive `\x03vorbis` scan corrupted). Opus review + independent UAT.
- **CPE-1134** (PR #448) — threaded `revert_attribution` into `checkpoint_preview_revert` (optional `session`;
  `None` = old conservative behaviour). Opus reviewer caught a real safety false-negative (`unwrap_or(0)`
  fallback on a torn index entry); fixed to conservative empty-set + regression test. Needed a
  `bindings.gen.ts` regen (specta doc drift — see the tuned-default note in `history.md`).
- **CPE-1135** (PR #450) — QA slice: `gui-smoke` pins the Agent-Watch Replay-scrubber render (seeded
  audit-journal + baseline fixture). Burns down MVD row CPE-1094 (render automated; feel residual).

**No queued honest-headless work remains.** A fresh 3-sweep researcher pass (2026-07-29, cross-checked vs git)
re-confirmed the well is tapped: every marker hit is correct-but-cautious, every unwired engine is a
documented GUI/model/attended gate, and the two remaining burndown tabs (CPE-1098 cost-ledger, CPE-1100 radar)
are fed by **live IPC only** — NOT seedable from an on-disk fixture, so they can't be gui-smoke-pinned the way
the replay/history tabs were. Next shift: expect to need the user (GUI / model key / signing cert / Mac /
heavy native deps) or an attended big-design go-ahead. **Do NOT manufacture filler `cpe-server` modules.**

## The honest state of the headless frontier (READ THIS before probing epics)
Per `[[headless-frontier-and-cpe-net]]` and re-confirmed today: **the clean pure/headless well is genuinely
tapped.** CPE-999/1001 (thought open) were already Done; CPE-1002's six detectors all Done; CPE-737 fully
complete. Nearly every epic is "In Progress" but their **headless cores are built** — remaining work is
**attended GUI / big-design / user-resource**. **Do NOT manufacture filler `cpe-server` modules.**

### The genuinely-honest headless work still on the table (was queued as wave 2, unbuilt at furlough)
1. **OGG read-side multi-page packet reassembly** — a *real* correctness bug: `read_ogg` in
   `crates/server/src/media_meta_read.rs` naively `\x03vorbis`-scans and mis-reads a comment header split
   across OGG pages. Memory flags this as "a legit read-side correctness slice" (not filler). Needs a proper
   page/packet reassembler; is the safety net that would also unblock the risky OGG **write-back**.
2. **CPE-732 optional headless follow-up** — thread `revert_attribution` into `checkpoint_preview_revert` so
   drift flags only *truly-outside* changes (today it conservatively warns about everything). Noted in the
   CPE-732 epic log as an explicit optional headless refinement.
3. **QA Architect** — fold the **CPE-1114 cost-History visual residual** into the `gui-smoke` CI job: seed a
   synthetic `history.jsonl` and assert `.hd-*`/`.hd-bar` render on the real build. Burns down an MVD row.
   (Was going to be filed as a new CPE ticket — next free id ≈ **CPE-1128**; verify the max before filing.)

### Everything else = surface to the user, don't force it
Big remaining menus, all **user-gated**: the AI-explorer UIs + real embedder/LLM/OCR backend (976–980, need a
model choice / API key); remote-filesystem connections sidebar + keychain + transfer UI + SMB/S3 (616);
native-metadata Properties UI + Mac Finder round-trip (717/828); archive
compress/extract context actions + password prompt UI (705); checkpoint **restore panel + timeline markers**
(CPE-1126, the CPE-732 GUI cap); media-studio editor UI; drag-OUT-to-OS (CPE-672/674, needs a plugin spike +
GUI). Also **CPE-002** code-signing (blocked on the user's cert).

## Tuned crew defaults (seed next shift)
- sonnet worker + opus reviewer for GUI/frontend; opus worker for genuinely-hard slices.
- One-worker-per-file + distinct anchors → zero merge conflicts (held again today).
- Only ONE bindings-touching backend build in flight at a time.
- Foreman-apply / Foreman-review tiny exactly-prescribed changes directly to stretch the agent budget.
- De-risk each hard slice with ONE read-only Plan agent before building.

## Budget at furlough
This session spawned only **3 sub-agents** (3 workers, 0 reviewers/UAT — furlough wind-down). Nowhere near the
200 cap. Fresh session next month = full budget.
