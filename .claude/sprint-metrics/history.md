# Sprint learnings — rolling log

Distilled, human-readable defaults learned from past shifts. Appended at each end-of-shift; read at
kickoff to seed model/parallelism choices. Newest last. Raw per-agent detail lives in `ledger.jsonl`
(gitignored). Format + rationale: `README.md`.

<!-- Each entry: -->
<!-- ## YYYY-MM-DD — <shift one-liner> -->
<!-- - Shipped: CPE-NNN, CPE-NNN … -->
<!-- - Tuned defaults: <class>: <model>, <N>-wide, ~<median> median, <stuck/retry note> -->
<!-- - Notes: <anything that changed a default> -->

_(no shifts recorded yet — first real sprint will seed this)_

## 2026-07-25 — Media-metadata studio: write-back + read codecs + columns (6 shipped)
- Shipped: CPE-1035 (ID3v2 write-back), CPE-1038 (FLAC/Vorbis write-back), CPE-1037 (MP4/MOV video read),
  CPE-1036 (PDF /Info read), CPE-1039 (PDF doc-info columns), CPE-1040 (video-tag columns).
- Tuned defaults: **metadata-codec + column-wiring: sonnet workers, 2–3-wide, ~5m median gauntlet, 0 stuck.**
  Sonnet workers handled every codec/column slice with no escalations.
- Review model: **downgrade metadata-codec/column-wiring reviews from opus → sonnet.** The two opus reviews
  (#352,#355) returned clean APPROVE at ~3–4× the cost; the **sonnet** review (#354) is the one that caught
  the shift's only real bug (object-number substring collision in read_pdf). Sonnet reviewers were at least
  as effective here and far cheaper. Reserve opus only for genuinely gnarly/high-blast-radius work.
- Throughput: serial merge kept pace with 2–3 parallel workers; merge queue never backed up. Pipelining
  Reviewer+UAT in parallel (not serial) roughly halved gauntlet wall-time.
- Frontier note: after this shift the CLEAN headless media slices are tapped — remaining is risky write-back
  (OGG needs page/CRC framing with no strong read-side safety net since read_ogg is a naive scanner; EXIF
  write-back needs TIFF-IFD rewriting) or the attended column-picker UI. Surface, don't manufacture.

## 2026-07-25 (evening) — CPE-724 code-intelligence + CPE-705 archive-planner layer (9 shipped)
- Shipped: CPE-1049 (native-meta OS-interop self-assert test → burndown #8 automated), then the full
  **CPE-724** code-intelligence headless layer — CPE-1050 code_folds, CPE-1051 indent_guides, CPE-1052
  minimap, CPE-1053 code_breadcrumb — and the full **CPE-705** archive planner/detector layer —
  CPE-1054 archive_format, CPE-1055 extract_plan, CPE-1056 compress_plan, CPE-1057 archive_diff.
- Tuned defaults: **pure-logic crates/server module class: sonnet workers, 3-wide, ~5m median gauntlet.**
  10 of 11 slices sailed one-pass; only CPE-1053 needed rework. Keep the 3-wide dispatch + distinct-lib.rs-
  anchor-per-worker trick (zero merge conflicts across 9 PRs). Reviewer+UAT pipelined ≈ halves gauntlet wall-time.
- Review model: **sonnet reviewers earned their keep decisively this shift.** A sonnet reviewer caught TWO
  real correctness bugs UAT missed on CPE-1053 (fold-less symbol swallowing a sibling — first forward, then
  the mirror-image backward case), forcing a robust redesign (drop the "smallest containing fold" rule
  entirely). Another sonnet reviewer caught a cross-OS CI-reddening test (CPE-1055 asserted a Windows-only
  `C:\x` rejection unconditionally → failed ubuntu/macos `Server crates` legs). **Do NOT downgrade these
  reviews.** Both catches were the whole value of the ≥2-check gate.
- Cross-OS lesson (reinforces [[ci-runs-three-os-backend-matrix]]): any test touching `Path::is_absolute()`/
  platform path semantics must `#[cfg(windows)]`-gate the Windows-only case. Brief workers on this up front
  for path/fs tickets; a Windows-only worker self-check won't catch it — the PR's own 3-OS CI does.
- QA: retired burndown #8 (native OS-metadata interop, pinned by Backend+Server-crates 3-OS cargo test).
  Filed CPE-1058 (updater manifest+minisign verify, #6 → 🔧) — **held for user green-light** (adds a dep +
  touches release.yml). MVD 8 → 7.
- Frontier after this shift: CPE-724 remainder is GUI/attended; CPE-705 remainder is the dep-heavy
  bz2/xz/zst READ expansion (needs new decompression crates — user call on binary size) + GUI. The clean
  pure-`crates/server` planner/detector veins across both epics are now largely mined; the next headless
  wave likely wants a fresh epic (PM flagged CPE-735 local-snapshots dedup planner as a runner-up vein).

## 2026-07-25 (late) — CPE-703 search power-filter DSL (4 shipped) + CPE-1058 updater-verify
- Shipped: CPE-1058 (updater manifest+minisign verify crate → burndown #6 download/verify sub-surface
  automated), then the full **CPE-703** power-filter query DSL — CPE-1059 size_filter, CPE-1060 date_filter,
  CPE-1061 type_class, CPE-1062 query_group. All pure `crates/server`, no new shipped-app deps.
- **HEADLINE — independent reviewers caught 4 real defects that workers self-verified clean AND UAT passed.**
  Do NOT downgrade or skip the code Reviewer; it is the highest-value gate in the pipeline. This shift's catches:
  1. CPE-1053 code_breadcrumb — fold-less symbol swallowed a sibling (forward), then the mirror-image (backward) after the first fix. Root-cause redesign (drop rule-2 ancestor-fold inheritance).
  2. CPE-1055 extract_plan — a `C:\x` drive-letter test asserted unconditionally → red the Linux/macOS CI (cfg-gate fix).
  3. CPE-1060 date_filter — absolute-year path used unchecked arithmetic → `parse("99999999999999999")` panics (debug) / wraps (release), reachable from a search token. Fixed by bounding year length.
  4. CPE-1062 query_group — unbounded parse/eval recursion → `"(".repeat(10_000)` = STATUS_STACK_OVERFLOW (uncatchable process abort). Fixed with a shared MAX_DEPTH=128 + tolerant recovery (also caught stacked-NOT as a second unbounded vector).
  Pattern: **UAT confirms the happy path + named cases; the Reviewer is what finds adversarial/overflow/cross-OS/aliasing bugs.** Both are needed. Reviewer stayed on sonnet and caught all four — sonnet reviewers are cost-effective and rigorous; keep them.
- Adversarial-input lesson for pure parsers on user-typed text (search tokens, query strings): brief workers
  UP FRONT to (a) use checked arithmetic / bound numeric input (no unchecked `*`/`+` on parsed numbers), and
  (b) bound recursion depth (or go iterative) in any recursive-descent parser/eval — user text WILL include
  huge numbers and deep nesting. Add these to the worker brief for future DSL/parser tickets to avoid the rework.
- Tuned defaults: **search-dsl pure-parser class: sonnet workers, 4-wide, ~6m median build; ~50% needed one
  rework round (2 of 4) — all reviewer-caught, all fixed one-pass.** Distinct-lib.rs-anchor trick again gave
  zero merge conflicts across 4 PRs. Combined UAT over the 4 sibling PRs (1 agent, 4 verdicts) worked well.
- Frontier: CPE-703 remainder = the index engine (CPE-832 big-design/attended) + live watcher (CPE-833) +
  search overlay GUI (CPE-834) + the integration ticket that grows `Candidate` with size/mtime and wires
  these filters into `Query::parse` (touches index_query.rs — serial/attended). Two tiny unreachable
  follow-ups noted but not filed: size_filter saturate-to-MAX on absurd inputs (safe, undocumented);
  query_group recursive `Drop` on a hand-built million-deep Node (unreachable via public API).

## 2026-07-25 (night) — CPE-728 Activity Replay & Scrub engine (4 shipped)
- Shipped the full CPE-728 headless layer: CPE-1063 replay (state_at event-sourcing projection — the
  foundation), CPE-1064 replay_transport (scrubber step/window/advance, saturating arithmetic), CPE-1065
  replay_view (children_at + diff_states), CPE-1066 replay_session (journal-backed load_replay). Pure folds
  over audit_journal::AuditEvent; reused read_session + activity_timeline::summarize; no deps.
- **Zero rework this wave** — all 4 one-pass APPROVE. The up-front worker briefs added this shift paid off:
  saturating arithmetic (transport advance verified at u64::MAX), cross-OS string-segment paths (no std::path),
  and the prefix-collision guard (children_at uses segment-slice equality, not starts_with — reviewer probed
  `dir="a"` vs `ab/x`, correct). Bake these briefs into every parser/path/numeric ticket.
- Dependency shape: A (replay) is the foundation; B (transport) parallel-independent; C (view) + D (session)
  depend on A — dispatched A+B first, merged A, then C+D. Distinct-lib.rs-anchor trick again → zero conflicts.
- Non-blocking follow-up (unfiled, trivial): add a named `children_at_excludes_sibling_with_dir_name_as_prefix`
  regression test to replay_view (logic already correct + reviewer-verified).
- Frontier: CPE-728 remainder is the scrubber/timeline GUI (play/pause/step/speed bar, jump-to-moment) —
  attended. These 4 modules are exactly the pure data those renderers consume. Standing order "do the next
  epic always" is in effect — rolling to the 5th epic.

## 2026-07-25 (late night) — CPE-730 Multi-agent conflict radar detectors (4 shipped)
- Shipped the full CPE-730 headless detector layer in the **sidecar ai-console crate** (not crates/server):
  CPE-1067 conflict_rename (divergence/collision), CPE-1068 conflict_window (temporal contention, symmetric
  saturating abs-diff), CPE-1069 conflict_owner (per-file attribution + deterministic owner), CPE-1070
  conflict_region (folder heat-map rollup). Pure folds over conflict::AgentActivity; no deps.
- **Zero rework again** — all 4 one-pass APPROVE. The standing briefs held: saturating arithmetic
  (window abs-diff at u64::MAX, saturating_add counts), cross-OS string-segment paths + the prefix-collision
  guard (region rollup: `a/` vs `ab/` stay separate via segment equality — reviewer + UAT both probed it).
- **Sidecar-crate note for future waves:** these modules live in `sidecar/ai-console/src/` (standalone crate,
  CI = the "Sidecar platform" 3-OS job); workers verify from `sidecar/ai-console` with plain `cargo test` +
  `cargo clippy --all-targets -- -D warnings` (cargo at ~/.cargo/bin). conflict.rs uses PLAIN derives
  (Debug/Clone/PartialEq/Eq — NO serde/specta); mirror that, don't add serialization.
- Semantic call logged: conflict_window uses adjacency-in-time (windows(2)) not all-pairs, so 3+ agents chain
  transitively — reviewer judged acceptable (documented + tested + never a false flag). Fine for a live signal.
- Frontier: CPE-730 remainder = the radar GUI (banner / per-file "who else is here" / owner-coloured heat-map)
  + the live per-session attribution FEED that shapes real events into these detectors' caller-shaped inputs
  (epic notes activity isn't session-tagged yet) — both attended. Standing order "do the next epic always"
  in effect — rolling to the 6th epic.

## 2026-07-25 (deep night) — CPE-731 Agent cost & resource ledger (4 shipped)
- Shipped the full CPE-731 headless metric layer in the sidecar ai-console crate: CPE-1071 session_metrics
  (per-session ledger fold, foundation), CPE-1073 throughput (bounded time-bucketed sparkline series),
  CPE-1072 fleet_metrics (per-agent/model/totals + division-safe averages), CPE-1074 efficiency
  (division-safe cost-per-progress ratios + deterministic ranking). Pure folds over usage/estimate_cost;
  no deps.
- **Zero rework, all 4 one-pass APPROVE — third consecutive zero-rework wave.** The standing briefs are now
  fully internalised by workers: saturating_add on every counter (fleet totals at u64::MAX), guarded
  division (0-session averages, all four efficiency ratios → None never inf/NaN), bounded allocation
  (throughput clamps the bucket index to max_buckets BEFORE sizing the vec — reviewer probed u64::MAX/2),
  PartialEq-not-Eq on every f64 struct. Workers even self-caught overflow in their own test helpers.
- Nice pattern this wave: the `None`-ratio ranking in efficiency uses an Option-tuple comparator so a real
  `Some(0.0)` and a `None` never collide (0.0 placeholder is substituted into the OUTPUT only, after the
  sort) — a clean way to rank with missing values. Reworth reusing.
- Frontier: CPE-731 remainder = the dashboard GUI (sparklines/tiles per dataviz conventions) + the live
  FEED shaping real session events into RunRecord/TimedRun (files-touched/churn not session-tagged yet —
  same gap CPE-730/728 noted). Both attended. Standing order "do the next epic always" in effect — 7th epic next.

## 2026-07-25 (deep night) — CPE-729 Intervene & approve — pure policy core (4 shipped)
- Shipped the pure rule-eval/scope/audit core of intervene-and-approve in the sidecar ai-console crate:
  CPE-1075 gate_ignore (gitignore-style matcher — fully ITERATIVE DP glob, no recursion), CPE-1076
  gate_scope (allow-root segment-guard + deny/secret verdict), CPE-1077 gate_decision (command-risk + scope
  fusion, strongest-signal-wins), CPE-1078 gate_audit (decision-ledger fold + division-safe rates). Builds
  on the shipped guardrail.rs; no deps.
- **Zero rework, all 4 one-pass APPROVE — FOURTH consecutive zero-rework wave.** The standing briefs are now
  reflexive for workers: iterative/bounded glob (reviewer verified O(1) stack depth on a 3000×1000 deep
  input — the exact stack-overflow class that bit CPE-1062 earlier), segment-equality scope containment
  (explicit `/repo` vs `/repo-secrets` prefix-collision test — the class that bit CPE-1055/1065), div-safe
  rates, PartialEq-not-Eq on f64 structs. The up-front briefing is paying off decisively.
- big-design handling: CPE-729 is tagged big-design only for the BOUNDARY decision ("the one Agent-Watch
  mode that DRIVES") + the hold-the-action integration. Those stay attended/flagged; the pure policy core
  needs no architecture call (same as CPE-915 shipped earlier under this epic). Scoping only the pure core
  is the right pattern for a big-design epic with a clean headless sub-vein.
- Frontier: CPE-729 remainder = the big-design boundary decision + the hold-the-action integration (pause a
  real agent pre-exec through the sidecar contract — epic flags feasibility as open) + the approve/reject/
  edit-scope GUI + a rule-persistence store (fs, headless-testable — the natural 5th slice). All attended.
  Standing order "do the next epic always" in effect — 8th epic next.

## 2026-07-26 (past midnight) — CPE-732 Checkpoint & rollback revert-safety layer (4 shipped)
- Shipped the revert-safety + surgical-execution layer in crates/server: CPE-1079 revert_attribution
  (agent-touched path set from the session-tagged audit journal — feed-ready NOW), CPE-1080 revert_safety
  (3-way conflict classifier — the flagged un-mined vein: Safe iff agent-touched, else Conflict-outside-agent),
  CPE-1081 revert_engine (surgical real-fs Create/Overwrite/Delete restore — escape-guarded, skip-on-error,
  tempdir round-trip), CPE-1082 conflict summary + safe-subset gate. Builds on shipped restore_plan/snapshot/
  snapshot_capture/audit_journal; no deps.
- **Zero rework, all 4 one-pass APPROVE — FIFTH consecutive zero-rework wave.** Notable integration-seam
  discipline: CPE-1080 and CPE-1079 were built in parallel (agent_touched passed as a plain param), and the
  reviewer explicitly VERIFIED the key-shape seam matches (both produce root-relative `/`-segment keys) — the
  parallel-with-a-shared-key-contract pattern held. revert_engine's escape guard even closed the Linux
  `C:/`-isn't-absolute bypass via a `:`-in-segment check (reviewer probed it).
- Real-fs testing pattern for future waves: process-unique tempdir + a PORTABLE missing-blob simulation
  (point at a never-written hash) instead of OS permission tricks → keeps the Windows CI leg green without
  `#[cfg]`-gating the whole test.
- Frontier: CPE-732 remainder = attended only — the timeline checkpoint-marker + restore/confirm UI and the
  thin `#[tauri::command]` wiring execute_restore + the safe/force choice behind the dialog (serde at that
  seam only). No architecture decision needed for the pure core (High priority, not big-design). Standing
  order "do the next epic always" in effect — 9th epic next. Clean deep headless veins are thinning but not
  yet exhausted (PM should keep being honest about it).

## 2026-07-26 (early hours) — CPE-723 Batch media transform engine (2 shipped)
- Shipped the transform ENGINE CPE-940's plan promised (its doc said "the transform engine executes the
  returned plan" — it didn't exist): CPE-1083 batch_transform (apply_ops bytes→bytes: resize/convert/rotate/
  flip/strip + EXIF-orientation bake, over the vendored image 0.25) and CPE-1084 batch_execute (execute_plan
  real-fs runner, skip-on-error, non-destructive, real planner→executor tempdir round-trip). Reuses
  batch_media::{MediaOp,BatchJob,PlannedItem,plan}; NO new deps.
- **Zero rework, both one-pass APPROVE — SIXTH consecutive zero-rework wave.** Two sharp worker catches this
  wave: (1) image 0.25's `thumbnail()` DOES upscale (resize_dimensions fill=false), so apply_ops guards
  Resize behind an explicit "only if larger" check — correction to the ticket's assumption; (2) decompression-
  bomb guard via `image::Limits` (max 20k px / 256MiB) rejects a 100k×100k IHDR at header-parse, verified with
  a hand-built minimal PNG (own CRC-32, no dep). Bake image::Limits into ANY future raster-decode ticket.
- Structure note: kept this a 2-cohesive-FILE wave (all ops fold into one apply_ops) instead of the PM's
  4-op-split, because splitting ops across workers = editing the same `apply_ops` match = merge conflict. When
  an epic's natural unit is one function, don't force op-level parallelism — one worker per file. Preserved the
  zero-conflict streak (36 tickets, 0 conflicts this session).
- Frontier: CPE-723 remainder = GUI (before/after preview + progress + batch-op dialog) — attended. Per the
  PM, the next-thinnest pure vein is CPE-718's PSD/EXIF-orientation thumbnail slice; after that the frontier
  genuinely closes to GUI/dep-heavy/credential-gated work. Standing order "do the next epic always" in effect
  — 10th epic next (PM to keep being honest as the veins thin).

## 2026-07-26 (early hours) — CPE-718 thumbnail PSD + EXIF-orientation vein (2 shipped) + frontier note
- Shipped CPE-1085 thumb_orient (reusable EXIF orient_for_display — copied CPE-1083's private orientation
  logic, faithful 8-value table) + CPE-1086 thumb_source (PSD decode via vendored psd + image::Limits bomb
  guard, wired into make_thumbnail_png with orientation) — fixing two real thumbnail defects (PSD→generic
  icon, phone photos sideways). No new deps. Both one-pass APPROVE — SEVENTH consecutive zero-rework wave.
- Reviewer caught a real pre-existing OOM: the `psd` crate has no size limit, so a huge-declared PSD OOMs in
  BOTH thumb_source.rs and image_preview.rs. Filed CPE-1087 to bound PSD dims before `.rgba()` in both sites.
- **HEADLESS EPIC FRONTIER REACHED (PM's honest read, validated):** after CPE-718, every open epic's remainder
  is GUI-attended (previews/panels/dialogs, timeline/scrubber, badges), OS-privileged (eject/mount, default-
  handler, Windows junctions/DeviceIoControl), dep-heavy needing a user binary-size call (video/PDF/SVG/font
  extractors, bz2/xz/zst archive read, AVIF/HEIC output), or credential/GUI-runner-gated (the MVD burndown
  rows #1-7). No clean cargo-testable pure feature vein remains unmined across the open epics. Next shift
  should surface this to the user for a direction call rather than manufacture low-value tickets.

## 2026-07-25→26 — SESSION SUMMARY (10-epic overnight, "do the next epic always")
- **39 tickets shipped, all merged + 3-OS-green**, across 10 epics + 2 standalone + 1 reviewer-fast-follow:
  CPE-724 code-intel (1050-53), CPE-705 archive-planner (1054-57), CPE-703 search-DSL (1059-62), CPE-728
  replay-engine (1063-66), CPE-730 conflict-radar (1067-70), CPE-731 cost-ledger (1071-74), CPE-729
  intervene/approve-core (1075-78), CPE-732 checkpoint/rollback (1079-82), CPE-723 batch-media-transform
  (1083-84), CPE-718 thumbnail PSD+orient (1085-86); + CPE-1049 native-meta test, CPE-1058 updater-verify
  crate, CPE-1087 PSD-OOM hardening.
- **Quality: 4 rework cycles across 39 tickets** — every one a reviewer catch UAT+worker missed (sibling-
  swallow ×2, cross-OS `C:\x` CI-red, year-overflow panic, deep-nesting stack-overflow). **The independent
  code Reviewer is the highest-value gate — never skip or downgrade it.** Sonnet reviewers caught all of them.
- **Zero merge conflicts across 39 PRs** via the distinct-lib.rs-anchor-per-worker + one-worker-per-file trick.
  When an epic's natural unit is one function, don't force op-level parallelism.
- **The standing worker briefs now paid off reflexively** (7 straight zero-rework waves at the end): saturating/
  checked arithmetic, bounded recursion + bounded allocation (image::Limits / max_depth), division-safe rates,
  cross-OS `/`-segment equality (never starts_with, never std::path platform semantics), PartialEq-not-Eq on
  f64 structs, sidecar plain-derives. Bake these into every future parser/fs/numeric/media ticket.
- **HEADLESS EPIC FRONTIER REACHED.** After this session, every open epic's remainder is GUI-attended, OS-
  privileged, dep-heavy (user binary-size call), or credential/GUI-runner-gated. Next shift: surface to the
  user for a direction call, don't manufacture low-value tickets.

## 2026-07-26 — GUI shift (code-preview → batch-media, + GUI #3 planned)

**Shipped (6 PRs merged, main green):** CPE-1088 search power-filters (#406) · CPE-1089 code_intel command
(#407) · CPE-1090 preview outline strip+breadcrumb+jump (#408) · CPE-1091 per-line rows+fold gutter+indent
guides+minimap (#409) · CPE-1092 batch-media plan/streamed-execute commands (#410) · CPE-1093 batch-media
dialog (#411). **GUI #1 (code-preview upgrade) + GUI #2 (batch-media dialog) both fully delivered.**

**Tuned defaults learned:**
- `svelte-gui` frontend tickets: **sonnet worker + opus reviewer** is the right pairing — the opus reviewer
  caught 2 real DOM-glue bugs sonnet UAT missed (CPE-1090 scroll header-offset; CPE-1093 avif-eligibility +
  debounce-timer-on-destroy). Keep opus on the reviewer seat for GUI.
- `svelte-gui-complex` (per-line hljs refactor, CPE-1091): **opus worker** paid off — 0 rework, splitter
  span-safety + wrap-safety + copy-excludes-gutter all correct first pass. Use opus when the crux is a
  subtle pure-algorithm (span splitter) or high-blast-radius core render.
- `rust-command-stream` (CPE-1092): sonnet worker, opus reviewer, 0 rework. Streaming commands = mirror an
  existing `*_stream` (apply_backup_plan_stream) + one-walker/two-callers (keep the sync fn for tests).
- **Research spikes before hard tickets pay for themselves**: 3 Explore researchers (hljs-per-line,
  batch-media surface, agent-watch substrate) turned 3 unknowns into shovel-ready tickets with proven
  approaches → the CPE-1091 opus worker had zero flailing. Files in `.claude/research-library/`.
- **Parallelism**: 2 independent builders (frontend CPE-1091 + backend CPE-1092) ran concurrently with zero
  conflict (different file trees) while the merge lock serialized cleanly. Reviewer+UAT always dispatched in
  parallel; next worker started while a PR was in review.
- **Foreman-applied reviewer-prescribed fixes** (2×) were faster than round-tripping the worker for tiny,
  exactly-specified changes — apply in the worker's idle worktree, re-verify, resume the SAME reviewer for a
  focused re-check, merge. ~1-2 min vs a full worker cycle.

**Metrics:** ~30 agent-runs (7 workers, 10 reviewer-passes, 8 UAT, 3 researchers, 2 foreman-fixes), 4 rework
cycles (all reviewer-caught, all fixed+re-approved), cost_proxy ≈ 72k (GUI block). Every merge CI-green.

**Left queued (GUI #3 + polish, all filed with Library-backed designs):** CPE-1094 Agent-Watch replay
scrubber (pure frontend, build first) · CPE-1097/1098 cost ledger (sidecar `cost:`→host `ai-console://agent-cost`
bridge, then panel) · CPE-1099/1100 conflict radar (multi-session watch + actor tags [big-design], then panel)
· CPE-1095 code-preview polish (fold-aware jump + doc wording) · CPE-1096 QA gui-smoke asserts code-preview
render (burns down CPE-1090/1091/1093 visual debt).

## 2026-07-26 (cont.) — GUI #3 Agent-Watch dashboards (all 3 shipped)

**Shipped (6 PRs, main green):** CPE-1094 replay scrubber (#412) · CPE-1097 cost bridge + CPE-405 read-fix
(#413) · CPE-1098 cost ledger panel (#414) · CPE-1099 multi-session watch (#415) · CPE-1101 actor tags (#416)
· CPE-1100 activity-overlap radar (#417). **All three Agent-Watch dashboards done.** Also: cut+installed a
fresh sidecar release 0.57.35 from main (user ran it, batch-media dialog human-verified "looks good").

**Notable:** the CPE-1097 worker found a **silently-broken-in-production bug** — `main.rs` blanket-wrapped
every announcement in `session:`, so `fs-read:`/`cost:` came out as `session:fs-read:{…}` and never matched
the host arm → Agent-Watch "read" activity (CPE-405) had been dead. Fixed + regression-tested in #413.

**Tuned defaults (this GUI initiative, CPE-1088..1101):** 46 agent-runs (12 workers, 14 reviewer-passes, 12
UAT, 4 researchers, 3 foreman-fixes, 1 planner), 6 rework cycles (ALL reviewer/UAT-caught, all fixed+merged),
cost_proxy ≈ 153k. Every merge CI-green.
- **sonnet worker + opus reviewer** stays the right GUI pairing — reviewers caught every one of the 6 rework
  items (scroll offset, avif eligibility, debounce leak, typed-payload AC, +2). Keep opus on review.
- **opus worker** for the genuinely-hard slices (per-line hljs splitter CPE-1091; multi-session watch
  refactor CPE-1099; cfg-gated actor ledger CPE-1101) — 0 rework on those, worth the spend.
- **Research/Plan spikes before hard tickets keep paying** — the hljs-per-line spike, the batch-media surface
  map, the agent-watch substrate map, the sidecar-cost rescope (caught a whole ticket aimed at DEAD CODE), and
  the CPE-1099 multi-session Plan all turned unknowns into zero-flail builds. 6 Library entries filed.
- **Off-means-off** is the recurring Agent-Watch gate — every panel proven to add 0 listeners/threads when not
  watching; reviewers traced it each time. Pure-derivation panels (radar) are the cleanest way to honour it.
- **Foreman-applied reviewer-prescribed fixes** (3×) beat round-tripping the worker for tiny exact changes.

**Left queued:** CPE-1095 (code-preview fold-jump polish), CPE-1096 (gui-smoke asserts code-preview),
CPE-1102 (extend "user" actor-tag to delete_permanent/transfers/extract), CPE-1099c note (honest unknown via
sidecar fs-write:). All low-priority fast-follows with Library-backed designs.

## 2026-07-26 (cont.) — Epic-completion stretch ("do the product calls") + 200-agent cap

**Closed 3 epics:** CPE-724 (code-intel preview), CPE-723 (batch media — added compress CPE-1103 + optional
image-overlay watermark CPE-1106), CPE-728 (activity replay & scrub — event-replay: journal writer CPE-1108,
baseline CPE-1109, replay_load+TS-fold CPE-1110, in-drawer reconstruction UI CPE-1111). **CPE-731 2/3:** 731a
fuller per-session metrics (CPE-1107) + 731b per-session metrics journal+flush-on-end (CPE-1113) merged; 731c
cross-session dashboard (CPE-1114) filed+designed but UNBUILT — hit the **200-agent session cap** first.

**Key wins:** the CPE-728 replay backend was ~80% pre-built+unwired (audit_journal/replay/replay_view/
replay_session) — a Plan pass found it, turning "big epic" into mostly wiring. CPE-731's fuller metrics needed
NO new sidecar capture (frontend JOIN of existing streams). Reused CPE-733's audit-journal pattern for a sibling
per-session metrics_journal. The CPE-1097 worker also fixed a silently-broken-in-production bug (CPE-405 read
activity, double-`session:` prefix).

**Cap lesson:** a very long single session exhausts the 200-agent crew budget. ~25 PRs / session was the ceiling
here. For multi-epic marathons, either raise CLAUDE_CODE_MAX_SUBAGENTS_PER_SESSION or split across sessions so the
crew budget resets. Reviewers again caught every rework item (6 cycles, all fixed pre-merge). sonnet-worker +
opus-reviewer stayed the right pairing; opus-worker for the genuinely-hard slices (fold port, persistence, cfg-gated).

## 2026-07-27 (sprint — short, furlough wind-down)

Resumed on a stale checkpoint; found the prior session had already closed epics CPE-730/731 + the CPE-732
headless scope and cut v0.57.37. Drained the last 3 genuinely-headless tickets, then the user called a
**furlough** (out of tokens until next month's plan). Clean stop: tree clean, worktrees pruned, Backlog empty.

**Shipped (3 workers, all sonnet, 0 rework, all merged):**
- CPE-1119 (#442) — deleted 5 orphaned sidecar `conflict*.rs` modules (946 LOC dead code; grep-proven).
- CPE-1122 (#443) — gated Ctrl+Z `undo()` in read-only views via the shared `blockedInArchive()` predicate.
- CPE-691 (#444) — full-list-render regression guard for the virtualized FileList (test-only, proven falsifiable).

**Metrics:** 3 merged · median gauntlet n/a (Foreman-reviewed, not full gauntlet — furlough) · 0 retries ·
~cost-proxy 3.8k (all sonnet). Admin-merged to save tokens; each passed full local suite; CI runs post-push.

**Re-confirmed frontier lesson (again):** every "open" pure/headless slice probed was already Done
(CPE-999/1001, CPE-1002 detectors, CPE-737). The clean headless well is tapped — remaining epic work is
attended GUI / big-design / user-resource. The *honest* headless work left (queued for next shift, unbuilt):
OGG read-side multi-page packet reassembly (a real correctness bug), CPE-732 revert_attribution threading,
and a gui-smoke extension for the CPE-1114 cost-History visual residual. Full detail in `CHECKPOINT.md`.

## 2026-07-29 (sprint — Ticketing/ container realignment + QA burndown)

Triggered right after the CPE-1128 `Tickets/ → Ticketing/` restructure. Assignment: CPE-1129 (realign the
sidecars to the new container). Shipped it + one QA follow-up, then the headless well was tapped (same
finding as prior shifts — remainder is user-gated GUI/big-design).

**Shipped (2 workers, both sonnet, 0 rework, both merged, full 2-check gauntlet each):**
- CPE-1129 (#445) — standalone `agent-board` sidecar now reads Epics (`Ticketing/Epics/`) + Sprints
  (`Ticketing/Sprints/`) at parity with the in-process board; added `/api/epics` + `/api/sprints` + a
  Board/Epics/Sprints view switcher. `ticket_mcp` + `ai-console` audited already-correct. Reviewer opus
  APPROVE + UAT real-HTTP PASS (negative check: old `Tickets/Epics/` correctly excluded).
- CPE-1130 (#446) — `gui-smoke` now pins the CPE-1114 cost-History render; the real build uncovered + fixed
  a genuine bug (AgentTimeline 5-tab strip overflowed the 340px drawer at default 1000×700 → History tab
  unclickable off-screen; scoped `.tl-tabbar` flex-wrap fix). Reviewer opus APPROVE + UAT PASS.

**Metrics:** 2 merged · gauntlet CPE-1129 ~16m / CPE-1130 ~39m (worker ran real gui-smoke 3×) · 0 retries ·
0 escaped-defects · opus-reviewer + sonnet-UAT gate on both.

**QA:** MVD 7→8 (row #9: standalone-board switcher live-browser click-through, deferred by CPE-1129 UAT);
CPE-1114 cost-History flipped to automated (pinned by gui-smoke via CPE-1130).

**Tuned defaults (seed next shift):** sonnet worker + opus reviewer + sonnet UAT held clean (0 rework). This
machine HAS cargo + tauri-driver + msedgedriver → a worker CAN run the real gui-smoke locally (~30-40m) and
it's worth it (found a real bug that way). No cargo workspace — check each crate dir. Greedy-sed hazard:
`Ticketing/Tickets` → `Ticketing/Ticketing` (cost CPE-1128 a CI red) — make targeted edits.

**Frontier (unchanged):** headless well tapped. Honest-headless-but-unbuilt: OGG multi-page reassembly
(couldn't cleanly locate this shift), CPE-732 revert_attribution threading. Rest user-gated (CPE-002 signing,
format decoders, CPE-672/674 drag-out, CPE-1126 restore panel).

**Budget:** ~9 sub-agents (2 workers + 2 reviewers + 2 UAT + Foreman recon). Nowhere near the 200 cap.

## 2026-07-29 (sprint — the 3 checkpoint-queued honest-headless slices)

Backlog empty, all Deferred/Blocked user-gated. Assignment = the critical path: build the three genuine
headless slices the prior CHECKPOINT queued but never built, then confirm the frontier.

**Shipped (3 workers, all sonnet, full 2-check + UAT gauntlet each, all merged, all 3-OS CI green):**
- CPE-1133 (#449) — `read_ogg` now reassembles the Vorbis-comment packet across OGG pages (real read-side
  correctness bug; the old naive `\x03vorbis` scan corrupted multi-page comment headers). Std-only page
  walker + multi-page/truncation tests. Opus Reviewer APPROVE (verified it doesn't over-reject valid OGG +
  the collateral column_extract fixture fix was legit); UAT PASS (independent OGG builder, 686 truncation
  offsets, 0 panics). Collateral: corrected one fake-OGG test fixture the old scanner accepted.
- CPE-1134 (#448) — threaded `revert_attribution::agent_touched` into `checkpoint_preview_revert` (optional
  `session` param; `None` = old conservative behaviour). **Opus Reviewer caught a real safety false-negative:**
  the `since_ts` `unwrap_or(0)` fallback on a torn index entry attributed the session's ENTIRE history away →
  fewer drift warnings → less safe than `None`. Foreman-applied the exact fix (conservative empty-set on
  index-miss + regression test `..ignores_pre_checkpoint_events`). UAT PASS.
- CPE-1135 (#450) — QA-Architect slice: `gui-smoke` now pins the Agent-Watch Replay-scrubber render (seeds a
  real audit-journal + baseline fixture, asserts `.rp-transport`/enabled `.rp-slider`/`.rp-recon` with the
  seeded filename). Sonnet Reviewer APPROVE (hook gating + fixture shapes verified); UAT PASS (independent
  real gui-smoke 3/3, ~2.5m build + 27s). Burns down MVD row CPE-1094 (render automated; feel residual).

**Metrics:** 3 merged · gauntlet CPE-1133/1134 ~7m worker + ~5m/side gauntlet, CPE-1135 ~19m (real gui-smoke) ·
1 retry (CPE-1134 review fix) + 1 CI fix (bindings drift) · **0 escaped-defects** (all 3 merge commits green
on main) · ~13 sub-agents. Opus reviewer on the two backend correctness slices earned its keep (caught the
attribution false-negative); sonnet reviewer fine for the test-infra slice.

**NEW tuned default (important):** a worker that changes a **specta-exported struct's doc OR shape**
(`RevertPreview` here) MUST regenerate `src/lib/bindings.gen.ts` via
`cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` — local `crates/server`-only
verification NEVER catches it; only the `src-tauri` Backend job's "Typed-bindings drift guard" does. Add this
to every backend-worker brief that touches a `#[cfg_attr(feature="specta"...)]` type. Held again: sonnet
worker + opus reviewer, one-worker-per-file zero conflicts, cargo at `C:\Users\Stewart Rogers\.cargo\bin`
(prepend to PATH), no cargo workspace (run per-crate).

**Frontier — re-confirmed TAPPED (fresh 3-sweep researcher pass, cross-checked vs git):** every marker hit is
correct-but-cautious, not a deferred bug; every unwired engine is a documented GUI/model/attended gate; the
two remaining burndown tabs (CPE-1098 cost-ledger, CPE-1100 radar) are fed by **live IPC only** — genuinely
NOT seedable from an on-disk fixture like the replay/history tabs were, so they can't be gui-smoke-pinned. The
honest-headless queue from the last checkpoint is now **empty**. Remaining epic work is all user-gated.

**Budget:** ~13/200 sub-agents. Nowhere near the reset line.

## 2026-07-29 (sprint #2 — docs debt; user present)

Second shift of the day, started right after the attended GUI session. Backlog empty, headless CODE well
confirmed tapped (this session's Library entry `headless-frontier-tapped-2026-07-29`). The one honest,
headless, user-facing slice remaining was **documentation debt**.

**Shipped (1 worker sonnet, full Reviewer+UAT gauntlet, merged, CI green):**
- CPE-1136 (#451) — documented the **Agent Watch** feature (drawer + Live/Replay/Cost/Radar/History tabs,
  incl. the replay scrubber human-verified earlier this session) as a new `## Agent Watch` section in
  `src/docs/03-explorer.md`. No new docs `Section` (it's a drawer under the explorer). Reviewer fact-checked
  every tab + the drawer-open mechanism against `AgentTimeline.svelte`/`ExplorerPane.svelte`/`i18n.ts`
  (all accurate, no invented UI); UAT confirmed it reads clearly for a first-timer. `npm run check` + the
  `sectionDocs` guard green.

**Metrics:** 1 merged · gauntlet ~11m (worker ~4m + review/UAT ~4m each, parallel) · 0 retries · 0 escaped
defects · 3 sub-agents (worker + reviewer + UAT), all sonnet — opus not warranted for docs.

**Docs-debt audit finding:** batch-media, batch-rename, and code-preview are ALREADY documented; Agent Watch
was the sole gap. So the docs library is now current — do NOT manufacture more docs tickets (would be filler).

**Two minor non-blocking polish notes (NOT ticketed — below the bar):** Cost-tab doc omits the churn/1k-tokens
ratio; the change-chip doc lists created/modified/renamed/removed but a read-only path can transiently appear
as an unstyled chip; "AI Console" could be hyperlinked to `04-ai-console.md`. Fold into a future docs pass if
ever touching that page — not worth standalone tickets.

**Frontier: unchanged — tapped.** Remaining work is user-gated (GUI / model key / signing cert / Mac /
attended big-design go-ahead on the instant-index engine). User was present this session.

## 2026-07-29 (sprint #3 — instant-index epic CPE-703 shipped, attended)

User gave the go-ahead on the "big-design" instant-index engine. Research revealed the engine (CPE-831/832/833)
was ALREADY built + tested but wired to nothing — so the work was enablement + UI, not a from-scratch build.
Sliced into 3, each through the full Reviewer+UAT gauntlet, all merged with 3-OS CI green, then user-GUI-verified.

**Shipped + merged:**
- CPE-1137 (#452, opus worker) — IndexService state + streamed build/search/status/drop commands + per-volume
  persistence + off-means-off. **Opus reviewer caught a same-volume build race** (shared temp file / map);
  Foreman-fixed with a per-volume build lock + superseded-skip + 2 concurrency tests; focused re-review APPROVE.
- CPE-1138 (#453, sonnet worker) — live `notify` watcher → `apply_create/remove/rename`; pure `resolve_touched`
  mapping. **Opus reviewer caught a nested-create ordering bug** (HashSet drain order → child applied before
  parent dir → apply_create silently drops it, losing files from archive-extract/git-checkout/cp-r on Windows);
  Foreman-fixed by sorting ancestors-first in a pure testable helper + ordering unit test + end-to-end
  nested-create regression test.
- CPE-1139 (#454, sonnet worker) — keyboard-first Ctrl+K global search overlay: debounced streamed
  index_search with generation-token supersede, ↑/↓+Enter reveal (reuses the navigate contract),
  "Build index" opt-in affordance (off-means-off), i18n ×12, docs subsection. Reviewer APPROVE + UAT PASS.

**Metrics:** 3 merged · gauntlet ~30-50m each incl. re-review · 2 retries (both real defect fixes) · **0 escaped
defects** (all 3 merge commits green on main) · ~18 sub-agents this epic. opus for the foundation worker + all
backend reviewers earned its keep (caught both concurrency/ordering bugs that unit tests + 3-OS builds passed
clean); sonnet fine for the watcher/overlay workers + the overlay reviewer.

**GUI-verify:** built the sidecar app from main (no devtools needed — instant search is real UI, not console-
seeded like the replay scrubber was), installed, launched; user confirmed Ctrl+K → Build index → cross-folder
type-ahead → reveal → live-edit-reflects all work. Epic CPE-703 CLOSED.

**Held again:** the specta-bindings regen discipline (CPE-1137 regenerated bindings for its new commands);
one-worker-per-file zero conflicts; opus-reviewer-on-backend-concurrency is worth it. **Frontier now:** the
instant-index epic was the last attended big-design item with a built-but-unwired core; remaining epics are
GUI/model-key/cert/Mac gated. QA follow-up noted (not filed): a gui-smoke render pin for the Ctrl+K overlay.

**Budget:** ~18 sub-agents this epic; well under the 200 cap across the whole session's shifts.

## 2026-07-29→30 (sprints ×2, attended — board hygiene + UX/feature polish)

User asked for "two sprints back to back", then gave two mid-run directives (epic-board cleanup + a pane
min-width bug). Delivered both plus two "instant-index twin" features the shift-2 researcher surfaced.

**Shift 1:**
- **Epic board hygiene** — reverted all 33 dormant "In Progress" epics → Proposed (none were actually being
  worked; 30 had every child Done). Board now honestly shows 5 Done / 33 Proposed / 0 active. ~30 flagged as
  Done-candidates for a future DoD review.
- **CPE-1140 (#455) pane min-widths** — middle pane got a real min (MID_MIN=372, floored at the Name column),
  removed both side maxes, dynamic side-pane clamp so the middle never collapses. Reviewer caught an
  order-dependent load re-clamp; Foreman-fixed with a proportional `fitSidePanes` + tests. UAT PASS,
  user-GUI-verified (drag panes wide, middle holds its columns).
- **CPE-1141 (#456) archive commands** — wired the built-but-unwired `compress_archive`/`compress_to_zip_encrypted`/
  `extract_zip_encrypted` engine fns as commands (tar.gz + password zip). Reviewer APPROVE + UAT PASS (backend-only).

**Shift 2:**
- **CPE-1142 (#457) rules-based auto-organize** — the researcher's #1 pick (built+tested `organize.rs`, unwired).
  Shipped a safe propose → checkpoint → apply → undo feature: `organize_plan` (read-only) / `organize_apply`
  (checkpoint FIRST, then move into `dir/<subdir>/`, collision pre-checked = never overwrites) + an
  OrganizeDialog (rule picker, grouped preview, Apply, one-click Undo via the checkpoint). **opus data-safety
  review** confirmed no overwrite / full-restore / no path-traversal; UAT proved the round-trip + collision-skip;
  user-GUI-verified (organize the demo folder, Undo restores). AI-assisted organize mode stays model-gated (skipped).

**Metrics:** 4 PRs merged · **0 escaped defects** (all merge commits green on main) · gauntlet caught 4 real
pre-merge defects (build race, nested-create ordering, since_ts safety false-negative, load-clamp order) · ~26
sub-agents across both shifts. opus reviewers on the concurrency/data-safety slices earned their keep every time.

**Shift-2 research verdict (filed as knowledge):** two real "instant-index twins" existed (CPE-979 rules-organize
[shipped], CPE-705 archive residual [shipped]); **skipped CPE-976 semantic** (FakeEmbedder = lexical overlap, not
real meaning — would mislead without a real embedder); OCR/copilot/smart-folders/near-dup are genuinely
model- or GUI-gated. Frontier is now user-gated except a real-embedder decision for the AI cluster.

**Tuned defaults held:** sonnet worker + opus reviewer for anything concurrency/data-safety/file-moving;
one-worker-per-file zero conflicts; regen `bindings.gen.ts` on any new specta command; batch GUI verifies into
one build→deploy→run.

## 2026-07-30 (sprints ×3 request — QA pins + epic closure; frontier confirmed user-gated)

User asked for three back-to-back shifts. Honest genuine autonomous work found + done (did NOT pad with filler):
- **QA new-surface pins (CPE-1143, #458):** gui-smoke render pins for the Ctrl+K instant-search overlay +
  the auto-organize dialog. Reviewer APPROVE + UAT independent real gui-smoke 5/5.
- **Epic closure review (opus DoD pass):** closed 4 genuinely-complete epics (CPE-711/737/740/862) and
  annotated the other 29 Proposed epics with their specific one-line remainder (mostly user-gated GUI or a
  real model). Board now 9 Done / 29 Proposed / 0 In Progress. Caught that the "all-children-done" heuristic
  was unreliable (CPE-691 in Done/ tree is actually Deferred; some epics decomposed only a headless first-slice).
- **Last QA pin (CPE-1144, #459):** gui-smoke render pin for the Batch-Media dialog (hand-rolled valid-PNG
  fixture; inert data-testids). Reviewer APPROVE + independent real gui-smoke 6/9. Flipped burndown CPE-1093.

**Metrics:** 2 PRs merged (1143, 1144) + epic closures · 0 escaped defects · gui-smoke suite now 6 specs/9 tests.

**Circuit-breaker lesson (user directive):** hit `API Error 529 Overloaded` during heavy parallel fan-out.
Saved memory `[[circuit-breaker-for-retryable-errors]]`: on 529, bounded exponential-backoff RETRY (don't
give up) AND throttle concurrency/burst rate (don't cause it) — fewer simultaneous agents, staggered dispatch,
longer tick gaps. Also: a sub-agent that yields waiting on an async notification (which sub-agents don't
receive) is a stall — resume ONCE with "run synchronously" (fixed CPE-1144's worker that way).

**Frontier — now genuinely tapped for autonomous work.** After the two QA-pin shifts + epic closure, the
remaining QA debt is the cost-ledger (CPE-1098) + radar (CPE-1100) tabs, both **live-IPC-fed = not seedable**
from an on-disk fixture, so not gui-smoke-pinnable without a live agent session. Everything else (the 29
Proposed epics) is user-gated: a **real model/embedder/API key** (976 semantic / 977 copilot / 979 AI-mode /
980 OCR), **GUI work to build together** (704 spotlight / 707 column-picker / 705 password UI / 713 tray /
714 terminal / 716 drive-bay / 717 Properties / 720 player / 725 studio write-back / 729 approve UI / 735
snapshots / 738 vaults / 739 macro bindings / 978 smart-folders / 997 near-dup review / 1000/1002 detector
surfacing / 1126 restore panel / 672-674 drag-out), a **cert** (CPE-002), or a **Mac** (717/829, 712).
Next shift: pick a user-gated item to do together — no more solo headless work remains.

## 2026-07-30 (attended feature — column-picker, epic CPE-707 CLOSED)

User picked the column-picker (CPE-707) to build together after the frontier went user-gated. The metadata
engine (per-family extractors + typed CellValue sort, CPE-918/971/974/975/1028/1029) was built-but-unwired, so
this was enablement + UI, shipped in slices through the full Reviewer+UAT gauntlet + user GUI-verify:
- **CPE-1145 (#460)** — streamed `metadata_column_cells` command + `metadata_columns_available` + MetaColumn/
  CellValue wire + bindings. Opus reviewer caught a real data-correctness bug: the 1 MiB header cap made
  `DocPages` (a linear /Type/Page scan) silently UNDERCOUNT a big PDF (wrong, not empty) — fixed to degrade to
  Empty on a truncated read + a >1 MiB regression test.
- **CPE-1146 (#461)** — dynamic FileList columns (reuses length-agnostic helpers), streamed visible-row fetch
  guarded by loadGen, fetch-on-sort with type-aware `compareCellValues` (numeric/area, Empty-last), per-folder
  `metaColumnsByFolder` persistence, ColumnPickerDialog + palette + header button. Reviewer + UAT both traced
  the full journey; no CPE-1140 regression (MID_MIN built-ins-only).
- **CPE-1147 (#462, #463)** — GUI-verify polish iterations: moved the picker button to the header LEFT
  (absolute-positioned → zero grid tracks, can't misalign columns), switched the icon gear→**spreadsheet grid**
  (user-chosen from a pick-list), and fixed the icon right-clip (shared `.col` overflow:hidden → overflow:visible
  + width 24→28). Foreman-applied the tiny CSS/icon iterations directly.

**Metrics:** 4 PRs merged · 0 escaped defects · gauntlet caught the PDF-undercount + the button clip/placement.

**529 throttle applied:** after hitting `API Error 529 Overloaded`, dropped to single-agent/sequential dispatch
+ longer tick gaps (memory `[[circuit-breaker-for-retryable-errors]]`). No further overloads.

**Process idea raised by the user (NOT yet built — awaiting go-ahead):** a **Visual Critic** role +
gui-smoke **screenshot capture** so the sprint can judge GUI work from screenshots (as the gauntlet's visual
leg, next to Reviewer=code / UAT=behavior) and escalate to the user minimally. This directly targets the
button-polish round-trips (clip/placement are screenshot-visible; only pure icon *preference* needed the user).
When approved: a QA-Architect ticket for the screenshot harness + a sprint-skill change adding the Critic to
the crew + gauntlet. The manual-test burndown's row #1 (headless GUI) + row #3 (visual/theme regression) are the
natural homes.

**Frontier:** still user-gated (the 28 Proposed epics' remainders — model/GUI-together/cert/Mac).

### 2026-07-30 16:31 USMST — resume: CPE-1126 visual layer landed
- User chose (a): build CPE-1126's visual layer, reserve the revert-safety judgment for them.
- CPE-1126 checkpoint markers + restore panel: worker built (PR #466), reviewer APPROVE (safety pattern sound, 1410 tests green), merged as 6e79f159.
- Ticket-tree hygiene fixed: CPE-1126 was duplicated (Deferred+Doing) — removed the Deferred dup; CPE-1148/1149 Done files had uncommitted status flips — finalized. (Root cause: an earlier  sans --delete-branch aborted at the branch-delete prompt AFTER merging, and a --ff-only pull couldn't advance over local staged changes; reconciled by hard-syncing to origin/main + restoring the finalized files.)
- Filed CPE-1150 (reviewer flag: no component test for the two-step revert confirm gate).
- Remaining on CPE-1126: user-present GUI safety/clarity verify (kept in Doing/, on the MVD burndown).

### 2026-07-30 20:29 USMST — wrap: CPE-1126 restore-panel arc complete (first Visual-Critic verdict)
- User chose (a): build CPE-1126's visual layer, reserve the safety call. Delivered the full arc:
  - CPE-1126 (checkpoint markers + restore panel) — built, reviewed, merged (#466).
  - CPE-1150 (two-step confirm-gate component test) — #467.
  - CPE-1151 (session-precise drift + drift echo in confirm) — the user's 2 verify refinements — #468.
  - CPE-1152 (gui-smoke checkpoint seed) — #469 — made the panel screenshot-able.
- **First formal Visual Critic verdict on a GUI ticket: VISUAL PASS.** The Critic judged the real captured
  restore-panel + confirm screenshots (look + revert-safety clarity); user shown the screenshots + verdict
  and signed off. CPE-1126 closed; its manual-verify-debt row RETIRED. The CPE-1148 loop paid off exactly as
  intended — crew verifies its own GUI, user looped in only for the final reserved call.
- Bookkeeping: caught + fixed a bad merge-sequence that had duplicated CPE-1126 + left 1148/1149 closes
  uncommitted; caught a stalled worker (yielded on a background notification) + resumed it synchronously;
  caught a stale-binary trap in gui-smoke (rebuilt to prove CPE-1151's echo).
- Throttle held all session (single agent in flight, staggered) — no 529 recurrence.
- Queue now: Doing empty, Backlog empty. Remaining work is user-gated (interaction-feel) or resource-gated.

### 2026-07-31 17:04 USMST — wrap (post-context-menu-arc shift; modest honest batch)
- Queue was empty at kickoff; PM (opus) scouted the frontier: deep headless well still essentially TAPPED, but the CPE-707 column-picker landing (2026-07-30, after the last sweep) reopened ONE real vein.
- Shipped (all merged, reviewed, pushed):
  - CPE-1166 (feature, sonnet worker / opus reviewer) — surfaced the dormant CPE-1000/1002 detectors (true-type / type-mismatch / encoding / line-endings) as opt-in metadata columns via the generic pipeline, NO new GUI; "applies-to-all" sentinel (empty extensions()). bindings regen, 3-mode clippy, 1096 cpe-server tests.
  - CPE-1167 (gui-smoke pin) — pinned the ColumnPickerDialog render + confirmed the 4 new columns appear in the REAL picker (doubled as 1166's end-to-end check).
  - CPE-1168 (QA pin, sonnet) — zero-dep headless click-through for the standalone agent-board sidecar; FOUND + FIXED a real view-switch bug ([hidden] overridden by display:flex so panes never hid); MVD 8->7. Two-board lockstep verified (in-process BoardView is svelte-if, immune).
- Tuned defaults confirmed: sonnet worker + opus reviewer for backend-correctness; test-infra fine on sonnet. One-worker-per-file. Slow Z: drive → cap concurrent builds at 1, stagger; git/lock writes routinely 2min-timeout under a worker build (harmless — retry).
- Frontier verdict: after this batch the well closes back to user-gated (GUI interaction-feel, model/embedder, certs, Mac). Filed no filler. Next genuinely-headless work will likely need a NEW capability to land first (as CPE-707 did) before another vein opens.

### 2026-07-31 18:58 USMST — wrap: three back-to-back QA/hardening batches (well confirmed near-dry)
- User asked to "run three sprints back to back." Queue was empty; PM (opus) frontier scout confirmed the
  deep headless well is essentially TAPPED — all 38 epics' remainders user-gated — leaving one honest
  QA/hardening batch, a thin second, and NO honest third. Ran it exactly that way; filed zero filler.
- **Shipped (6 PRs merged, all through the ≥2-check gauntlet, all pushed, final HEAD CI 10/10 jobs green ×3 OS):**
  - **Batch 1:** CPE-1169 parser panic-safety property harness (#487, sonnet worker / opus reviewer) — 27
    entrypoints × ~1000 adversarial inputs, catch_unwind, no parser bug found (bounds-checking held). **Opus
    reviewer caught a REAL hollowness bug**: the ID3 frame loop + PDF body were never reached (fake magic
    rejected at the container gate). Foreman-applied the fix (maxed syncsafe ID3 size across majors 2/3/4;
    full `%PDF-` sig), re-verified 27/0 with the loops genuinely exercised. CPE-1170 visual-diff comparator
    for gui-smoke (#486) — hand-rolled PNG decode + pixel diff, advisory (GUI_SMOKE_VISUAL_STRICT), 15 tests,
    no deps. CPE-1171 Linux gui-smoke CI leg (#485) — ubuntu xvfb + WebKitWebDriver, non-blocking; first live
    run: 11/12 steps green incl. the real tauri build, only the xvfb WebDriver drive flaked (known WebKitGTK-CI
    class, same as the Windows leg's CPE-1048) → row 4 stays 🔧.
  - **Batch 2:** CPE-1172 (#488) — un-hollows the comparator suite by testing the Sub/Up/Average/Paeth defilter
    branches (the ones that decode live screenshots); 21 tests, mutation-tested.
  - **Batch 3:** CPE-1173 (#489) — gui-smoke cost-ledger pin via a test-mode `__CPE_TEST_INGEST_COST__` hook
    (spike-confirmed feasible, mirrors the CPE-1130/1135/1114/1152 precedent).
- **Escaped defect caught + fixed same shift:** CPE-1170 red-ded main — the root vitest glob had no
  `gui-smoke/` exclude, so it collected gui-smoke's `node:test` files and failed them "No test suite found".
  Foreman hotfix **CPE-1174** (#490) added `**/gui-smoke/**` to `vite.config.ts` exclude; root vitest back to
  130 files / 1482 tests. **Gauntlet hardened:** any gui-smoke-touching change now also gates on root
  `npm test`, not just gui-smoke's `test:unit` (that gap let the escaped defect through). Ledger:
  CPE-1170 back-annotated `post_merge_defect: ci-red`.
- **Metrics:** 6 PRs merged · 1 pre-merge defect caught (hollow harness) · 1 escaped defect caught+fixed
  (ci-red) · 1 retry (the 1169 Foreman-applied fix) · ~16 sub-agents (nowhere near the 200 cap) · zero merge
  conflicts except two trivial burndown/README rebases. Tuned defaults held: sonnet worker + opus reviewer for
  correctness-sensitive test-infra; one-worker-per-file; slow Z: → 1 cargo build at a time.
- **Frontier verdict (unchanged, now firmer):** after this batch the honest headless well is dry. Everything
  left is user-gated (GUI interaction-feel, model/embedder/API key, code-signing cert, Mac). A NEW capability
  must land before another solo-headless vein opens (as CPE-707 did for CPE-1166). MVD still 7 (rows 3 & 4
  advanced to 🔧-with-ticket; none fully retired — visual-diff needs real blessed baselines, Linux leg needs
  to go reliably green).

### 2026-08-01/02 — catch-up: five epics + backlog cleared across back-to-back mega-runs (~37 PRs), then vaults
This block back-fills runs that landed on 2026-08-01 and 08-02 but were never distilled here (git log had
them; history.md stopped at 07-31). Authoritative record is the checkpoint commits; distilled defaults below.
- **Epics CLOSED (all merged, gauntleted, pushed):** CPE-704 global quick-launch spotlight, CPE-978 smart
  folders & saved searches, CPE-718 universal-thumbnail (headless slice: SVG/font extractors + streaming
  client), CPE-714 embedded terminal dock (PTY backend #544 + xterm.js UI #545, hardened #546/#547/#548),
  CPE-738 encrypted vaults (crypto core→lifecycle→mount→create-UI→security-doc #550-553) + secure-delete.
- **Notable follow-ups:** CPE-1212 danger-colour centralization; CPE-1225/1227 snapshot-schedule migration
  + regression test; CPE-1235 parentDir POSIX-root edge; CPE-1239 thumbnail retry-cap; CPE-1244/1246 PTY
  lifecycle hardening (in-proc + sidecar parity); CPE-1252 vault orphan-session sweep; CPE-1253/1254
  home-menu + compress-password refresh race.
- **Tuned defaults held:** sonnet worker + opus reviewer for backend-correctness; test-infra fine on sonnet;
  one-worker-per-file; slow Z: drive → cap concurrent cargo builds at 1 and stagger. GitHub Actions runners
  were intermittently STALLED late 08-01 into 08-02 — merges were verified via the full local triad
  (Reviewer + UAT + Visual) + built-app gui-smoke instead of waiting on offsite CI.
- **Bookkeeping note:** epic *frontmatter* statuses were correctly flipped to `Done` during these runs; only
  this rolling log and some in-file Work-Log *prose* lagged. Epic status is trustworthy; narrative prose in
  older epic bodies may describe a pre-build state.
- **Frontier verdict (re-confirmed 08-02, 4th time):** the honest solo-headless PRODUCT well is TAPPED. Every
  remaining epic slice is user-gated — model/API key (976/977/979/980), code-signing cert (002), Mac/OS
  hardware (616/717/713/716), heavy native deps (1238 ffmpeg/pdfium), or interactive GUI feel. Unwired
  `cpe-server` modules map exactly onto that gated set. Next product vein needs a user "you choose" pick.

### 2026-08-02 (cont.) — user green-lit 4 gated epics; SHIFT A shipped: thumbnails epic CPE-718 CLOSED
- After the honest "well is tapped" report, user picked ALL FOUR gated epics off the wrap pick-list → real build run.
- **Shift A shipped (3 PRs, all opus-reviewer gauntleted, pushed):** CPE-1256 PDF thumbnails (pdfium-render in-process,
  `pdf-thumb`, #558) · CPE-1257 video thumbnails (bundled-ffmpeg shell-out, `video-thumb`, #559) · CPE-1258 enablement
  (features on + per-feature CI + release native-dep staging + docs, #560). Epic CPE-718 CLOSED; CPE-1238 done.
- **Gauntlet SAVE:** the #560 opus reviewer caught a real blocking bug the CI could NOT — resolvers used
  `current_exe().parent()` but Tauri stages resources to `resource_dir()` (differs on macOS/Linux) → silent icon
  fallback on 2 of 3 OSes. Fixed by injecting `app.path().resource_dir()` into the Tauri-free cpe-server via a setter
  (mirrors resolve_sidecar_bin). 1 retry (attempt 2 passed). This is exactly why the ≥2-independent-checks gate exists.
- **Tuned defaults held:** sonnet worker + opus reviewer for backend-correctness; ONE heavy cargo build at a time on the
  slow Z: drive (serialize reviewer builds vs new workers; pipeline only non-build work). Research-first for the
  dep-weight + drag-out architecture calls (2 opus researchers, both filed to Library → reused, not re-run).
- **Frontier note:** the "tapped" verdict was PRODUCT-work-without-a-user-decision; once the user made the decisions,
  large genuinely-headless veins opened (thumbnails; AI search via the pre-built local embedder needs NO key). Lesson:
  a user "you choose"/dep-approval unlocks shifts of real work even when the autonomous frontier reads dry.

### 2026-08-02 (cont.) — SHIFT B shipped: AI file-content search (epic CPE-976 headless core)
- **Shipped (2 PRs, opus-gauntleted, pushed):** CPE-1262 backend wiring (content_index_build streamed + content_search
  + per-root persist over the pre-built SemanticIndex/local FakeEmbedder — NO key, #561) · CPE-1263 content-search UI
  (ContentIndexSearchDialog: query + streamed build progress + ranked snippets + navigate; 11 jsdom tests; palette entry;
  docs; i18n ×12 locales, #562).
- **Gauntlet SAVE #2:** #561 opus reviewer caught a REAL reachable panic — snippet_for indexed the original char-vec
  with a char-index computed from a SEPARATELY-lowercased string; Turkish `İ` (1→2 chars under lowercase) made start>end
  → slice panic crashing content_search. Fixed via per-char positional lowering (len-aligned) + windows() match + 2
  regression tests. 1 retry. (Shift A had a similar save on #560's bundled-path bug.) The ≥2-check gate is earning out.
- **Key enabler:** the "AI needs a key" gate was FALSE for search — the pre-built local FakeEmbedder (bag-of-words) makes
  content search work offline with no model/key. Framed honestly as file-CONTENT search, embedder-pluggable (a real model
  is the deferred, user-gated upgrade). CPE-976 headless core done; better-embedder + pdf/docx text extraction deferred.
- **Process note:** workers that move ticket files to Done in their PR while the Foreman also moved them to Doing in main
  → squash merge DUPLICATES the ticket (both Doing/ + Done/ copies). Happened twice (CPE-1262/1263); cleaned up each time.
  FIX GOING FORWARD: Foreman owns ticket-file lifecycle in the main tree; tell workers NOT to move ticket files.
- **Tuned defaults held:** sonnet worker + opus reviewer; one heavy build at a time (serialize reviewer vs worker builds);
  frontend slices verify fast (jsdom + vitest). GitHub Actions runners STALLED entire run → merged via local triad + --admin.

### 2026-08-02 (cont.) — SHIFT C shipped: drag-out foundation + gauntlet-surfaced hardening; 3-shift run COMPLETE
- **Shipped (3 PRs):** CPE-1264 drag-out PLUMBING (tauri-plugin-drag v2.1.1 + drag:default capability + dragOut.ts
  wrapper, not wired to rows — headless foundation, #563) · CPE-1261 video-thumbnail temp-file hardening (exclusive
  scratch dir, CWE-377 close, #565) · CPE-1265 content-search UI robustness (gen-bump on rebuild + probe-reject test, #564).
- **Honest scope call:** the REMAINING drag-out (wire-into-rows + coexistence, archive-on-drag) and ALL shell/OS
  integration (712 default-handler / 713 tray / 716 drive-bay) genuinely need ATTENDED verification (real drag-drop,
  OS registration, hardware) — building them blind risks unverifiable regressions. So shift C delivered the headless
  drag-out foundation + closed the two hardening follow-ups the gauntlet surfaced this run, rather than padding with
  blind code. The attended remainder is handed to the user as a checklist.
- **Run totals:** 9 PRs merged (#557 radar pin + #558-565), 0 escaped defects. The ≥2-check gauntlet caught 2 real
  BLOCKING pre-merge defects CI could not (macOS/Linux bundled-binary path mismatch #560; Turkish-İ snippet slice
  panic #561) + several non-blocking items (→ filed+fixed CPE-1261/1265). Full local verification throughout
  (GitHub Actions runners stalled the entire run; merged via --admin). Sonnet worker + opus reviewer for
  backend-correctness held as the winning default.
- **Frontier lesson (reinforced):** the autonomous "well is tapped" verdict meant "no PRODUCT work without a user
  DECISION" — a single pick-list answer from the user (green-lighting deps/model/scope) unlocked 3 full shifts of
  genuinely-headless work (thumbnails; content search via the pre-built local embedder needed NO key). Always offer
  the decision, don't just report dry.

### 2026-08-02 (cont.) — user "run so I can eyeball" → CI-recovery arc + thumbnail end-to-end fix
- User asked to install+run the app to eyeball the new features. Uncovered a cascade, all now resolved:
- **CI stall root-caused + fixed (CPE-1266):** GitHub Actions had been stuck for HOURS — NOT an outage/billing.
  ~15 gui-smoke jobs (no `timeout-minutes`) hung to the 6h max holding every account concurrency slot, starving
  the queue. Cancelled the hung+stale runs; added `timeout-minutes:20` to both gui-smoke jobs + `cancel-in-progress`
  concurrency groups to ci.yml/gui-smoke.yml. A fresh run went in_progress within seconds → confirmed. LESSON: read
  ALL runs (the hung ones were OLDER than the recent-queued slice I first looked at), and every long CI job needs a timeout.
- **Thumbnails didn't render in the installed app → real end-to-end gap (CPE-1267):** backend decoders + bundled
  pdfium/ffmpeg were correct (proven by a throwaway example on the user's actual files), but the FRONTEND
  `hasThumbnail` gate (`src/lib/filetypes.ts` `THUMBNAIL_EXTRA_EXTS`) was never updated past CPE-1236 → the grid never
  requested pdf/video thumbnails. Compounded by a Windows case-collision: my first edit went to `fileTypes.ts`
  (camelCase) while the imported/tracked file is `filetypes.ts` (lowercase), so the "fix" never entered the build.
  Landed in the correct lowercase file, shipped in **v0.57.41-sidecar** (verified live by the user). LESSON: headless
  tests + reviews can't catch a frontend↔backend WIRING gap; and on Windows always edit/commit the exact tracked case.
- **CI greened after the outage (CPE-1268):** the whole sprint had merged via `--admin` during the stall, so CI's
  first real run was red across many jobs — thumb_video tiny-max_edge (real), pty kill-already-reaped ESRCH (real Unix),
  macro-via-trash (CI has no Recycle Bin → probe-skip), macOS dead_code cfg-gate, pdfium install tar/cygpath +
  best-effort. PR #566: all 10 CI jobs green (verified), merged; main green across all 3 OSes.
- **Regression pin:** two-sided frontend↔backend `THUMBNAIL_EXTRA_EXTS` parity guard (vitest + a Rust thumb_source
  test) + a gui-smoke pdf/video render spec — the CPE-1267 drift can never silently recur.
- LESSON (process): merging via `--admin` during a CI outage defers, doesn't skip, verification — it all came due at once.

### 2026-08-03 — "do all of them in order": 4-item user queue COMPLETE
- User picked "do all 4 in order". Delivered sequentially, each gauntleted (worker + independent reviewer), all merged, main CI green:
  1. CPE-1271 sidecar-bundle resource guard (#568) — CI test asserts every runtime-resolved resource (icon/pdfium/ffmpeg/sidecars) is in the merged shipped sidecar bundle per OS; proven non-hollow by deliberate-fail. Closes the "works in test, broken in shipped build" class that bit thumbnails (CPE-1267) + drag-out (CPE-1270).
  2. CPE-674 archive extract-on-drag (#569) — Alt-drag a file OUT of an archive → extract_archive_entry_any → native drag.
  3. CPE-713/CPE-1272 tray-resident (#570) — Tauri tray icon + quick-access folder menu + show/hide + close-to-tray toggle (default off); reviewer caught + fixed a stuck-hidden hazard (hide gated on tray presence).
  4. CPE-976/CPE-1273 configurable real embedder (#571) — OpenAI-compatible HttpEmbedder (reused ureq, off-by-default feature) for content search; API key in OS keychain only; LM Studio (local, no key) or OpenAI; disabled path byte-identical.
- Assessment during item 3: CPE-712 shell context-menu + CPE-716 sidebar drive listing ALREADY shipped; the invasive remainders (OS default-file-manager registration, drive eject/removable) deliberately NOT built blind — flagged as explicit user opt-in.
- Shipped builds for attended verify: 0.57.43 (drag-out fix), 0.57.44 (tray + archive-drag). Item 4 needs a 0.57.45 build + the user's endpoint to try AI search.
- LESSON reinforced + now guarded: the sidecar-overlay build drops base array-form bundle.resources — caught twice by hand, now caught automatically by the CPE-1271 guard.

### 2026-08-03 (cont.) — "keep going" post-queue: content-search deepened, then safe well tapped
- After the 4-item queue, user said "keep going + remember we need to get back to [AI testing]". Saved a
  [[pending-attended-verifications]] memory. Continued with safe headless work:
  - CPE-1274 (#572) — content search now extracts+indexes PDF (bundled pdfium) + docx/xlsx/pptx (existing zip
    dep), not just plain text. char-safe cap, no new dep, 19 tests.
  - CPE-1269 (#573) — drag-out icon resolution hardened (never passes a relative icon to the plugin) + unit coverage.
- Cut 0.57.45 (adds the CPE-1273 configurable embedder) + installed for the user to enable AI search (LM Studio/OpenAI) — NOT yet tested by the user.
- WRAP: safe immediately-valuable headless well is tapped (queue + follow-ups done). Remaining is USER-GATED:
  AI copilot/auto-organize (CPE-977/979) + OCR (980) need a generative LLM / OCR engine (the user's model — LM Studio
  makes a configurable-LLM seam viable like CPE-1273 did for embeddings, but it's inert without their model);
  invasive shell (712 default-file-manager, 716 drive-eject) need explicit consent/hardware. Plus pending ATTENDED
  tests: AI search (v0.57.45), tray, archive-drag. Did NOT manufacture filler.

## 2026-08-03 — "3 sprints back to back": safety-scan vein + media-metadata codecs (14 shipped)
- Shipped: CPE-1279 (release single-draft + eject glyph), CPE-1281/1282/1283/1284/1285 (5 cpe-server safety
  scans: archive/zip-bomb, empty-dirs, orphan-sidecars, dangling-links, disguised-file sweep), CPE-1286
  (file_type +13 magic-byte formats), CPE-1287 (wire 5 scan commands + specta bindings), CPE-1293 (front-end
  File-Health model), CPE-1288 (EXIF write), CPE-1290 (IPTC read), CPE-1291 (XMP+WAV read), CPE-1289 (OGG write).
  Epics CPE-1002 + CPE-1000 (safety-scan vein) and CPE-725 (media codecs) advanced; all headless.
- Tuned defaults:
  - **cpe-server pure scan adapter (walk over a done pure core)**: sonnet, ~2-wide+ (5-wide held fine on 32
    cores), ~9-11m median, 0 retries. The disjoint-new-module + "no command/bindings, integration ticket does
    that" split kept the parallel front conflict-free — repeat it. Only shared surface = crates/server/src/lib.rs
    mod list (append, trivial).
  - **format codec (byte-exact write: EXIF/OGG)**: opus, ~15-33m, format-RISKY — always opus + an opus reviewer.
    Both write codecs had subtle correctness issues an opus reviewer caught/verified (EXIF data-loss; OGG CRC KAT).
  - **read codec (IPTC/XMP/WAV/file_type)**: sonnet + sonnet reviewer, fine.
  - **command-integration (lib.rs + bindings regen)**: sonnet; reviewer MUST re-run the bindings generator +
    `git diff --exit-code` for zero drift and check BOTH generate_handler! lists.
- Gauntlet earned it (2 catches, 0 escaped defects):
  1. CPE-1288 EXIF write silently DESTROYED GPS/DateTime/Orientation/Make (rebuilt IFD from only editable tags).
     Opus reviewer proved it with a probe test; 1 rework fixed it (re-emit all IFD/sub-IFD fields, override only
     editable). Lesson: a codec whose READER marks fields read-only must have its WRITER re-carry them.
  2. CPE-1291 XMP/WAV: no functional bug but reviewer required wiring new byte-parsers into the repo's
     parser_panic_safety.rs fuzz harness (CPE-1169, overflowing-length-field class). New byte-parser => add a
     `*_never_panics` entry there, non-negotiable.
- PROCESS SCAR (see [[admin-merge-conflict-deletes-branch]] memory): `gh pr merge --admin --delete-branch` on a
  CONFLICTING PR CLOSES + DELETES the branch WITHOUT merging (nearly lost the reviewed EXIF fix). ALWAYS
  `gh pr view <n> --json mergeable` first; if CONFLICTING, resolve locally (branch from PR head, `git merge main`,
  union-resolve, ff-land) and NEVER pass --delete-branch pre-confirmation. Parallel read-vs-write codec PRs on the
  same file (media_meta.rs) always conflict on imports+doc+dispatch arms → resolve to the UNION.
- Throughput: ~14 tickets, ~34 sub-agent runs, deep budget headroom (never near the 200 reset line). Main green
  throughout (real CI; GUI-smoke still times out at 20m systemically — pre-existing, unrelated).

## 2026-08-03 (run 2) — "another 3 sprints": streaming scans + robustness + write symmetry (9 shipped)
- Shipped: CPE-1294/1295/1296 (streaming walkers for the mismatch/dangling/orphan tree-sweeps —
  flush-callback over ipc::Channel per listing.rs), CPE-1297 (close parser_panic_safety gap:
  iptc/exif-write/ogg-write/vorbis-write), CPE-1298 (write_wav RIFF LIST/INFO), CPE-1299 (wire 3 stream
  commands + 3 cancel + bindings), CPE-1300 (post-audit bug sweep — ALL CLEAN, honest, +1 regression test),
  CPE-1301 (write_pdf incremental /Info update), CPE-1302 (exclude-glob support for the 4 tree-scans +
  shared glob matcher). Epics CPE-1002 + CPE-725.
- PM verdict up front: headless well "real but shallowing — ~2 solid shifts, thinning to a 3rd of lower
  value." Held to it: did the 8 solid tickets + 1 genuinely-useful shift-3 item (exclude-globs); SKIPPED the
  flaky/low-value perf-smoke (T9) rather than manufacture filler.
- Tuned defaults CONFIRMED from run 1:
  - **streaming-walker refactor (flush-callback + collect-to-vec wrapper)**: sonnet, disjoint single-file,
    ~9-13m, 0 retries. Reviewer must confirm collect-to-vec parity byte-identical + Break + no-empty-flush.
    Design nuance to respect: dangling-links can't stream incrementally (is_cyclic needs the whole link set)
    → walk-to-completion-then-batch is correct, not a missed optimization.
  - **format codec (write_wav/write_pdf)**: sonnet for WAV (RIFF is simple), OPUS for PDF (xref/trailer
    arithmetic) + opus reviewer. write_pdf: incremental-append (prefix property) is the safe approach; refuse
    xref-stream PDFs with an honest Err rather than emit a broken table.
  - **stream/command integration (lib.rs + bindings)**: sonnet; reviewer re-runs export_bindings +
    `git diff --exit-code` for zero drift AND checks BOTH generate_handler! + collect_commands! lists.
  - **audit ticket**: opus; an all-clean result is a valid honest outcome (don't manufacture a fix). Reviewer
    should EMPIRICALLY spot-check (mutate a slice → confirm the regression test catches it).
- Gauntlet: 0 escaped defects, 0 retries this run (vs 1 catch-and-fix in run 1). Reviewers were rigorous —
  one wrote a throwaway test to prove stream-cancel actually breaks the walk; one mutated a slice to prove an
  audit regression test was real; one validated the dangling-stream design with a concrete misclassification
  example. This depth is why nothing bounced.
- PROCESS: applied the run-1 lesson ([[admin-merge-conflict-deletes-branch]]) — checked `gh pr view --json
  mergeable` before every --admin merge; media_meta.rs / media_meta_read.rs union-conflicts (parallel
  read-vs-write codec PRs) resolved locally via branch-from-PR-head + `git merge main` + ff-land, never
  --admin-on-conflict. Zero lost work this run.
- Throughput: 9 tickets, ~34 sub-agent runs, deep budget headroom. Main green throughout (real CI).
- HONEST STATE AFTER 2 RUNS: the clean headless well is now essentially tapped. Remaining frontier is
  GUI/attended (File-Health panel UI, metadata-edit UI, scan-exclude UI, near-dup review) or user-gated
  (AI model/key, OCR engine, cloud/SFTP creds, code-signing cert, Mac). Next session should either take an
  attended/GUI epic WITH the user, or get a user resource — not scrape for more headless filler.

## 2026-08-04 (run "6 sprints") — media-metadata write-back completion + robustness (10 shipped)
- Shipped: CPE-1304 (perf-budget harness), 1305 (IPTC/XMP write-back), 1306 (Linux shell integration), 1307
  (macOS xattr OS-interop test — retired MVD row5; was mis-numbered CPE-828), 1308 (media-meta polish:
  EXIF clear-symmetry + IPTC 1:90 UTF-8 charset + 8BIM survivor), 1309 (write_mp4 video metadata + iso_bmff
  refactor), 1311 (binary/data preview panic coverage), 1312 (folderWatch data-integrity bug), 1313 (IIM
  mid-codepoint truncation bug), 1314 (write-codec panic harness). **Epic CPE-725 write-back COMPLETE (9 formats).**
- Gauntlet: **0 escaped defects**. 4 tickets bounced-and-fixed IN-gauntlet before merge — independent UAT caught
  2 real photo-corruption-adjacent bugs in 1305 (camera-JPEG save fail + silent clear no-op) and an
  unreachable-feature bug in 1306; the 1307 macOS test hit a lossy xattr readback on the real runner; an opus
  re-verify caught an i18n coverage-gate break (fixed by the Foreman directly). This depth is why nothing bounced.
- Tuned defaults CONFIRMED:
  - **JPEG-segment codec (write_iptc/xmp/exif)**: opus build + opus reviewer (photo byte-surgery). Dispatch must
    key on EDITED-groups not field-presence; strip the segment on last-field-clear; reviewer checks insert-vs-
    replace BOTH paths + EXIF/XMP APP1 disambiguation (Exif\0\0 vs the xap URL).
  - **MP4 atom write (write_mp4)**: opus build + opus reviewer. The write_pdf-style copy-moov → append at EOF →
    shadow old moov "free" strategy (NEVER move mdat / touch stco/co64) is the ONLY safe approach. Load-bearing
    test = re-derive stco/co64 offsets from the REWRITTEN file → deref to mdat bytes (a tag-only round-trip
    MISSES silent playback corruption). Plan filed in Library (mp4-metadata-writeback-plan-2026-08-04).
  - **cross-OS-cfg tickets (Linux/macOS)**: the worker can't run the cfg arm locally → `cargo check --target
    <triple>` (a zig-cc wrapper handles the C build-scripts: rusqlite/lzma-sys) is the local proof; real gate is
    the CI other-OS leg; merge only after PR CI green. Structure the pure logic OS-agnostic + unit-test it.
  - **test-only PRs (panic/coverage)**: one Reviewer + the 3-OS CI run AS the 2nd independent check (third-party
    parsers can panic per-OS) — proportionate, saves a UAT spawn.
  - **Foreman-applied tiny fixes**: the i18n coverage-gate needs keys in ALL 12 locales (es/de/fr/it are all
    COMPLETE_LOCALES) — Foreman-applied directly, saving a worker round-trip.
  - **ID hygiene**: verify next-free ID with a RECURSIVE find incl. Done/ (bash `**` doesn't recurse without
    globstar) — a mis-numbered CPE-828 collision cost a rename pass (→ CPE-1307).
- Throughput: 10 tickets, ~53 sub-agent runs, budget deep headroom (never near the 150 reset line). main green
  throughout (real 3-OS CI). Merges gated on PR CI green via background pollers (offsite Actions).
- HONEST STATE: clean headless well DRY after this run (3 independent sweeps agree). Remaining = attended-GUI or
  user-gated (model key / Mac / signing cert / SFTP / Docker). Next session should take an attended epic WITH the
  user or get a resource — do NOT scrape filler.

## 2026-08-04 (run 2, cut short by weekly usage limit) — File-Health panel GUI (2 slices shipped)
- Pivoted to GUI features (backends already built) via the jsdom-logic-test + gui-smoke-spec + (owed) Visual-Critic
  loop, since the headless well was dry. Shipped CPE-1315 (panel shell + dangling stream tab) + CPE-1316
  (mismatch + orphan stream tabs, per-scan-isolated). Slice 3 (CPE-1317) died mid-build on the weekly limit (no PR).
- Streaming-consumer UI pattern (rawInvoke+createChannel+per-scan generation token+cancel-prior+late-batch-drop+
  finally-clears-loading) is now proven for the File-Health panel — reuse it for remaining scan tabs.
- OWED: a real build → gui-smoke → Visual-Critic pass across the panel (streamed-Channel end-to-end only jsdom-
  mocked so far; low risk, proven-by-identity to SimilarImagesDialog).
- LESSON: Janitor must never rm -rf worktree globs while a worker is active (clobbered a live worker; recovered).

## 2026-08-05 (run 2 cont.) — File-Health GUI epic COMPLETE (8 slices, Visual-Critic-verified)
- After the headless well ran dry (run 1) + a weekly-limit reset, user "continue" → surfaced the built-but-unshipped
  file-inspection-safety scans in the GUI: CPE-1315..1322 (4-scan File-Health panel + archive-safety dialog + visual fixes +
  corrupt-zip signal + mismatch rename-to-correct-ext). All merged, Frontend/backend CI green, 0 escaped defects.
- **GUI-via-Visual-Critic loop proven:** build app → gui-smoke screenshots (streaming Channels proven over live IPC) →
  taste-aware Visual Critic caught 2 real layout defects markup-review missed → fixed → re-screenshot PASS. No user round-trip.
  This is the template for doing GUI epics autonomously.
- Tuned defaults for GUI slices:
  - **streaming-consumer UI**: rawInvoke + createChannel + PER-SCAN generation token + cancel-prior + late-batch-drop +
    finally-clears-loading (empty-result edge). Each scan needs its OWN gen counter/cancel (cross-tab isolation test).
  - **non-streaming tab**: plain `invoke` from src/lib/invoke.ts (busy-cursor), stale-response guard via a gen counter.
  - GUI slices SERIALIZE on shared wiring files (FileHealthDialog.svelte, App.svelte menu/palette arrays, i18n.ts) — build
    one, merge, next. i18n = ALL 12 locales or i18n.test.ts fails. sectionDocs.ts + a src/docs page for new sections.
  - Merge frontend-only PRs on the Frontend CI job (backend legs irrelevant); backend/bindings PRs need the 3-OS + drift guard.
  - GUI-verifier sub-agents MUST run the tauri build + gui-smoke SYNCHRONOUSLY (foreground) — one stalled by backgrounding
    the build + yielding (no bg notifications for sub-agents); resume via SendMessage telling it to run synchronously.
  - `overflow-x:hidden` on a flex-wrap results list kills a spurious horizontal scrollbar that clips tall rows.
- Throughput run 2: 8 GUI tickets, ~50 sub-agent runs (heavier: build+gui-smoke+Visual-Critic per epic). Budget ~114/200 at
  clean checkpoint. LESSON: never rm -rf worktree globs while a worker is live (clobbered one; recovered).

## 2026-08-05 (run 3) — GUI "backends-exist" sweep: 7 tickets, 4 epics, 0 retries, 0 escaped
User: "start three sprints back to back". Headless well long-dry, so continued the proven GUI-via-Visual-Critic
template (build → gui-smoke screenshots → taste-aware Critic, no user round-trip). Shipped 7 FE-only tickets,
each through the full Reviewer+UAT gauntlet, all merged on Frontend CI green, **zero rework, zero escaped defects**:
- **CPE-1323** File-Health exclude-glob input UI (backend CPE-1302 existed; frontend hardcoded `excludes:[]`). #619
- **CPE-1324** NearDuplicatesDialog keeper-guarded move-to-bin (parity w/ SimilarImagesDialog; reused duplicates.ts). #620
- **CPE-1325** Metadata Studio checkpoint-before-save (best-effort, non-blocking). #621
- **CPE-1326** Metadata Studio batch strip / copy-from-first — worker also found+fixed a latent Svelte reactivity
  bug (`currentValue(f)` closure defeated static dep-tracking; now passes `edited` explicitly). #622
- **CPE-1327** Metadata Studio per-field revert + reset-all (pure client-side, no write/checkpoint). #623
- **CPE-1328** truthful checkpoint-status bugfix — un-unwrapped `checkpointCreate` swallowed `Err(String)`
  (`{status:"error"}` doesn't throw) → "(checkpoint saved)" could lie. Fixed via `unwrap()` in 3 dialogs; also
  fixed 2 double-wrapped test mocks + a dead-code test. Both reviewers independently surfaced the pattern. #624
- **CPE-1329** NEW Declutter junk-review dialog (epic CPE-979) — surfaces the built-but-unwired `organize_clutter`
  engine; safe move-to-bin, applied the CPE-1328 unwrap lesson; new `declutter.smoke.ts` spec. #625

Tuned defaults / lessons (seed next GUI shift):
- **GUI slices on a shared component SERIALIZE** (MetadataStudioDialog carried 1325→1326→1327→1328). i18n.ts is
  append-only so parallel branches auto-merge (fh.*/nd.*/studio.* didn't collide); the merge lock rebases fine.
- **`unwrap()` (src/lib/invoke.ts) is mandatory on `checkpointCreate`** — the generated binding only throws on an
  `Error` instance, so a Rust `Err(String)` resolves `{status:"error"}` silently. Any new best-effort command call
  that gates a success message must unwrap or it will lie. (CPE-1328.)
- **Function-call closures defeat Svelte's static dep-scan** — `currentValue(f)` didn't re-run on programmatic
  `edited` changes; pass state explicitly (`currentValue(f, edited)`) + reassign (`edited = {...}`). (CPE-1326.)
- **Worktree base lag:** workers branched before an untracked Backlog ticket file existed → recreated/didn't-touch
  it inconsistently. Foreman reconciled centrally (move Backlog→Done after merge). File the ticket + commit it, or
  accept workers won't see it.
- **Batched Visual Critic works:** one build screenshots the resting state of multiple merged surfaces; Critic
  judged VISUAL PASS. OWED: specs capture only resting state (no filled exclude-pill / enabled Move-to-Bin) —
  add post-interaction snaps. MetadataStudioDialog has NO gui-smoke spec (opens on media selection, not palette).
- **Frontier: TAPPED after CPE-1329** (survey + [[clean-gui-vein-tapped-after-declutter-2026-08-05]]). Remaining =
  NEEDS-BACKEND (audio/video decode, unix driveType) or USER-GATED (AI classifier/model key, Mac, signing cert,
  SFTP, Docker, removable-drive hardware). Next shift: take an attended/backend epic WITH the user, don't scrape filler.
- Throughput: 7 tickets, ~46 sub-agents, budget deep (~46/200, never near reset line). Median gauntlet ~7m,
  0 retries, 0 escaped defects. main green throughout (real 3-OS CI; FE-only PRs gated on Frontend job).

## 2026-08-05 (run 3 cont.) — "keep going" ×2: QA/docs debt + 3D-model feature (6 more tickets, 14 total)
After the GUI vein tapped, user "keep going" → closed real debt + opened the backend frontier:
- **CPE-1331** gui-smoke coverage: new metadata-studio.smoke.ts (byte-accurate ID3v2.3 seed) + filled-exclude-pill +
  enabled-Move-to-Bin interactive snaps — all Visual-Critic PASS. Metadata render gap RETIRED. (#629)
- **CPE-1332** in-app docs pages for Declutter/Near-Duplicates/Metadata-Studio (CPE-579 gap). Reviewer caught 2
  wrong menu labels ("Find clutter"→"Declutter", File-menu→context-menu) → Foreman-applied fixes. (#627)
- **3D-MODEL FEATURE (epic CPE-118, complete):** CPE-1333 pure-Rust STL/OBJ geometry reader (model_3d.rs, zero
  deps, 20 inline tests incl. malformed→None, Reviewer panic/DoS-clean) (#628) · CPE-1335 glTF+GLB parsing
  (serde_json, bounds-checked, honest tri/vertex=0) (#631) · CPE-1334 preview-pane info section wiring
  readModelInfo (stale-guard, "Faces" for OBJ) (#630) · CPE-1336 glTF rendering (format label + Meshes row, no
  bare "0 triangles", zero-bbox dim suppression) (#632).
- Escaped defect: CPE-1329's declutter.smoke.ts asserted on the wrong DOM node (spec type-checked, not run) →
  caught by the GUI-verifier build, fixed + verified-green as CPE-1330. LESSON: gui-smoke specs must be RUN at
  build/review, not just type-checked.
- Tuned defaults: backend format-reader = pure-logic in crates/server + INLINE-byte-array fixtures (no committed
  binaries) + regen bindings via `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"`.
  Backend PRs gate on 3-OS + drift guard; Windows/Ubuntu server-crates legs run ~20-26min (be patient, don't
  strong-signal-merge then rapid-push bookkeeping — the concurrency group cancels the in-flight CODE CI run,
  producing a scary-looking "X cancelled" that is NOT a failure).
- Throughput: 14 tickets (CPE-1323-1336), 7 epics, ~110 sub-agent runs, 0 escaped defects open, budget ~108/200.

## FRONTIER (2026-08-05, after run 3) — CLEAN AUTONOMOUS VEIN EXHAUSTED
Two GUI surveys + one backend survey + building the one clean backend slice (3D reader) agree: no clean,
locally-cargo/gui-verifiable, no-user-resource work remains. Remaining candidates are gold-plating (glTF
accessor-deref for real tri/vertex counts — deliberately deferred, low value) or USER-GATED (AI model key,
Mac, signing cert, SFTP/Docker/removable-hardware, or a NEEDS-BACKEND/heavy-dep format reader HEIC/DICOM/RAR).
Library: [[clean-gui-vein-tapped-after-declutter-2026-08-05]], [[clean-backend-vein-3d-reader-2026-08-05]].

## 2026-08-05 (run 3, keep-going ×3) — 3D reader rounded out to 5 formats (4 more tickets, 18 total)
User kept re-issuing "keep going" → extended the shipped 3D reader with genuinely-clean, zero-dep, cargo-testable
format work (the incremental format-support vein the app has always mined):
- **CPE-1337** PLY (Stanford Polygon Format) reader — 4th format; header declares vertex/face counts; ASCII bbox
  computed, binary bbox honestly zeroed. FileType::Ply exact magic. (#633)
- **CPE-1338** folded `read_model_info` into the cross-cutting `parser_panic_safety.rs` fuzz battery (was a gap
  the CPE-1337 reviewer flagged) — 3D parser panic-safety now pinned. (#634)
- **CPE-1339** real glTF/GLB vertex+triangle counts from accessor `count` fields (no buffer deref needed after
  all) — mode-aware (TRIANGLES n/3, STRIP/FAN n−2, points/lines 0). Replaced the honest-0 placeholder. (#636)
- **CPE-1340** frontend PLY rendering (MODEL_EXTS + "PLY" label + "Faces" count). (#635)
- INCIDENT (recovered, lesson reinforced): CPE-1339's Backend CI failed ONLY on the Typed-bindings drift guard —
  the worker edited the `ModelInfo` field DOC COMMENTS (a specta::Type), which regenerates bindings.gen.ts even
  though the shape was unchanged; worker wrongly concluded "no regen needed". Foreman regenerated + pushed → green.
  REINFORCES [[regen-specta-bindings-on-struct-change]]: regen after ANY edit to a specta::Type, INCLUDING comments.
- Net: 18 tickets this session (CPE-1323-1340), 8 epics. 3D reader = STL/OBJ/glTF/GLB/PLY, honest counts, full UI,
  fuzz-pinned. Budget ~136/200 at checkpoint (approaching the ~150 reset line — hence this clean hand-off).

## FRONTIER (2026-08-05, after 3D lane) — clean vein down to heavy-dep/gold-plating
The 5 common mesh formats are done. Remaining CLEAN incremental candidates are thin: more file_type magic
signatures (fonts ttf/otf/woff — small, real, zero-dep — the ONE remaining clean-ish slice), or gold-plating.
Everything of scale is USER-GATED (AI model key, Mac, cert, SFTP/Docker/hardware) or NEEDS a heavy/licensed dep
(HEIC/DICOM/RAR/camera-RAW). See [[clean-backend-vein-3d-reader-2026-08-05]], [[clean-gui-vein-tapped-after-declutter-2026-08-05]].

## Shift 2026-08-05 (resume) — 4 tickets, file-type correctness lane (epic CPE-1000)
Resumed the sprint; fresh session, full budget. Checkpoint said the last clean slice was "font signatures"
but they were already done — reading file_type.rs surfaced 2 real items instead, and a mid-shift evidence-based
frontier scan surfaced 2 more. Shipped 4, all gauntlet (Reviewer+UAT) + CI-green on 3 OS, 0 escaped defects:
- CPE-1341 (#637) ftyp-brand disambiguation — real false-positive bug (mov/heic/avif/3gp flagged as MP4 mismatch).
- CPE-1342 (#637) +11 magic signatures (tar/psd/cab/icns/ar/aiff/midi/flv/cur/lz4/lzip).
- CPE-1343 (#638) type_mismatch_scan HEADER_CAP 64→512 — TAR@257 was unreachable by the tree-sweep (a latent
  gap CPE-1342 exposed). LESSON: new offset-based signature → verify HEADER_CAP/column caps can read that deep.
- CPE-1344 (#639) OLE2/CFBF container signature (legacy doc/xls/ppt/msi/msg/vsd), were invisible to mismatch.
Tuned defaults: file_type-sniffer class = sonnet, ~11m gauntlet median, 0 retries, 0 stuck; parallelize non-
colliding tickets by FILE (1343 type_mismatch_scan.rs ∥ 1344 file_type.rs). Windows Server-crates CI ran ~30m
(cold runner, both feature modes) — expect the long tail; Backend-windows is green much earlier on the same crate.
NOTE: gauntlet agents run withOUT worktree isolation leave scratch (pr637.diff) in the MAIN tree — Foreman
git-rm'd it; consider isolating reviewers too or a targeted add. FRONTIER: signature vein now TAPPED (Library
`file-type-signature-vein-tapped-2026-08-05`); only DDS/EOT left = gold-plating, skip. Next work needs the user.

## Shift 2026-08-05 (resume, cont.) — GATED FORMAT-READER PROGRAM COMPLETE (13 tickets)
User: "you pick and if you do all of it that would be fine" + "keep trying" → took the 4 long-Blocked
format-reader epics (HEIC/DICOM/RAR/camera-RAW). Vetted licensing first (Library
`gated-format-readers-dicom-raw-rar-2026-08-05` + `heic-preview-platform-apis-2026-08-05`), then shipped:
- Backends: CPE-1345 DICOM (dicom-rs, feature-gated), CPE-1346 camera-RAW embedded-JPEG (0 deps), CPE-1347
  RAR4/RAR5 listing (0 deps, no UnRAR). CPE-1341-1344 file-type detection earlier same session.
- Wiring: CPE-1348 RAR into archive browser; CPE-1349 RAW preview provider; CPE-1350 DICOM provider + SHIP
  (dicom-thumb enabled in app build, user-approved +2.81 MiB); CPE-1351 HEIC via per-OS platform APIs
  (Windows WIC — real decode verified on this machine; macOS ImageIO cfg-gated + CI-compiled).
- Polish: CPE-1352 trim DICOM ship-cost (drop dicom-pixeldata image feature → exr/pnm gone, 315->307 crates);
  CPE-1353 fix a YCbCr green-term SIGN BUG (we now render color Doppler more correctly than upstream dicom-pixeldata).
GAUNTLET CAUGHT (all fixed within-shift, 0 escaped defects): RAR integer-overflow hang (checked_add), HEIC
macOS objc2 0.3 deprecations under -D warnings, HEIC huge-dimension buffer guard, DICOM silent YBR wrong-color
regression, then the deeper upstream YCbCr sign bug. Tuned defaults: format-reader class = sonnet (opus for the
unsafe WIC FFI worker+reviewer); provider-wiring shares src-tauri/lib.rs + bindings.gen.ts + provider.ts →
MUST sequence (branch each off the prior merge), can't parallelize. HEIC needs OS HEIF extension on Windows
(graceful Err fallback). ~86 sub-agents this session. FRONTIER: program complete; remaining = attended visual
(build->deploy->run; macOS on a Mac) — no clean headless work left in this lane.

## Shift 2026-08-06 (resume, cont.2) — POST-PROGRAM HARDENING (CPE-1354/1355), 15 tickets total
After the format-reader program, a broad frontier scan (evidence-based, NOT trusting "dry") found 3 real items:
- CPE-1354 (HIGH): the shipped `dicom-thumb` feature was NEVER run by CI `cargo test`/clippy — so the DICOM
  tests incl. the CPE-1353 YBR regression were invisible. Added dicom-thumb to ci.yml server job + put
  rar/dicom/camera_raw into the panic-safety fuzz battery. Lesson: when a feature SHIPS, grep ci.yml to confirm
  its tests actually RUN there.
- CPE-1355: real Linux drive-type classification (was hardcoded "fixed"). Pure classify fn (testable on Windows)
  + linux-cfg /proc wrapper (CI-compiled on ubuntu). Gauntlet caught the unpartitioned whole-disk nvme/mmcblk
  reduction bug (nvme0n1->nvme0n). CPE-1355 (Linux drive-type) filed CPE-1355; gui-smoke fixtures for the 4 new
  preview types (#4) left as heavier-infra follow-up (not filed — needs tauri-driver pipeline effort).
Session total: 15 tickets (CPE-1341-1355), main GREEN throughout, 0 escaped defects. Gauntlet caught 5 real
bugs pre-merge (RAR overflow-hang, HEIC macOS deprecations, HEIC dim-guard, DICOM YBR wrong-color, DICOM upstream
sign bug, drive-type whole-disk reduction). ~105 sub-agents. FRONTIER: clean headless vein tapped again — remaining
is attended visual (build->deploy->run; a Mac) or the heavier gui-smoke-fixtures infra.

## Shift 2026-08-06→07 (CLI, ~19:30–02:00) — DUAL-PANE EPIC COMPLETE + QA-AUTOMATION BATCH — 19 PRs, 0 escaped defects
User: "run 10 sprints back to back." Started with a 5-ticket backlog (dual-pane/selection bugs); it opened a
huge vein. Delivered **19 PRs merged, main GREEN throughout, 0 escaped defects**, ~140 sub-agents.

**Program 1 — Dual-pane commander pane-B FULL PARITY (epic CPE-617), 11 PRs (CPE-1370–1388):** pane B went from
mouse-only second-class to first-class across keyboard-nav, bulk-select scroll, PageUp/Down, cross-pane DnD,
display props (search/filter/tags/cut/sizes), context menu, inline rename, custom columns, Home actions+nav,
clipboard copy/cut/paste, and the whole bulk-op + archive/vault family (duplicate/batch-rename/media/copy-to/
move-to/compress/extract/shred/vault) — all pane-routed with the CPE-1370 `snapshotConfirmTarget` safety model
for every destructive op. Reusable patterns: `pickActivePane`/`activePaneState()` (keyboard, live activePane),
`paneStateFor(ctx.inPaneB)` (context-menu, menu-open-time), `snapshotConfirmTarget` (destructive), and
`refreshDropSourcePane`/`refreshPasteAffectedPanes`/`refreshBatchApplyTarget(dir)` (both-panes refresh, no ghost).
GAUNTLET CAUGHT 4 REAL BUGS PRE-MERGE (2 data-loss): (1) Tab-mid-confirm permanent-delete hit the WRONG pane
(CRITICAL); (2) drag-move ghost row when both panes mirror a folder; (3) empty-area New-Folder created in pane A
from a pane-B menu; (4) cut+paste-fail silently lost the clipboard. All fixed within-shift.

**Program 2 — QA-automation render-specs (MVD burndown), 7 PRs (CPE-1389–1395):** frontier scan (rigorous
re-verify of all 34 epics) confirmed the clean FEATURE well DRY → pivoted to jsdom render-specs (locally
vitest-verifiable, NOT flaky gui-smoke). QA-Architect (opus) found the real gap = shipped App-wired dialogs with
ZERO coverage: Integrity, RunCommandConfirm (external-process safety gate), ConflictDialog, DataBrowser,
CompareDialog, SessionHistoryDialog, SyncDialog. All pinned with the `vi.mock("@tauri-apps/api/core")` +
@testing-library/svelte recipe (mock the CORE invoke seam, since bindings.gen's TAURI_INVOKE re-exports it). These
are PARALLEL-SAFE (independent new test files) → ran 4-wide fan-out. Specs surfaced 2 real UI bugs → fixed
(CPE-1396 ConflictDialog error/empty-state clash, CPE-1397 DataBrowser missing space, PR #674).

Tuned defaults: render-spec class = sonnet, ~1 review each (test-only → single-reviewer gauntlet, reviewer does
its OWN mutation-check), parallel-safe; pane-B chain = serial on App.svelte, bundle same-block tickets. LESSON:
test-only + independent-file work parallelizes wide; App.svelte work serializes hard. Every merge Reviewer+UAT
(or single-reviewer for test-only) gauntlet-verified + Frontend-CI-green; GUI-smoke is non-gating flake (merge on
UNSTABLE-not-BLOCKED). FRONTIER after this shift: clean headless well DRY again (Library
`headless-well-dry-post-dualpane-2026-08-07`) — next FEATURE work needs the user (attended GUI, Mac, signing cert,
AI keys, SFTP/Docker creds). Low-value remainder: QA-Architect 2nd-tier render-specs (BackupDashboard etc.) +
follow-ups CPE-1385(done)/1386(done); open follow-ups: none in backlog (all closed).

## Shift 2026-08-07 (CLI, resume ~04:50–06:30) — SECURITY + COVERAGE HARDENING — 8 PRs, found a REAL DoS
User: "start 8 sprints back to back" (after the dual-pane+QA shift confirmed the feature well dry). A scout
re-verified: integration-bug vein is genuinely THIN (dual-pane program drained it, seams already guarded w/
citing comments); real value in Vein C (security-hardening) + Vein B (component coverage). Delivered 8 PRs
(CPE-1398–1406), main GREEN throughout, 0 escaped defects, ~40 sub-agents.

**Headline — a REAL DoS found+fixed (CPE-1398, #678):** adversarial panic battery for WebDAV `parse_multistatus`
(untrusted network XML) found that deeply-nested XML (~500 levels, few-KB payload) triggers an UNCATCHABLE
`STATUS_STACK_OVERFLOW` process crash (roxmltree recurses per level) — a genuine DoS from a malicious/buggy
WebDAV server. GAUNTLET THEN CAUGHT THE FIRST FIX'S OWN BYPASS: a quote-unaware `>`-scan let `<a b="/>">`
misclassify a real open tag as self-closing → guard evaded → crash reproduced by the reviewer. Robust v2 fix:
count depth via `xmlparser::Tokenizer` (roxmltree's non-recursive lexer ancestor, added as direct dep — vetted:
0 transitive deps, MIT/Apache, v0.13.6 carries xmlparser's own recursion fix), cap lowered 128→64. Re-reviewer
built 9 divergence-attack payloads on small-stack threads, couldn't crash it. LESSON: static "no panics found"
≠ safe; only adversarial fuzzing on the real recursive parser found it; a security guard needs a re-review that
TRIES to evade it (weaken the gate → prove the battery catches forgery).

**Other security battery:** CPE-1399 (#677) JWT `HmacJwtVerifier::verify` fuzz battery (~40 adversarial cases:
alg-confusion/`alg:none`/tamper/splice/wrong-key + a valid-token positive control so it can't pass by
rejecting-everything). Reviewer weakened BOTH the signature gate and the alg gate → battery caught each. No
production bug found (verify is sound), but now has adversarial regression coverage.

**Coverage specs (Vein B) — each FOUND a real (minor) UI bug:** CPE-1400 WatchRulesDialog (Add-btn not gated on
condition validity → silent no-op; fixed in CPE-1402), CPE-1401 FileNameSearchDialog (gen-token supersede — no
bug), CPE-1404 DiskSpaceView (cache+gen-token+refreshToken — no bug), CPE-1405 ColorRulesDialog (SAME Add-btn
validation bug → CPE-1407 filed), CPE-1406 SidecarManager (failed-repair renders "Repaired: …failed" → CPE-1408
filed). Pattern: jsdom render-specs on shipped-but-untested dialogs keep surfacing real bugs — genuine value,
not busywork.

Tuned defaults: security-Rust batteries = sonnet + a re-review that ADVERSARIALLY tries to evade the guard (the
single most valuable check — it caught the DoS-fix bypass AND would catch a verify regression); render-specs =
sonnet, single-reviewer-with-own-mutation-check, parallel-safe (independent files, fan out wide). FRONTIER after
this shift: the genuinely-valuable vein is now down to minor follow-up FIXES (CPE-1407/1408) + the truly-thin
UserCommands/archive-pin specs. Feature work still user-gated. Session ~135 agents used → checkpointed for a
fresh-session reset. Two shifts this session = 27 PRs total (dual-pane epic + QA render-specs + security/coverage
hardening), 0 escaped defects.

## Shift 2026-08-07 (CLI, resume cont. ~06:56–08:30) — UNTRUSTED-PARSER SECURITY SWEEP — found 4 REAL DoS/hang bugs
User: "keep working, back in an hour" (×2) after the coverage vein drained. Pursued the ONE genuinely-valuable
unscouted vein: adversarially fuzz every parser of UNTRUSTED bytes (opened files / network) — the vein that found
the WebDAV DoS. It paid off big. 9 PRs this segment (CPE-1407/1408 fixes + 1409/1410 coverage from the prior
segment, then CPE-1411/1412/1413/1416 security). main GREEN throughout, 0 escaped defects.

**FOUR real DoS/hang bugs found by fuzzing (static review had missed ALL of them):**
- WebDAV `parse_multistatus` deep-XML stack-overflow (CPE-1398, prior segment) — FIXED.
- SVG `thumb_svg.rs` deep-nesting stack-overflow (CPE-1413) — FIXED (quote/comment/CDATA/PI-aware non-recursive
  depth guard MAX=64, run before usvg; reviewer built 6 evasion payloads incl. the webdav quote-bypass shape —
  none bypassed). Worker learned from CPE-1398 and made it quote-aware from the start.
- SVG mutual `<use>` reference-cycle stack-overflow (CPE-1414) — DEFERRED (safe on prod 2MB spawn_blocking stacks,
  low risk; needs a fragile non-recursive cycle detector; `#[ignore]`d reproducer).
- ISO `archive.rs:iso_entries` malformed-record INFINITE-LOOP HANG (CPE-1411) — FIXED (`continue`→`break`;
  iso9660 0.1.1's iterator doesn't advance on parse error).
- sevenz-rust 0.6.1 crafted-.7z overflow panic (CPE-1415) — reported; CONTAINED (spawn_blocking task boundary →
  Err, no crash; no panic=abort). catch_unwind mitigation = low-pri follow-up.
- Font glyph (CPE-1412, ab_glyph SFNT/glyf) — fuzzed ~250 cases, NO bug (held up).
- Wire `read_envelope` unbounded read → memory-DoS (CPE-1416) — FIXED (`.take(16 MiB)` cap; reviewer mutation
  showed the old path silently decoded a truncated frame).

Tuned defaults: security-parser batteries = sonnet worker + a reviewer that ADVERSARIALLY tries to evade the fix
(the single highest-value check — caught webdav's bypass, verified SVG's guard). Probe stack-overflow on a 256KiB
`std::thread` (uncatchable by catch_unwind — a failed .join() is the detector). Guard recursive-parser DoS with a
non-recursive depth pre-scan. Library entry: `untrusted-parser-fuzz-sweep-2026-08-07` (coverage map + lessons).
FRONTIER: this vein now COVERED (archive/svg/font/webdav/jwt batteries); remaining = 2 low-pri follow-ups
(CPE-1414/1415) + user-gated feature work. Session TOTAL across all segments = 35 PRs, 0 escaped defects, 5 real
security bugs found (4 fixed).

## Shift 2026-08-07 (CLI, "Do 3 sprints") — 8 PRs merged, epic CPE-1433 closed, feature well confirmed dry
Shipped: CPE-1432 pane-aware quick-look (#701), CPE-1415 sevenz catch_unwind (#702), CPE-1427/1428 cert
hardening (#703), **epic CPE-1433 structured previews** — .eml (#704) + .ics/.vcf (#705), CPE-1438 dual-pane
crypto Inspect overlay (#706), CPE-1440/1441 security dep bumps (#707: quick-xml High DoS + dompurify XSS).
Parked CPE-1414 (SVG use-cycle guard) after the 3-attempt circuit-breaker — adversarial reviewer found a real
256KB-stack bypass on EACH attempt (entity-encoded href → DTD-entity → xlink:href precedence); low real risk
(prod 2MB stack safe), exact remaining fix documented. Filed CPE-1437/1439/1442/1443 (deferred/blocked follow-ups).

Tuned defaults / lessons:
- **Adversarial security re-review is the highest-value leg** — caught the SVG guard bypass 3× AND the #707
  lockfile-propagation gap. A security guard needs a reviewer that TRIES to evade it (re-run per fix).
- **Repo has MULTIPLE independent Cargo.locks** (no workspace root): a dep bump in crates/server must ALSO
  regenerate + commit `src-tauri/Cargo.lock` (the shipped-app lockfile) — cargo audit/build from crates/server
  alone MISSES the shipped binary. New memory: [[multiple-independent-cargo-locks]].
- **Verify epics against CODE, not briefs** — repeatedly found "candidate" epics already fully built (format
  readers, checkpoint-rollback, file-type detection, activity-replay/cost-dashboard/conflict-radar all wired).
- **Structured-preview template** (jwt_preview.rs → command → Svelte + jsdom, provider before text) = the proven
  shape for new file-type viewers; hand-rolled zero-dep parsers + a panic-safety battery each.
- Model mix: sonnet workers/reviewers for most; opus for the adversarial SVG review + the epic scouts (costly-if-wrong).
- **FRONTIER: clean headless FEATURE well is DRY** (2 independent scouts + drained queue). Next increment needs
  the USER: attended GUI punch-list, macOS (no tauri-driver path), signing cert (CPE-002), live agent session
  (CPE-1098), 3D viewer (CPE-118), real-network E2E (CPE-819/820). Only headless left = low-value CPE-1414/1437
  SVG hardening. See Library [[structured-preview-runway-2026-08-07]].

## Shift 2026-08-07→08 (CLI, "run 12 sprints in batches") — 8 PRs merged, SVG stack-overflow DoS class CLOSED, well confirmed dry
Pivoted off the (still-dry) feature well into security hardening — a real, high-yield vein. Shipped 8 PRs / 9 tickets:
- CPE-1439 archive-ext preview routing (#708) — xz/bz2/zst/lz/lzma → "compressed file" info preview, dmg/cab won't-fix.
- **SVG stack-overflow DoS class CLOSED** — CPE-1437 (parked after 3-attempt breaker) re-scoped into **CPE-1444** (#712,
  combined hops×nesting product bound + 16MiB guaranteed stack) + **CPE-1445** (#713, bounded SVGZ decompress + reject
  double-gzip). Adversarial audit found a NEW adjacent vector on EACH of 5 attempts (use-composition → clip/mask chains →
  hops×nesting product → double-gzip) before it held.
- CPE-1446 (#710) office/ebook zip-entry deflate-bomb OOM cap (8MiB `.take`); CPE-1447+1449 (#711) thumbnail size-gate moved
  into decode_thumb_image after the video early-dispatch (fixes image OOM AND a video-thumb over-block in one place);
  CPE-1448 (#714) truncation-marker visibility; CPE-1450 (#715) flaky organize_apply test → tempfile::TempDir.
Metrics: 8 merged, **0 escaped defects**. Adversarial security auditor found 5 real SVG vectors + a researcher sweep found
3 real resource-exhaustion bugs — ALL pre-merge.

Tuned defaults / lessons:
- **Adversarial opus Sec-Auditor is decisive on SVG/parser diffs** — sonnet reviewers APPROVED every time while the opus
  auditor found a real bypass 4×. On any untrusted-parser diff, gate on an opus auditor that TRIES to overflow it, not just
  a code review. Reject-nested-input beats predict-recursion (a fixed stack can't bound an input-scaled hops×nesting
  recursion; bound the INPUT).
- **usvg has recursions NOT bounded by its 1024 cap** (clipPath/mask/pattern/marker converter recursion, gzip decompress) —
  a raw-byte guard is defeated by a compressed wrapper; a per-vector pre-scan is whack-a-mole (durable answer = process
  isolation, filed nowhere yet since the bound holds).
- **Resource-exhaustion is a DISTINCT sweep from panic-safety** — the panic sweep hunted malformed-byte crashes; well-formed
  pathological inputs (zip/gzip bombs, deep nesting) were a separate, productive vein. Library: [[resource-exhaustion-dos-sweep-2026-08-07]].
- **Editing a #[tauri::command] DOC COMMENT drifts bindings.gen.ts** (CPE-1447) — regenerate or the Linux drift guard fails.
- FRONTIER: headless FEATURE well dry (re-confirmed) AND the security vein now substantially tapped (font/net/doc readers
  checked clean). Backlog EMPTY. Next increment needs the USER — attended GUI verify of the 8 merged PRs, macOS, signing
  cert, live agent session, or a fresh feature direction.

## Shift 2026-08-08 cont. (CLI, "keep going") — batch 2: network/IPC security sweep, 4 PRs, 1 HIGH traversal-to-RCE closed
After the batch-1 wrap said "well dry", the user said "keep going" — which correctly surfaced that the UN-swept
surface (network protocol + crypto + IPC + frontend) had never been audited. A 3-auditor deep sweep + a frontend
XSS audit found real work the file-reader sweeps missed. Shipped 4 PRs / 8 tickets:
- **CPE-1461/1462 (#717, HIGH)** — provider-agnostic path-traversal → arbitrary local write from a hostile
  WebDAV/SFTP server (`transfer.rs download_tree` sink + webdav-href/sftp-name sources). `guarded_join`
  (Normal-components-only), source `is_safe_name`, validate-before-mkdir symlink guard, leaf-symlink skip, walk
  depth/entry caps + streaming, webdav `redirects(0)`. Adversarial opus auditor SEC PASS after 1 rework (symlink
  ordering). LATENT (pre-CPE-616/685 wiring) but fixed before it ships.
- **CPE-1471/1472/1473 (#718)** — sidecar host-OOM (unbounded `buf.lines()` → bounded `read_bounded_line` 16MiB),
  handshake `expected_id` validation, ed25519 `verify_strict`. CPE-1471 was the one CURRENTLY-prod-reachable bug.
- **CPE-1453/1454 (#716)** — net client stream-item / server WS-header unbounded reads → capped.
- **CPE-1475 (#719)** — bounded-reader ~8KiB overshoot nit.
Frontend XSS audit (all `{@html}`, previews, filenames): **CLEAN** — one dompurify funnel, SVG via `<img>`, email
HTML backend-stripped, structured previews plain-text. No findings.
Crypto/signing/vault/JWT/broker/egress/updater audited **CLEAN** (credit noted).

Tuned defaults / lessons:
- **"Well dry" was too hasty at the batch-1 wrap** — the file-reader sweeps never touched net/sftp/vfs/crypto/IPC/
  frontend. When declaring a surface tapped, ENUMERATE which crates/surfaces were actually audited; an un-swept
  crate that parses untrusted (esp. NETWORK) input is a real vein.
- **Path traversal = the top class for a file explorer's remote layer**: `local_dir.join(untrusted)` where an
  absolute/drive/UNC component REPLACES the base. Fix = keep only `Normal` path components + validate-before-mutate.
- **Review-agent worktree hygiene**: some reviewers ran `git checkout -b` in the SHARED checkout, leaking a branch
  onto main's working tree + leaking a worker commit onto local main twice. Brief review/audit agents to use
  `git worktree add <tmp>` in their OWN dir, never a bare checkout in the shared repo. Foreman must re-verify local
  main == origin/main after each merge (reset --hard origin/main) — the leak recurred 2×.
- **Concurrent process coordination**: the desktop nightshift was building the sprints_* skill family (CPE-1476)
  the whole time — left its untracked WIP + IDs alone, numbered above it.
- Session total across both batches: **11 PRs merged, 0 escaped defects**, 1 HIGH + 1 prod-MED + many DoS closed;
  security surface now comprehensively audited (readers/network/crypto/IPC/frontend). FRONTIER: genuinely dry for
  headless — next needs the user (attended GUI/macOS/cert/live-agent/network-E2E) or a fresh direction.

## Sprint 2026-08-08→09 (CLI) — 20 batches: workshift→Sprint rename + Network SFTP/WebDAV/FTP LIVE
Renamed the loop workshift→Sprint (1f31f045, 266 files; SPR-NN kept). User activated the Network program; shipped
the whole foundation LIVE: keychain CPE-1510, vfs-route CPE-1511 (local byte-for-byte PROVEN by opus adversarial
review), sidebar UI CPE-1513 (VISUAL VERIFY OWED), FTP provider CPE-1514, host-key TOFU persist CPE-1512 — so
sftp://, webdav://, ftp:// browse in the left pane. Earlier: CSP, waveform, gui-smoke restoration (0→37), thumb SSRF,
binary-arch, image-diff, split/join. Lessons: worktrees INSIDE project (.claude/worktrees/), never Z:\repos parent;
never git add -A while worktrees live (a leak swept files onto main); workers run cargo/git/gh synchronously; opus
adversarial reviewer catches SSRF/overflow/local-regression sonnet misses; providers mirror cpe-sftp/webdav +
regen src-tauri/Cargo.lock via cargo check. Handed off at batch 20/40, budget ~118/200 — resume fresh.

## Sprint 2026-08-09 (CLI) — batch 21: Network discovery + sidebar UX (user-directed, mostly away)
5 merges, 0 escaped defects: CPE-1516 permanent Network sidebar section (#736), CPE-1519 Windows-native WNet
discovery backend (#737) + frontend "Discovered" tier (#741), CPE-1520 user-reorderable sidebar sections (#739),
bindings base-fix (#740). Separate user request: Gource repo-history visualization → PR #738 (open for user
review; orphan-branch publish, NOT committed to main to avoid history bloat). Filed CPE-1521 (WNet outer-loop
cap, opus follow-up). **Incident:** #737's `NetShare.kind` DOC-COMMENT change (a specta::Type) drifted
bindings.gen.ts → CI Typed-bindings drift guard reddened main (Backend ubuntu); the Windows-local gauntlet + the
drift *unit test* both passed and masked it; caught via a later PR's inherited-red, fixed by regen (#740).
**Lessons:** (1) verify the ubuntu Backend drift-guard CI leg before merging ANY specta/backend PR — don't merge
on the Windows-local gauntlet alone; doc-comment changes DO drift bindings. (2) `cargo` isn't on the
non-interactive shell PATH → `%USERPROFILE%\.cargo\bin\cargo.exe` (PowerShell). Tuned defaults: frontend=sonnet,
2-wide, ~20-35m, clean; unsafe-FFI diffs=opus adversarial reviewer; Foreman-apply mechanical fixes (bindings
regen) directly = 0 agents. Z: drive I/O-saturates under concurrent cargo builds; GitHub API intermittently
slow → background git/gh + Read .output files. Wrapped on work-dry (~135/200), not the budget wall. Queue:
CPE-1521; CPE-1518 QNAP TS-133 E2E (ATTENDED, NAS 2026-08-10); epics CPE-1504 SMB / CPE-1500 OS-mount / CPE-1517
LAN mDNS discovery.

## Sprint 2026-08-09 (CLI) — batch 22: WNet hardening + mDNS epic activation (post-"resume")
User said "resume the sprint" (same session, so budget continued ~135→~140, NOT reset). Shipped: **CPE-1521**
(#742) — WNet discovery outer-loop cap (WNET_MAX_TOTAL_ENTRIES=4096 + iteration guard, partial-results),
closing the opus-review follow-up from CPE-1519; Reviewer+UAT+backend-CI green. **Activated + decomposed epic
CPE-1517** (LAN mDNS/SSDP discovery): dep-vetted → adopted **`mdns-sd`** (pure-Rust, no native Bonjour SDK,
permissive, maintained; beats zeroconf/astro-dnssd which need the Bonjour SDK on Windows). Filed fully-specified
slice-1 **CPE-1523** (new `crates/mdns` + `discover_network_mdns` command + parallel WNet+mDNS merge in the
existing Discovered tier) + research entry. **Wrapped on budget discipline** (~140/200 same-session, reset line
150) rather than start CPE-1523 (new-crate+dep+bindings — needs full budget headroom to avoid stalling at the
wall). Hand off: resume in a FRESH session → build CPE-1523 first. Lessons reinforced: verify ubuntu Backend
drift-guard CI leg before merging specta PRs; Foreman-apply mechanical fixes = 0 agents; cargo at
%USERPROFILE%\.cargo\bin\cargo.exe for own shells. Queue: CPE-1523; CPE-1518 QNAP E2E (attended, now covers WNet
+ mDNS + SFTP/WebDAV/FTP); epics CPE-1504/1500. Owed to user: Gource PR #738 review, sidebar visual sign-off.

## Sprint 2026-08-09 (CLI) — batch 23 (BATCHED run "run many sprints in batches", 23/40)
Shipped **CPE-1523 (#743)** — cross-platform **mDNS/DNS-SD LAN discovery** (epic CPE-1517 slice 1): new
`crates/mdns` (`cpe-mdns` + `mdns-sd` 0.20.3), pure `map_mdns_service` 6-scheme table + bounded `discover()`,
`discover_network_mdns` specta command, frontend `mergeDiscovered` running WNet+mDNS in parallel into the
existing Discovered tier (Sidebar.svelte unchanged). Opus reviewer cleared the dep audit (only mdns-sd + flume/
if-addrs/socket-pktinfo + libc patch — no bloat/dup) and the `ShareProtocol` Webdav/Davs enum blast-radius
(all matches exhaustive/catch-all). Full suite 2641 green; **the batch-21 bindings-drift trap did NOT recur** —
worker regenerated bindings + both Cargo.lock and verified clean, ubuntu Backend drift-guard confirmed green
pre-merge. Filed CPE-1524 (nfs discovered-row ＋Add gate nit). **Batched run continues 23/40** — same-session
budget ~145/200 spent → hand off to a fresh session; well NOT dry (ready: CPE-1483 Linux-root-drive-tile, 1508
image-compare, 1509 split/join, 1524 gate-nit; then CPE-1518 QNAP attended). Lesson reinforced: brief every
specta PR to regen bindings+lockfiles and verify the ubuntu drift-guard leg before merge; opus for
new-dep/enum-change diffs.

## Sprint 2026-08-10 (CLI resume) — BATCHED "up to 50" TARGET HIT (36→50, +#796 pending-CI)
14 tickets merged this session, 0 escaped defects, single 2-check gauntlet throughout. Closed epic CPE-1486
(browsable Trash: 1558 backend/1559 bindings/1560 UI). Started Binary Studio (epic CPE-1561): 1572 inspector DTO
merged, 1581 x86/x64 disasm (iced-x86 1.21.0) fully-gauntleted awaiting CI (#796 = batch 51, merge on resume).
Advanced per-file-type pane (CPE-1568: 1570 action-bar/1573 JSON-tree/1576 image-actions/1578 archive-actions) and
docs completeness (CPE-1569: 1571 IA+guard/1574+1575 Tier-1 pages/1582 kbd-reference). 3 user hrdr.cmd fixes
(1579/1580/1583). Filed 5 epics from user requests (Binary Studio CPE-1561 +children 1562-1567, per-filetype pane
CPE-1568, docs CPE-1569) with research banked to the Library (5 new entries). Bugs found by docs audit: CPE-1577,
CPE-1584. Tuned defaults: frontend=sonnet (opus for big App.svelte integration); gate Rust/dep/specta PRs on 3-OS
Backend+Server-crates matrix (ignore flaky GUI-smoke); new dep → both Cargo.locks; STOP sub-agents that poll CI
(Foreman owns CI verify); Foreman-apply trivial exactly-prescribed fixes. Budget ~124/200 → clean hand-off.

## Sprint 2026-08-10 (CLI resume, evening) — BATCHED "up to 50", reached 21/50 before the budget line
21 batches merged this session, **0 escaped defects**, Reviewer+UAT gauntlet throughout. The headline is
not the count — it is that the crew stopped trusting itself. Nine rework cycles, every one a genuine
defect caught pre-merge, several of them regressions *introduced by a fix* for a real bug.

**The gui-smoke turnaround (CPE-1594/1595/1601).** The QA-Architect audit found the harness had produced
**796 cancelled / 4 failed / 0 succeeded across 800 runs** — not flaky, silent. Three causes: the Windows
leg never executed a single assertion (WebView2, CPE-1048) and its 45-min timeout stamped whole runs
`cancelled`; the Linux leg worked and was discarded; and **`actions/upload-artifact` silently excludes
dot-prefixed dirs**, so `gui-smoke/.screenshots/` never uploaded (`include-hidden-files: true` fixes it).
Also: the standing "it's flaky, ignore it" note cited the WRONG ticket for weeks, which is part of why
nobody re-opened it. Now a blocking ratcheted gate: went 33/7 → **37 passed / 3 failed**, four specs
retired, screenshots uploading (5.4MB artifact). The remaining 3 are one coherent WebKitGTK `getText()`
family, not a vague pile.
**Screenshots paid for themselves the same day**: a worker read `shred-dialog-fail.png` and found a real
product bug — `.ctx` had no scroll container, so context-menu items past the fold were *unreachable*.

**Three-round tickets are worth it when each round finds something NEW.** CPE-1601 took 3 rounds: fix →
broke every flyout (`overflow-y:auto` forces `overflow-x:auto`; `overflow-x:visible` cannot opt back out)
→ fixed via `position:fixed` + JS anchoring → flyout orphaned on scroll → fixed by closing on
anchor-out-of-clip. CPE-1590 took 3 rounds the same way. Rule learned: **allow round 3 when the findings
are different each time; park when they repeat.**

**jsdom cannot see layout — say so out loud.** 3,231 tests passed while every submenu in the app was
clipped to nothing. Every real finding on menu work came from rebuilding the component CSS in **real
Chrome**. The best agents ran a **negative control** (reproduce the bug with the OLD code in the same
harness) before trusting a green result. Adopt that as standard for any visual/layout claim.

**Security-shaped findings, all from adversarial review rather than the author:**
- Archive safety reported "No zip-bomb risk detected" on a password-protected zip having read **zero**
  entries (CPE-1591). Fix = a tri-state; "unknown" must be structurally distinct from "safe".
- The ratio scan trusted the archive's **own declared sizes** (CPE-1602). Reviewer forged it. Round 1's fix
  was **one-directional** (caught shrunk-uncompressed, missed inflated-compressed) and — the subtle part —
  even a correct trigger still divided the honestly-measured numerator by a **forgeable denominator**.
  Final fix: a decompression-free **physical ceiling** from on-disk layout, sorted by real `data_start`,
  clamped everywhere. Padding-based evasion costs threshold-proportional real bytes, i.e. it stops being a
  lie. Common case still ~10-30ms.
- A crafted font froze the UI **8.8s** (bounded output, unbounded *work*: the cap counted codepoints
  *pushed*, not *examined*). ~3ms after.
- Batch Media silently overwrote originals with no confirm and nothing on the undo stack (CPE-1590), and
  the guard was frontend-only until CPE-1599 made the **engine refuse**.

**Docs work is a bug-detector.** Writing claims against real code found **nine** shipped bugs this session
(1577, 1584, 1590, 1591, 1592, 1605, 1606, plus 1613/1614 from reviews). Brief workers that the prose is
the by-product.

**Open at hand-off (all filed, none lost):** CPE-1613 (**High** — same-file check is raw string equality, so
`IMG_1.JPG`→jpg overwrites the original on Windows *even in non-destructive mode*), CPE-1611 (Medium,
raised from Low — shred_paths has no trash fallback and a smaller fix than batch-media had), CPE-1606
(Agent Watch keeps watching after you leave), 1600, 1603, 1607(merging), 1614, 1518.

**Tuned defaults confirmed:** sonnet for nearly everything, opus only for the QA-Architect audit; batch
trivial same-file tickets into one worker; **Foreman-apply exactly-prescribed fixes** (did so ~6 times,
0 agents); tell sub-agents to run everything **synchronously** — one stalled "awaiting background
notification" and had to be restarted; hold new dispatch when the merge queue backs up rather than
lengthening the CI jam; reserve ~25% agent budget to finish what's in flight.

## RUN 2026-08-11 (CLI resume, batched "up to 50") — batches 29→43, 21 merged, 0 escaped defects

**Shipped:** CPE-1606 Agent Watch "off means off" · CPE-1614 smart-folder notice (12 locales) ·
CPE-1603 File Health honours unreadable archives · CPE-1611 shred_paths confirm gate · CPE-1625 co-located
agent sessions · CPE-1619 docs depth pass · **CPE-1613 same-file canonicalisation (High data-loss)** ·
CPE-1615 .NET metadata tab · CPE-1616 notebook viewer · CPE-1627 45 notices → 12 languages · CPE-1600
durable checkpoint-failure records · CPE-1626 pause-vs-end metrics · CPE-1618 log viewer · CPE-1629
preview-pane screenshot harness · **CPE-1623 batch-media output containment (High)** · CPE-1633 onDestroy
teardown · CPE-1635 (premise disproven; hardening + harness) · CPE-1630 vault_create confirm gate ·
CPE-1637 bounded log window · CPE-1617 YAML/TOML viewer · CPE-1631 syntax highlighting.

**Filed from gauntlet findings: 29 tickets (CPE-1620…CPE-1648).** Two High: CPE-1642 (resolve output
identity instead of pattern-matching link shapes), CPE-1647 (vault_unlock's session_dir is uncontained, so
lock can shred an arbitrary directory), plus CPE-1645 (locking a vault silently destroys edits made while
unlocked).

### What the gauntlet caught that tests could not
Every PR passed its author's own tests. Independent checks still bounced **13** of them, every bounce a real
defect. The pattern worth keeping:
- **Ask for the number, not the impression.** "Is highlighting fixed?" → *measure token coverage*: 70.4%,
  with JSON at 23.6% because one class carried every object key. "Does the YAML viewer work?" → *count real
  files*: 4/13, with 2 valid files falsely called broken.
- **Use real inputs, never the committed fixture.** 13 real logs, 20 real configs, real .NET assemblies,
  real CI workflows. The fixture agrees with the parser by construction — it was written by the same author.
- **A negative control or it didn't happen.** Reproduce the bug against the OLD code in the SAME harness
  before believing a green result. This caught a "fix" that fixed nothing more than once.
- **Three lenses find three different things.** On CPE-1623: the reviewer found inputs wrongly accepted, UAT
  found valid input wrongly rejected, security found the engine wasn't the enforcement point at all.

### Failure modes to expect (all hit tonight)
- **A test written by reading the code can only confirm the code.** `foldBlockLines`'s test encoded the buggy
  output as expected, so the suite actively protected the bug. Derive expectations from the spec or a
  reference implementation (PyYAML/`tomllib` worked well).
- **"We don't know" must never look like "it's fine."** Hit four separate times: a corrupt binary rendering
  as an empty module, a non-empty log window reading as "This file is empty", a crashed session's record
  indistinguishable from a clean one, a failed checkpoint needing to be a *different type* to be safe.
- **Fixing a symptom invites the next variant.** CPE-1623 took three rounds — raw text → one-hop links →
  chains → contended reads — because the check matched link *shapes*. Filed CPE-1642 to resolve identity once.
- **A fix can be worse than the bug.** CPE-1626 round 1 stopped truncating a session's record and started
  losing it entirely. Two checkers caught it with separate harnesses.
- **Exposed ≠ caused.** CPE-1637 made a latent UTF-16 bug reachable by lifting a size ceiling. Say which.
- **Agents overstate severity.** A worker reported "95% of app.css dead"; triangulation showed one rule lost.
  Approve the code, don't approve the record — and correct it in place rather than quietly.

### Tooling lessons (now in .claude/qa-architecture/MANUAL-TEST-BURNDOWN.md)
- Headless Chrome's `--window-size` does NOT set the CSS viewport under `--headless=new`; it clamps to ~500px
  and rescales the screenshot. **This produced a false defect report (CPE-1635).** Mount in an **iframe** and
  confirm the width from inside it. Reproduce a layout defect with a verified viewport *before* filing it.
- **Never `taskkill /IM chrome.exe`** — it closed the user's own browser session. Kill your own PID only.
- The Linux GUI-smoke leg runs WebKitGTK with compositing disabled, where `waitForClickable` can never
  resolve for a button inside an `overflow-x:auto` strip. Dispatch the click at the DOM node instead.
- Two parallel PRs inserting at the same anchor in `i18n.ts` merge clean then conflict — sequence, rebase,
  and verify the **merged** state.

### Tuned defaults confirmed
sonnet for everything; three-leg gauntlet (Reviewer + UAT + Visual/Security as the diff earns it); Foreman
applies exactly-prescribed fixes directly (~8 times, 0 agents); `SendMessage` to resume a worker with context
beats a fresh dispatch; a worker that reports "awaiting a background notification" is **stalled** — restart it
and tell it to run synchronously; hold dispatch when the merge queue backs up.

## 2026-08-11 (CLI resume, BATCHED "up to 50") — batches 44→50, run COMPLETE at 50/50

**Shipped (6 PRs, 11 tickets):** CPE-1641 crashed-session visibility (#839) · CPE-1620+1622 small
frontend fixes (#837) · CPE-1647 vault session containment + lock-time re-validation (#838) ·
CPE-1632 contrast guard extended app-wide (#841) · CPE-1642 batch-media output identity + long-path
fail-open (#840) · CPE-1636/1638/1644 log viewer follow-ups (#842) · CPE-1621/1643 close-all-consoles
+ onDestroy leaks (#843).

**Escaped defects: 0. Blockers caught pre-merge: 7.** Four of six PRs needed a second pass; one needed
three. Every blocker was found by the *independent* leg, never by the author.

**What actually caught things — worth repeating:**
- **Neutralise each guard separately.** On CPE-1647 the reviewer disabled the lock-time re-check and the
  symlink refusal one at a time; both went red independently, and that experiment revealed one of them was
  the sole protection on a second code path nobody had mentioned. On CPE-1642 the same technique found a
  guard that could be **deleted with the suite staying green** — an unpinned last line of defence.
- **A fix can be worse than the bug.** CPE-1642 round 1 introduced a MAX_PATH fail-open: the probe used a
  Win32 call that silently stops working past 260 chars while the writer (`std::fs`) does not. Base `main`
  refused the case; the "fix" allowed it. **When a fix replaces a mechanism, diff the new mechanism's reach
  against the old one's.**
- **Test the guard, not just the code.** CPE-1632's contrast guard passed vacuously on
  `color: var(--token, #fff)` — the reviewer proved it by injecting a deliberately-broken pair and watching
  the guard stay green and emit no assertion at all.
- **Real inputs beat fixtures, again.** Every UTF-16 fixture in #842 was pure ASCII; one emoji in a real log
  broke detection entirely. The UAT used genuine Windows logs (Edge update, MSI, CBS, DISM) and live
  `node`/`python`/`cargo` crashes.
- **Refuse "too noisy to measure".** A reviewer re-ran the CPE-1642 perf test against base and found the
  fix is ~19% *faster* — the worker had written it off as unmeasurable noise.
- **Two independent legs converging is the signal.** On CPE-1621 the reviewer and the UAT found the same
  failure-path defect by different routes (code reading vs an executable UI repro).

**Tuned defaults observed:** security-containment-rust → **opus** for both worker and reviewer (every one of
those tickets needed 2-3 rounds; sonnet review would have missed the MAX_PATH and TOCTOU classes).
frontend/theme/log-viewer classes → **sonnet** throughout, no escapes. Median agent 1779s; 30 agent-runs
for 11 tickets (~2.7/ticket) — the conditional-legs gating held.

**Foreman-applied fixes (0 agents):** the CPE-1642 guard-pinning test, red-then-green verified. Worth doing
when a reviewer prescribes an exact, small change.

**9 follow-up tickets filed from review findings:** CPE-1650 (SSH repo URLs), 1651 (delete_permanent has no
backend gate — the exploit chain's step 2), 1652 (reparse tags + census cost), 1653 (link debris), 1654
(refused-lock UX + docs), 1655 (errors with no level word), 1656 (u16-table binaries + Go/Ruby/Rust traces),
1657 (timestamp-shaped digits defeat the bracket gate), 1658 (idle conhost survives close-all).

---

## 2026-08-12 → 08-13 (CLI, BATCHED "up to 40") — **COMPLETE at 40/40**

Resumed from the batch-26 checkpoint; ran batches 27–40 in one session. **10 tickets merged, 0 escaped
defects, 10 new tickets filed** (every one from a gauntlet finding, not invented).

**Shipped:** CPE-1688 (network-form coercion guard) · CPE-1690 (cpe-mdns's 17 never-run tests now in CI +
a crates/ coverage guard) · CPE-1691 (S3 SigV4: nine ways to smuggle content into a signed request) ·
CPE-1680 (GUI ratchet stops trusting its own inputs) · CPE-1692 (8 sites reporting denied-as-absent) ·
CPE-1682 (S3 errors name the real cause) · CPE-1679 (4 GUI flakes root-caused, 78% → 0%) ·
CPE-1698+1694 (specBasename + gui-smoke unit tests finally gate CI) · CPE-1699 (guard extended to
sidecar/*) · CPE-1701 (gui-smoke/lib flatness guard) · CPE-1697 (3,186-file duplicate tree removed) ·
CPE-1700 (S3 refusal precision + Trojan Source) · CPE-1695 (SigV4 SP/HTAB-only trim).

**Tuned defaults learned:**
- `error-taxonomy-sweep` and `parser-hardening`: **opus reviewer, sonnet worker**. Both classes had
  round-1 findings a sonnet reviewer would plausibly have missed (the `exists()`/`try_exists()` syscall
  split; the `>`-inside-a-comment evasion). Worth the tier every time on security-adjacent parsing.
- `ci-coverage` / `guard-hardening`: **sonnet both legs** is sufficient — 4 of 4 came back clean or with
  one exactly-prescribed nit.
- **3 Foreman-applied fixes, 0 agents each.** Every one was an exactly-prescribed reviewer nit
  (a `#[cfg]` gate, a dead-field acknowledgement, a doc correction). Applying these directly instead of
  spawning a worker round-trip is the single best budget lever found this run.
- **CI is the throughput ceiling, not the crew.** Median gauntlet leg 14 min; `Server crates (windows)`
  alone ran 55 min. Four concurrent PRs saturated the runner pool and everything queued behind it.

**Lessons worth carrying (the theme was one thing, five times over):**
- **A test can be worse than no test.** CPE-1692's permission tests probed with `fs::metadata` while the
  code called `try_exists()` — different Windows syscalls — so every leg skipped and the suite looked
  covered. Restoring the original bug left it green. CPE-1682's byte-cap test sized its fixture from the
  constant it was meant to pin, so the cap could be widened 4096× with CI green. CPE-1680's fix line had
  zero coverage; reverting it kept all 59 tests passing.
- **Trace the real path, not the obvious function.** CPE-1695's worker read `write_header` and concluded
  VT/FF reach the wire. The reviewer read one hop further: the send loop calls `Header::value()` first,
  which drops the *entire header* on one non-conforming byte — including the NBSP that ticket had just
  decided to preserve.
- **Two independent legs converging is the strongest signal.** Reviewer and UAT independently found
  CPE-1682's self-referential cap test, and independently reached CPE-1694's nested-test blind spot.
- **The 3-OS matrix earns its cost.** Two CPE-1692 failures were Unix-only and invisible on this Windows
  box: an ungated Windows-only message assertion, and a struct field only the Windows cleanup path reads
  (dead code under `-D warnings`).
- **Hand agents the blocking command, not just the prohibition.** Six stalls, all the same shape: an agent
  pushes a branch, wants to watch CI, invents a "background poll", and parks. The brief said "you get no
  notifications"; it did not say `gh run watch <id> --interval 30`. Saying the latter is what stops it.
  Memory `[[subagents-run-work-synchronously]]` updated.
- **Commit before you probe.** Four separate agents (and the Foreman once) wiped uncommitted work with
  `git checkout --` during guard-neutralisation. Committing the fix first makes the restore correct.

**Capacity note:** two consecutive `API Error 529 Overloaded` kills on an opus worker mid-task. Backed off
rather than retrying a third time, and preserved its 1,284 uncommitted lines as a WIP commit so nothing
was lost. Handed to the next run.

---

## 2026-08-26 → 08-27 (CLI, `/sprint`) — IN PROGRESS, notes so far

**Foreman override recorded (per CPE-1835):** CPE-1881 / PR #1046 was allowed a **4th** build→check
attempt, past the skill's 3-attempt circuit-breaker cap. Reason: the ticket was converging — each
round's findings were strictly finer than the last and nothing was re-found — and round 4's list
contained two genuine defects (a count/row mismatch that undercounts its own list on any mixed
outcome, and a failure/refusal distinction carried by **hue alone at matched lightness** in light
theme, invisible to protan/deutan vision and to greyscale) rather than polish. The cap exists to stop
burning agents on a ticket that needs a rethink; that is not this shape.

**Pattern worth carrying: a plausible verification API can return a false clean.** On PR #1045 both
the author and the reviewer's first pass used `document.elementsFromPoint` to check that an overlay
was not swallowing clicks, and both got a clean answer. Dispatching an **actual CDP mouse click**
showed the click landing on the wrong element. Twice on the same question. When the property is "can
a user actually do this", drive the real input — do not ask the DOM what it thinks is under a point.

**Pattern worth carrying: an assertion that reds when it fails to *observe* a transient state.** The
CPE-1822 mid-stream gui-smoke case failed on CI not because anything broke — 23 of 26 passed and the
run produced zero AssertionErrors from the app — but because the app finished streaming 2,500 items
faster than the poll caught the loading state. A test whose only failure mode is "I was too slow"
reds forever on fast runners and trains the crew to re-run rather than read.

**Second override recorded (per CPE-1835):** CPE-1896 / PR #1043 was also allowed a **4th**
build→check attempt. Same reasoning as CPE-1881 — converging, nothing re-found — and round 4's single
finding was a test half that proves nothing: the Reviewer disabled the leaf surrogate guard entirely
and the **full 2404-test suite stayed green**, because a symlink at the leaf is refused ~50 lines
earlier by an unrelated path check. That is the repo's signature defect and the cap must not be the
reason it ships.

**Pattern worth carrying: a fixture can be structurally unable to test the thing it is cited for.**
CPE-1896's synthetic reparse point proves the code reads the *tag* rather than the attribute — real,
and correctly measured. It cannot prove anything about a real OneDrive placeholder, because
`FILE_FLAG_OPEN_REPARSE_POINT` exists to **bypass the handler that owns the tag**, and the synthetic
tag has no handler. The one structural difference between fixture and reality is exactly the variable
being inferred. Honest at the code comment; the user-facing doc had promoted it to fact.

**Pattern worth carrying — the shadowed guard, with a diagnostic tell.** A guard cannot be given test coverage while an earlier guard answers on the same underlying fact; it is then simultaneously *safe* and *unverifiable*, and those are easy to mistake for each other. **The tell: a sabotage that leaves the suite green AND a fault-injection that changes no behaviour, on the same guard.** Separately each reads as evidence of safety; together they mean the guard is unreachable. Found on CPE-1896, where three symptoms presented and only the third looked like a problem at the time. Filed as CPE-1929 with a named lead (`batch_media::open_output_verified`, same shape, unexamined).

**Process lesson (bit twice): while a PR is open, its visual evidence lives on the branch only.** The Foreman landing screenshots on `main` at paths an open PR also carries produces a modify/modify conflict the moment the worker re-captures — and a CONFLICTING PR schedules **zero** CI checks, so it reads as "no runs yet" rather than "blocked". Cost two diagnosis detours on CPE-1883 alone. Do not land a PRs evidence on main until it merges.

**Pattern worth carrying — prove the harness can go red before you trust its green.** CPE-1896's worker built a dependency-free extraction harness to check the Unix/macOS arms from this Windows box. Its first version rewrote `pub(crate)` to `pub` while extracting — and a `pub` item is never `dead_code`, so the harness reported the **known-bad** code clean. It only trusted the harness after sabotaging the source and watching it reproduce the real CI error verbatim. This is the shadowed-guard disease (CPE-1929) one level out, in the verification tooling rather than the code: a check that cannot fail is indistinguishable from a check that passes.

**Pattern worth carrying — enumerate, do not recall.** CPE-1896's round-1 dependency change updated the two `Cargo.lock` files everyone knew about; both reviewers independently verified those two and both were **correct**. Seven others were stale, and CI discovers them **serially**, one per hour-long run. The worker's framing: *"round 1 didn't break the rule, it got the **enumeration** wrong — it updated the two it knew about and never asked how many existed."* A rule followed from memory is followed incompletely. The mechanical form — `git ls-files '*Cargo.lock'` -> grep for the changed package -> `cargo metadata --locked` in each — is complete by construction and takes seconds. Filed as CPE-1932.

**Foreman error worth recording — a red run list is history, not a diagnosis.** I filed CPE-1917 ("plain Release broken 27 days") from a checkpoint note plus `gh run list --workflow=release.yml`, which showed three failures, latest 2026-08-23 14:35. All true. What I never asked was whether a fix had landed **since the last run** — and it had, at 20:27 the same day (CPE-1872, `f97aef8a`). For a **tag-triggered** workflow, "what happened" and "what is true now" diverge the moment a fix merges without a tag, and stay diverged for as long as nobody tags — a month, at that channel's cadence. The worker's suggested pre-flight, two seconds: `git log --oneline -- <the file the failure names>` against the last run's timestamp. I also wrote a *proposed* flag from a PR review round into that ticket as landed fact; it was falsifiable with one `grep`. Both are the same disease this crew spent the night finding in code — a claim stated with more confidence than its evidence — committed by the Foreman, in a ticket.

**Pattern worth carrying — provenance claims in comments are untested by construction.** A comment saying "exactly the way `release.yml` invokes it" / "same as X" / "mirrors Y" cannot be checked, and decays silently **because the surrounding green test reads as vouching for it**. CPE-1872 carried three such claims that were true for exactly one commit before its own round 2 moved the check. Either derive the claim from its source at runtime, or do not make it. Filed as CPE-1933 with a seed grep.

**Foreman miss worth recording — a PR fell out of the rotation for four and a half hours.** PR #1039 (CPE-1908) sat at its round-2 head, CLEAN and MERGEABLE, from ~01:50 to ~06:35 while I chased five other PRs. Its worker had been sent a round-3 list and an addendum; it never pushed and never reported. `ListAgents` showed it "running 4h" — the only signal, and one I only looked at when I had a spare turn. When stopped, its last line was *"The macro compiles cleanly. Now let's run the full test suite"* — so it was genuinely working, just never reporting, which is exactly what the dispatch contract forbids and what the stall-check exists to catch **on a returned report**. There is no equivalent check for an agent that simply never returns. The lesson is a Foreman one: **track PRs, not agents.** A per-tick sweep of `gh pr list` against a known set would have surfaced this in minutes; watching for agent notifications did not, because the absence of a notification is not an event.

**Pattern worth carrying — a red-proof that fails BACKWARDS means the reproduction is broken, not the fix.** CPE-1908's takeover worker was building a byte-for-byte reproduction of a pre-fix predicate to red-proof its own change. The RED assertion failed — but in the **wrong direction** (`expected [] to deeply equal ['sidecar']`), the opposite of what a working reproduction shows. Root cause: a Python heredoc hand-typed `\b` inside a **non-raw** string, and Python silently converts that to an ASCII backspace byte (0x08) rather than the two characters — unlike `\s`, which is not a recognised escape and merely warns. So the *reproduction's* regex was corrupt and it was proving nothing. Found with `xxd` on the raw bytes, then a defensive control-character scan caught the same mistake repeated in the ticket narration. **The tell is the direction of the failure**: a red-proof is only evidence if it fails the way the historical bug would have.

## 2026-08-27 — real Linux execution is now a one-liner on this machine (from CPE-1913 / PR #1050)

A worker sent to fix a Linux+macOS CI failure that Windows could not see **bootstrapped a working
Linux Rust toolchain inside WSL** rather than reasoning about the failure from Windows:

- WSL Ubuntu 2 had rustc 1.97.1 but **no C compiler**, and `rusqlite/bundled` needs one, so musl +
  `rust-lld` was not a way around it. Docker Desktop's daemon was not running.
- It bootstrapped gcc-15 rootlessly: user-scoped `apt-get update`, `--print-uris` on
  `build-essential`, 41 debs unpacked with `dpkg-deb -x` into a sysroot, plus a `cc` wrapper that
  fixes the `libc.so` linker script (which names dev-only members no root install had put there).
- The environment then **reproduced CI exactly before any code changed**: 2402 passed / 1 failed /
  10 ignored, the same single failure.

**The toolchain is left in place at `~/lintools` inside the WSL VM** (outside the repo, so it does
not dirty the tree). Real `cargo test --lib` and `cargo clippy` runs on Linux are now a one-liner
for the rest of this sprint. **Dispatch prompts for anything touching `crates/server` should say so**
— this repo's 3-OS matrix means Windows-only local green has been a repeated source of an hour-long
CI round trip, and three separate PRs paid that cost this shift.

Two related lessons from the same fix:

1. **A cross-compile harness is not a test run.** PR #1050 reported "the Unix harness is clean on
   both targets" and that was true and useless — it cross-*compiles*. It cannot execute Unix tests,
   so it could not see a test that had stopped testing.
2. **The test's own inertness guard is what caught it**, not an assertion failure: *"fixture is
   inert: the refusal is not the link one this test is about"*. A guard that checks it is still
   exercising its subject turned a silently-vacuous test into a red one. That is worth copying.
3. **The fix made the assertion stronger, not weaker.** The old string
   `could not open the destination for writing` was a generic open-failure prefix any open error
   satisfied; the new one names the component and the link, and `open_beneath`'s own negative test
   pins the surrounding boilerplate as lexically disjoint from `"is a link"`. When a guard trips,
   check whether the replacement can be *tighter* than the original — relaxing it to green is the
   move that creates a shadowed guard.

## 2026-08-27 — a green STATIC suite is not evidence a code path is unreachable

PR #1059's worker ran five sabotages. Four reddened. The fifth — a by-path `remove_file` placed
*after* the Unix handle-relative descent — stayed green, and it reasoned that `O_NOFOLLOW` refuses at
the component so the leaf is never reached. Clean, plausible, and it wrote the conclusion into the
module's **design narrative** as a finding rather than a miss.

The Reviewer did not argue with the reasoning. It ran the **race harness the same PR had just built**
and never pointed at that question:

    pristine (unlinkat)                        200 trials    0 files deleted outside the root
    by-path remove_file after the descent      200 trials   94 files deleted outside the root

The leaf *is* reached on every successful delete, and a by-path leaf **re-resolves the whole path from
the root**, which a concurrent rename redirects. `O_NOFOLLOW` only makes the leaf unreachable with a
hostile name in a **static fixture**.

Two rules from this:

1. **Reachability is a claim, and a static suite cannot support it.** If a PR ships a race or fuzz
   harness, run it against the reachability question before concluding anything. Now a standing line
   in every dispatch prompt.
2. **A wrong "this is unreachable" note is worse than no note.** It invites a future maintainer to
   swap the primitive back for the by-path call and reintroduce a 94-in-200 escape with green CI. The
   fix is to correct the narrative in *all three* places it was written — PR body, ticket Work Log,
   and the function's own doc — and specifically **not** to add CPE-1929's untestable-backstop
   annotation, which would cement the false claim.

Sibling finding from the same review, same family: `apply_delete` computed a permanent-vs-transient
classification and **discarded** it — `Refused.permanent` is read only in the *write* loop. Replacing
the branch with an unconditional `transient` left the suite at 2419/0 unchanged, while the shipped
user-facing doc asserted the distinction was real. **A branch no test can distinguish is not a
feature, and documenting it to users makes it a false promise.**

## 2026-08-27 — the janitor pruned a live reviewer's worktree twice in one shift

Two independent reviewers (#1052's and #1058's) had their round-1 worktrees pruned out from under them
mid-review and had to re-create them from the PR head. Nothing was lost either time and `main` was
never touched, but it is the known [[janitor-prune-breaks-agent-resume]] hazard costing real re-setup
time on **agents that are still attached to an open PR**. If a cleanup pass runs during a sprint, it
must skip every worktree whose agent has an open PR in the queue — not just the ones currently
executing.

## 2026-08-27 — real Linux `sidecar-host` runs now work too

CPE-1949's worker extended the WSL sysroot at `~/lintools` rootlessly — `pkgconf`, `libpkgconf7`,
`libdbus-1-dev`, `libsystemd-dev` unpacked with `dpkg-deb -x`, plus a `~/lintools/bin/pkg-config`
wrapper that sets `PKG_CONFIG_SYSROOT_DIR`. Two earlier workers had reported `sidecar-host` as
unbuildable there (`libdbus-sys` needs headers the base sysroot lacks) and fell back to
Windows-only verification.

So the crew now gets real Linux runs for **`crates/server` and `sidecar/host`**. Say so in dispatch
prompts for both.

**The `/mnt/z` staleness gotcha is confirmed and has now produced a false green twice.** Cargo reported
`Finished in 0.53s` and ran stale code against a sabotage. **`touch` the file inside WSL before every
run.**

## 2026-08-27 — a `pub(crate)` test seam can leak across tests

CPE-1937's worker added `BETWEEN_DESCENT_AND_LEAF`, a `#[cfg(test)]` injection seam, to close a real
CI coverage hole — the right fix, and the Auditor proved it absent from a **linked non-test binary**
(strings 0, symbols 0, against a test-binary control of 3/3; the only rlib occurrence is in
`lib.rmeta`, which the linker discards).

But it can be **left armed**: arm the hook, run a delete whose *descent* refuses so the seam is never
reached and nothing consumes it, and the next unrelated delete fires it. Measured
`still_armed_after_refused_descent = true`, `fired_on_next_unrelated_delete = 1` on both platforms.
The seam is `pub(crate)`, so any module's tests can arm it, and under `--test-threads=1` libtest
shares one thread — so a leak crosses tests.

**Rule: a test injection seam must clear on drop (RAII) or at entry to the function it instruments,
not rely on the instrumented path being reached.** `WALK_SYSCALLS` has no such guard either, and its
only consumer prints an unasserted number — so there is no existing pattern to copy, and the next
module reusing this machinery (`copilot::apply_op` + `renameat`) is the one at risk.

## 2026-08-27 — the backstop fired, filed a correct diagnosis, and nobody read it for hours

`catalog-freshness.yml` detected that the live catalog URL a real client fetches returns **HTTP 404 —
no catalog published at all**, and **filed GitHub issue #1062 ten seconds later** with the right
diagnosis. It sat open, unread, while the Foreman ran an entire shift's worth of ticks.

`Ticketing/wiki.md` → "External findings" **already specifies** a Foreman `gh issue list` sweep. The
sweep simply never happened, because nothing in the tick loop prompted it.

**Rule: `gh issue list --state open` belongs in the same first-thing-every-tick sweep as
`gh pr list --state open`.** An automated backstop that files somewhere nobody looks is
indistinguishable from no backstop. PR #1064's worker was right to decline adding a second, louder
channel — a second channel would have the same failure mode. **The fix is procedural.**

Worth noting how it surfaced: not from the alarm, but from a worker answering an *incidental*
question about published catalog indexes and choosing to measure all 60 release runs instead of
giving the one-line answer it was asked for.

## 2026-08-27 — a skipped job is invisible to the merge gate

The catalog job did not fail for 33 days; it was **skipped**, because `needs: release` and the release
was broken. `ci-poll.mjs` reports `pending` and `failure` counts — a **skipped** job is neither, so
this whole class is invisible to the gate the Foreman actually merges on.

Two instances found today by one enumeration (CPE-1932): `catalog` behind `release`, and **all five of
`ci.yml`'s test jobs behind `lockfile-preflight` with no `if:`** (CPE-1956). The second matters
because **GitHub counts a skipped required check as satisfied** — latent only because this repo has no
branch protection at all.

**Rule: when a job is chained behind another, decide explicitly whether a skip should be loud, and
prefer a terminal `if: always()` verdict job** — the shape `gui-smoke-linux-verdict` (CPE-1753)
already uses, which exists because "everything else happened to pass" is not "everything ran".

Corollary worth carrying: **CPE-1893's guard was real but conditional.** It made the job fail loudly
at `gh release upload` — *only when the signing key was present*. With the key unset, every step was
gated off and the job ended **green having published nothing**. A green job is not even suspicious.

## 2026-08-27 — record evidence in its DURABLE form, because evidence decays too

PR #1063's Reviewer settled a rebase question with a precise fact: *"the merge base of the branch and
`origin/main` is `6312b87b`, which is #1058's squash-merge commit."* The Foreman relayed it and asked
the worker to record the rebase in the Work Log.

The worker went to write it down and found **the evidence had already decayed** — a second rebase had
moved the merge base to `3d4276f8`. Writing "the merge base is `6312b87b`" would have been a **fresh
stale claim, in the entry written specifically to prevent stale claims.**

It recorded the durable form instead:

    git merge-base --is-ancestor 6312b87b HEAD   -> true

That statement stays true across every future rebase. The sha-equality one was true for one tree.

**Rule: when recording evidence, prefer the form that survives the next commit.** "X is an ancestor of
HEAD" over "the merge base is X"; "this test reds when Y changes" over "this test passed at sha Z".
This is the same defect family as CPE-1933 (provenance claims) one level up — the claim is about the
*repository state* rather than about the code, and repository state moves faster than anything else.

Two smaller instances from the same round, both caught by their authors before pushing:

- Correcting a comment's cause list, the worker's **first draft was also wrong** — it wrote "four
  early returns" where a grep of the function shows **five failure exits, four distinct causes** (the
  parse step contributes two). It grepped rather than recalled, on the second try.
- Reviewing its own diff it saw `CPE-1955` showing as **deleted** — its `origin/main` ref predated the
  Foreman filing that ticket. Re-fetched rather than resolving the phantom deletion.

## 2026-08-27 — a test harness can report a wall of green PASSes that are all artifacts

PR #1064's Reviewer, attacking new workflow gates, wrote this before its findings:

> My first attack harness reported a wall of green PASSes that were **all artifacts** — Windows temp
> paths unreachable to bash (rc=127), `subprocess(env=)` not reaching this WSL bash (`set -u` killed
> every body at line 2), and PATH stubs with no exec bit. **Every one of those satisfied "expect
> nonzero" for the wrong reason.**

It only trusted its numbers after fixing all three and **re-asserting that each guard fails with its
own diagnostic message**.

This is the exact inverse of the week's other lesson. We have been finding guards that pass when they
should fail; this is a *harness* that fails when it should pass, reported as success. Both come from
the same root: **asserting on a coarse signal (an exit code, a green suite) instead of on the specific
thing the check is about.**

**Rule: an "expect nonzero" assertion is nearly worthless on its own.** A missing binary, an
unreachable path, an env var that never arrived, a stub without an exec bit — all produce nonzero.
Assert the **diagnostic message** the guard is supposed to emit. Same for "expect zero": assert what
was produced, not that nothing complained.

Corollary observed twice in one day: **a test that `return`s early on a missing tool reports a green
pass for a test that never ran.** #1064 has 3 of 33 doing this where `jq` is absent (so its "33 tests"
is 30 on that machine, and its 23/33 mutation kill is a *lower bound*), and #1061's bash-gated tests
had the same shape until its author was asked to make them throw. **Prefer failing on a missing tool
over skipping** — CI always has it, so the only machine that skips is the one where a human would
have wanted to know.

## 2026-08-27 — "assert the diagnostic" means YOUR diagnostic, not the shell's

PR #1064 took the false-green lesson (assert the guard's message, not just a nonzero exit) and applied
it one notch too literally: its tests asserted on **bash's** error text.

    Git Bash (Windows):  bash: line 1: [: 0\n1: integer expected
    GNU bash (Linux):    bash: line 1: [: 0\n1: integer expression expected

Green locally, red on CI, on a wording difference between shell builds.

**The rule needs its second half stated:** assert on **your own** guard's diagnostic — the message the
code under test emits — and on the **behavioural** fact (branch not taken, exit code, no output
written). An external tool's human-readable text is not the thing under test, and it is not stable
across builds, versions or platforms.

The same review round had already flagged the general form on a different tool: `/npm error/i` matches
npm 7+ but **not** npm 6's `npm ERR!` prefix, so a future wording change would silently revert that
guard to fail-open. Same defect, caught before it shipped there and after it shipped here.

**Practical test:** if the string you are asserting on was written by someone else's program, it is
evidence, not the assertion. Match it loosely if at all, and say at the site why it is loose.

### Coda, same day — the "loose pattern" fix I suggested was ALSO wrong

Having told #1064's worker to assert its own diagnostic rather than bash's, the Foreman offered
`/integer expres/` as a loose fallback if it wanted the shell's complaint too.

**That pattern matches `integer expression expected` but NOT `integer expected`** — green on ubuntu,
red on the Windows machine the original was written on. The same failure, inverted. A narrower pin
wearing a wildcard.

The worker caught it because it **ran** the fix rather than reasoning about it: its first attempt went
red locally on exactly that pattern. It shipped `/integer\b.*\bexpected/`, verified against both
literal spellings before committing, with the site recording what to do if a third wording appears:
**widen to the exit code alone, never grow an alternation of vendors' prose.**

Two things to carry:

1. **A regex over another tool's prose is not made safe by being loose.** Looseness has to be
   *verified against every spelling that exists*, which is the same work as not depending on it.
2. Its better fix was to stop needing the string at all — the MECHANISM test now captures `[`'s own
   exit status (`cmp_rc=2`) and asserts branch-not-taken. **`cmp_rc=2` is a stronger statement than
   any message match**, because it names the thing (`[` reporting an *error*, not a verdict) rather
   than a symptom of it.

Its external-tool sweep of the whole file also produced the right general test: one surviving message
assertion (`release not found`) is safe because **that string is the test's own `gh` stub speaking** —
a fixture the test controls, not a claim about how any released `gh` words it. *Own the string or
don't assert on it.*

## 2026-08-27 — a suite-size number in a comment is stale before it is committed

PR #1066's worker wrote sabotage results into code comments as **absolute suite sizes** ("the whole
2,423-test lib suite stayed green"). Over the course of one PR that number moved four times — 4,857 →
4,883 → 4,923 → **4,932** for the frontend suite, and 2,423 → 2,425 for `crates/server` — **none of
them caused by its own code.** Other PRs merging is enough.

Its own `batch_media` sabotage figure was already one behind by the time it was reviewed (2,423 vs the
merged state's 2,424), which cost a round of reconciliation.

**Rule: record the DELTA, not the absolute.** *"disabling it leaves the suite unchanged"* stays true
forever. *"leaves 2,423/0"* is true for one afternoon. Where an absolute genuinely helps, stamp it —
"measured at 2,425 on merge" — so a reader knows whether the drift is theirs.

This is [[record-evidence-in-its-durable-form]] again, one layer down: the merge-base version was
about *repository state*, this one is about *the test suite*, and both move faster than the code the
comment is attached to.

Related, from the same PR: **writing the number at the site is itself the rule.** Its `CLAUDE.md`
entry says to run the two sabotages and write the numbers into the comment — and four of its own sites
argued the measurement in the PR body while the code comment merely asserted. On a ticket whose
thesis is that an argument is not a measurement, that was the one place the diff argued where it
could have measured.

## 2026-08-27 — a token that backs several roles gets pinned at the LOOSEST of them

CPE-1919 was filed as "dark-theme JSON string values measure 3.70:1". The real defect was structural:
`--accent` backs **three** roles with **two** different WCAG bars — a solid button fill under white
text and an icon/ring/border (1.4.11, **3:1**), and running text (1.4.3, **4.5:1**). CPE-1632 tuned the
dark value for the first two; `JsonTreeNode.svelte` paints the third.

**And the guard was green the whole time.** `dark-contrast.test.ts` *does* assert `--accent` against
`--bg`/`--surface` — at `>= 3:1`, labelled *"used as text/icon/focus-ring accent"*. The pairing was
**enumerated at the wrong bar**, not missing. That assertion reads exactly like coverage.

**Rule: a design token that serves more than one role needs a token per BAR, not per colour.** When a
guard names several roles in one label, that is the tell — it is grading all of them at whichever bar
is loosest. The fix here split `--accent` (chrome, 3:1) from `--accent-text` (running text, 4.5:1),
left `--accent` untouched so no button or ring moved, and put 22 text sites on the new token while 12
icon-glyph sites deliberately stayed.

Two things the sweep found that nobody had reported:

- **hc-dark's `--accent` was 4.48:1 on `--surface-alt`** — a second live failure, in a *high-contrast*
  theme, found only because the worker measured **every** token against **every** painted surface
  rather than the one the ticket named.
- **The ticket's own 3.70:1 was against the wrong ground.** `.preview-pane` paints `--surface`, so the
  real reading is **3.21:1**, and `.jt-row:hover` repaints to `--surface-alt` — a third surface no
  palette guard measured text against at all.

**Corollary: derive the painted surface, don't assume it.** The new guard reads the ground out of
`.preview-pane`'s background and `.jt-row:hover`'s fill, and **throws if either stops setting one**, so
it cannot grade against a colour nobody paints. Same shape as the provenance rule: derive it, or do
not claim it.

## 2026-08-27 — the re-run reflex had ALREADY let a real regression through

CPE-1955 was filed because `gui-smoke` shard 2 died seven times in one day, always reporting
**"0 new failing cases"**, always green on re-run. The ticket's stated worry was that this *trains the
crew to reach for `gh run rerun` on a red GUI shard — the habit that eventually lets a real regression
through.*

It already had.

PR #1068 found the "0 new failing cases" was a **second, independent defect**: the spec-attribution
variable advanced only on the reset's *success* path, so when recovery threw, attribution froze on
spec #1. The other thirteen specs still ran, still failed, and **were never written to disk** —
confirmed against the stored artifact, which held one result file for a shard that visibly executed
fourteen.

The first CI run of that fix, on its own PR:

    14/14 spec file(s) reported, 26 case(s) — 23 passed, 1 failed
    NEW GUI REGRESSION: "macro-param-prompt.smoke.ts :: running a bound {ask:suffix} macro
      opens MacroParamPrompt before any dry-run confirm" — not listed in known-failing.json
    FAILED — 1 new failing case(s), incomplete=false

A **named** failing case in a **named** spec, not listed as known-failing, where seven previous runs
had said "0 new failing cases".

### Correction — the mechanism above is WRONG, and the conclusion survives by a different route

The Foreman's hypothesis was that `macro-param-prompt` had been failing *inside the swallowed
thirteen*. PR #1068's worker checked instead of agreeing, and it is one step off:

- `grep -c 'ctx .flyout .row'` on job `98646323315` is **0**. In the four swallowed runs the transport
  died at spec **#2**; `macro-param-prompt` is spec **#6**, so the app was already gone and it never
  really ran. **Those runs cannot speak to that spec's health at all.**
- The regression is **not new and not caused by #1068**: job `98697809924`, on a *different agent's*
  branch at sha `373ee259`, **before the fix existed**, reported byte-identical output — same
  `14/14 reported`, same 23/1/2, same case, same real error `element (".ctx .flyout .row") still not
  existing after 5000ms`. That also explains the `2 skipped/pending` as pre-existing rather than a new
  symptom.

**So shard 2 had TWO independent failure modes on the same day**: an illegible transport death
reporting "0 new failing cases", and a genuine intermittent `macro-param-prompt` failure. Runs that
died reported nothing actionable and were re-run; runs that *survived* reported the regression — and
those were re-run too.

**The conclusion holds and is arguably worse than the original claim.** The re-run reflex was not
merely discarding evidence that had not been written; it was discarding a **legible, named regression
that the ratchet had correctly reported.** The masking and the real failure were separate problems,
and the habit swallowed both.

Three things to carry:

1. **"0 new failures" from a suite that did not complete is not information.** The ratchet was right
   to red on `incomplete=true`; the number beside it was meaningless and read as reassurance.
2. **An intermittent infra failure can be a mask.** Every re-run that "fixed" shard 2 also discarded
   thirteen specs' results. The flake was not merely costing CI cycles — it was **suppressing
   evidence**, and the standing "re-run once, then investigate" rule is what eventually surfaced it.
3. **Do not exempt the thing your tool just found in order to land the tool.** Adding
   `macro-param-prompt` to `known-failing.json` to get #1068 green would have destroyed the finding.
   It gets its own ticket; the harness fix stays a harness fix.

## 2026-08-27 — an allowlist that fails closed beats a sweep that has to find everything

CPE-1919's first sweep grepped `color: var(--accent)`. The Visual Critic independently enumerated the
survivors and **reached the same set** — which read as a strong cross-check.

Both were blind to the same thing: **five sites spell it `var(--accent, <fallback>)`**, plus one on
`--accent-hover`. All six were running text, one at **3.43:1 on 10.5px**. Two independent
enumerations agreeing is not evidence of completeness when both use the same query.

The durable fix was not a better grep. The guard now **inverts the default**: every `color:` in `src/`
resolving to `--accent`/`--accent-hover`, in *both* spellings, **fails unless** its selector is
declared in an `ICON_ROLES` allowlist with a note naming the glyph it paints.

Two properties worth copying:

1. **An allowlist, not a heuristic — deliberately.** Nothing in CSS separates a checkmark from a word;
   both are `color:` on an inline box. Any heuristic is guessing, and **a guard that guesses "icon" is
   silently back to no guard.**
2. **A third test reds on any allowlist row that stops matching anything**, so an exemption cannot
   outlive what it excuses. That is what stops the allowlist becoming the next stale artifact.

**Rule: when a sweep must find every instance of something, prefer a guard that fails on anything
undeclared over one that searches.** Searching is bounded by the query you thought of; declaring is
bounded by the thing itself. Same shape as *enumerate, don't recall* (CPE-1932) — but stronger,
because it does not depend on the enumeration being right either.

## 2026-08-27 — "there is no guard for this" is a claim about other files, and needs measuring like any other

PR #1069 (CPE-1919) added a careful comment recording that the repo has **no general theme-parity
guard** — written specifically so nobody would trust an unchecked assumption about coverage. Its
round-3 Reviewer deleted a token and ran the tests: `dark-contrast.test.ts` **caught it by name**,
because that file already carries a fixture-independent symmetric check. `hc-contrast.test.ts` did
not. So the note was wrong in the *safe-sounding* direction — it understated existing coverage, which
sends the follow-up ticket at the wrong half of the problem and tells the next maintainer that
light↔dark parity is unguarded when it is guarded.

The lesson is narrower and more useful than "measure your claims": **a negative claim about coverage
feels like an observation and is actually a claim about several other files at once.** "There is no X
in this repo" is the same species as CPE-1933's provenance claims — untested by construction, and made
*more* dangerous by sitting inside a comment whose whole purpose is to be trusted. It is also
cheap to check, and the check is a deletion, not a read. Both of this PR's blockers, in consecutive
rounds, were unmeasured claims: first a contrast ratio stated as measured when it was estimated, then
a coverage gap stated as total when it was partial. Same author, same care, same defect twice.

Filed as **CPE-1962**, and the fix turned out to be nine lines copied three times rather than a design.

## 2026-08-27 — the janitor deleted a live worktree again, mid-review

PR #1069's Reviewer reported its worktree `agent-a8a8bc5e076a3faf8` was removed by the janitor while
it was working. It rebuilt from `origin/` and lost nothing, so this cost minutes rather than work —
but it is the **second** occurrence of [[janitor-never-rmrf-active-worktrees]], and the first one
clobbered a live worker and dirtied `main`. A reviewer's worktree looks idle exactly when it is
thinking, so "no recent writes" is not a liveness signal. The janitor's skip rule needs to key off
**the agent registry**, not filesystem mtime.

## 2026-08-27 — claim-then-rename closes a TOCTOU race and silently rewrites the file's access control

PR #1070 fixed CPE-1958's hard-link race the right way: stop writing through a handle whose object an
attacker can substitute, and instead stage bytes into a `create_new` sibling and commit with a rename.
It measured 0/2,000 on Windows and 0/10,000 on Linux, with the controls red — and its Security Auditor
re-raced it independently and confirmed every number, including the subtle part: in the planted shape
the *fixed* arm faced **more** hard-linked trials than the pre-fix body (Win 1,016 vs 889, Linux 9,480
vs 7,913), so the improvement is not the attacker having got weaker.

Then it found what the fix cost, which nobody had thought to measure. **A rename does not carry the
destination's security descriptor.** After the change, a Windows file with inheritance broken and one
owner-only ACE came out `AreAccessRulesProtected=False` with **four inherited ACEs including
`Authenticated Users: Modify`** — and its `Zone.Identifier` stream gone, which is Mark-of-the-Web, so
a downloaded file put through a confirmed Convert loses SmartScreen and Protected View. `main`'s
in-place write in the same folder preserved all of it.

Three things make this worth writing down:

1. **The downgrade is the exact one a sibling ticket already forbids.** CPE-1739 *fails the save* on
   Unix rather than leave the user a file more readable than the one they saved. This path now performs
   that downgrade on Windows — because `carry_protections`'s Windows arm is a deliberate no-op that
   delegated the job to `ReplaceFileW`, and the new commit path never calls `ReplaceFileW`. The
   guard did not fail; it was **routed around** by a change in a different file.
2. **The PR recorded it as a "preservation cost."** That framing is what let it through: it reads as a
   fidelity nicety rather than an access-control change. When a fix changes *which syscall creates the
   final object*, the question is not "did we preserve the metadata" but "who can read this now".
3. **The invariant was true and still not enough.** *"Nothing planted at the destination can redirect
   the commit"* is correct — and silent about the **source**. The staging file is an enumerable
   `*.cpe-tmp` in the same attacker-writable directory; unlink it, hard-link an outside file into its
   name, and the rename commits that inode. Measured **2,805 / 3,000 on Linux**: `Ok(())` returned,
   the user's bytes never written, the destination now a second name for a file outside the scope root.

**The general shape: a containment fix that changes the object's provenance inherits a whole second
set of properties nobody audited** — ACLs, streams, inode identity, mode bits, and the directory
permissions the new path now requires (F2: operations that worked on `main` with file-write alone now
need directory-write, and fail). Race-closure and access-control preservation are different
properties, and closing the first is a good reason to re-measure the second, not evidence about it.

## 2026-08-27 — "intermittent" and "consistent" were both wrong: it had a commit boundary

CPE-1960 was filed as an intermittent gui-smoke failure. I then raised it to High with a note arguing
the opposite — that **every** shard-2 run which actually completed had reported it, so the word
"intermittent" should come out. I was wrong in a specific and instructive way: I reasoned from the runs
I had already looked at instead of running the query the ticket itself specified. The worker ran it and
found **two** completed clean runs (jobs `98681871872` and `98686079109`, both `14/14 … 24 passed, 0
failed`). Both **predate** commit `48aa8697`; every completed run after it fails. **100% deterministic
on either side of one commit.**

Neither "intermittent" nor "consistent" was the right frame, and the tell was in the evidence I had
already used to argue High: *"three unrelated branches"* is not the signature of a race — it is the
signature of three branches that had all rebased past the same commit. I read that fact and drew the
opposite conclusion from it.

**The cause is the sharper lesson.** `48aa8697` is CPE-1945 — the `npm audit` sweep — and its only
functional change is `gui-smoke/package-lock.json`: **webdriverio 9.30.0 → 9.31.4**. WebdriverIO's
`element.scrollIntoView()` is not the DOM API; it injects a real mouse wheel. 9.30.0 emitted
`deltaX:0, deltaY:0` with `origin: <the element>` — a no-op. 9.31.4 emits a real delta with **no
`origin`**, so the wheel lands at viewport **(0,0)**. Seven specs called it on `position: fixed` popup
rows that can never need scrolling; on WebKitGTK the stray wheel moved the hover target, the flyout
closed on `mouseleave`, and the retry found nothing.

So: **a lockfile-only change inside a patch release silently converted a no-op into an input event**,
and it did so in the harness that guards the whole GUI verification leg. Two things follow. A
dependency bump with no source diff is still a **functional** change and deserves the same
gauntlet as a code change — "audit fix, lockfile only" is exactly the diff nobody reviews. And a
helper named for a DOM API that does not call that DOM API is a trap that had been armed in five
further latent call sites, waiting for whichever spec next put a fixed-position element in front of
it. The fix replaces the command with the actual DOM API and adds a guard test so the command cannot
come back.

## 2026-08-27 — a reviewer in a worktree off the PR branch sees the repo as of the PR's BASE

PR #1070's Reviewer reported, as a finding, that *"there is no `CPE-1957` anywhere in `Ticketing/`"* and
that no ticket existed for the two sites the PR deliberately left. Both tickets exist on `main` —
CPE-1957 and CPE-1961, the latter filed hours earlier from that same PR's Security Auditor. The
Reviewer's worktree branches off #1070's base, which predates both, so its `find` was correct about its
checkout and wrong about the repo.

This is easy to repeat and easy to miss, because the finding *reads* like the strongest kind: an
absence, mechanically verified. The rule: **any claim of the form "X does not exist in this repo" made
from a PR worktree is a claim about that branch's base commit.** Ticket files, docs and sibling guards
all move on `main` while a review runs, and a long review — this one took an hour — guarantees drift.
Check such claims against `origin/main`, not the working tree, or state the base you checked.

The corroborating half of the same finding was genuinely valuable and nearly lost with it: the Reviewer
independently re-measured arm E at **51/1,000 (Windows) and 55/1,000 (Linux)** on the fixed branch,
a second confirmation of CPE-1961's numbers from a different harness run. Worth separating the
verifiable half of a finding from the stale half rather than discarding both.

## 2026-08-27 — three successive characterisations of one failure, each better than the last, each still wrong

CPE-1960 was described three times before anyone got it right, and the progression is worth keeping
because each step *looked* like the correction that ended the matter.

1. **"Intermittent"** — filed that way from two observations.
2. **"Consistent; every completed shard-2 run shows it"** — my correction, raised to High. Wrong, and I
   had the disproof in hand: *"three unrelated branches"* is the signature of three branches rebased
   past one commit, not of a race.
3. **"100% deterministic on either side of commit `48aa8697`"** — the worker's root-cause, from 5
   sampled jobs. It identified the real mechanism (a lockfile-only webdriverio bump turning
   `scrollIntoView` from a no-op into a real mouse wheel at viewport (0,0)) and the real fix. The
   boundary was still wrong.
4. **The measured answer** — its Reviewer enumerated **all 32** shard-2 jobs in the window and
   fingerprinted what `npm ci` actually installed (`added 479 packages` = wdio 9.30.0, `489` = 9.31.4,
   a clean split). Result: **0/14 fail on 9.30.0, 9/10 fail on 9.31.4**. Onset is **20:33Z on the
   branch**, two hours before the merge. The rate is **~90%, not 100%**.

**The discriminator was what got *installed*, not what got *merged*.** Every wrong version of the story
used merge time as the boundary, which is why the falsifier hid in plain sight: job `98681871872`, cited
in three separate files as the clean *pre-bump* run, checked out PR #1065's own **merge commit**,
installed **489** packages, and passed. A clean, complete run **on the broken version**.

**The consequence is operational, not academic.** At ~90%, **one green CI run does not verify this
fix** — a green run already happened on the broken version. The PR, the ticket, the harness README and
two source comments all told the next person the opposite, and that claim was about to land inside a
permanent guard file where a passing test would appear to vouch for it. Same shape as CPE-1933.

Two rules fall out. **Sampling a handful of CI jobs and calling the pattern deterministic is a guess
with a table around it** — enumerate the window and fingerprint the artefact, because the *effective*
version is what the runner installed, not what the branch had merged. And **a rate is not a detail**:
"100%" and "90%" imply opposite verification strategies, and only one of them is safe.

## 2026-08-27 — the merge gate itself counted SKIPPED as success, and `main` has no branch protection

Two facts, found by PR #1074's Reviewer while reviewing something else, that change how this crew
should read a green CI verdict.

**`scripts/ci-poll.mjs:341` counts `SKIPPED` as success.** `ci.yml`'s five Rust test jobs sit behind
`needs: lockfile-preflight` with no `if:`, so a preflight failure makes GitHub **skip** all five —
and `ci-poll.mjs` then reports **`CI VERDICT: completed success`** on a run where the entire Rust
suite never executed. This is the fifth instance of the same family today (a bash `-lt` returning 2, an
npm `--json` error path with no `metadata`, a catalog job green having published nothing, CI jobs
skipped rather than failed) — and it is the worst placed, because it sits inside **the gate the
Foreman merges on**. Routed into CPE-1906, which already owned the "an error reads as pending" half.

**`main` has no branch protection.** `branches/main/protection` → 404, `rulesets` → `[]`,
`branches/main` → `"protected": false`. So there are **no required status checks on this repo**, the
`--admin` merges bypass nothing, and `ci-poll.mjs`'s verdict is effectively the only gate. That is
what makes the point above matter rather than being a nuisance: there is no second line of defence
behind it.

**The transferable part is where the finding came from.** It was not found by auditing the poller. It
was found by a reviewer asking, of a *different* PR, "what does this actually buy today?" — and
following that question into the tool that consumes the thing being changed. The PR's own author had
argued the benefit was a latent one (a skipped job satisfying a required check, which cannot happen
here because nothing is required). The live benefit was one layer down and larger. **When a change
guards against a hazard, check what already consumes the signal it is changing** — the honest answer
to "is this worth it" is often in a different file than the change.

## 2026-08-27 — I asked for "treat SKIPPED as not-success" and the worker was right to refuse the blanket version

Having found that `ci-poll.mjs` counted `SKIPPED` as success, I routed the fix with a strong steer:
treat a skip as "did not run", because that is the house rule the day's other four fail-opens
established. The worker measured before complying and found the steer would have broken the tool:
**`GUI smoke (windows-latest)` is SKIPPED on every PR**, by its own job-level `if:`. A blanket rule
would have made every board red forever — which is not fail-closed, it is broken, and it would have
been reverted within a day, taking the real fix with it.

What shipped instead derives the distinction at run time from `.github/workflows/*.yml`: a job with an
`if:`, plus the transitive `needs:` closure behind it, is **skipped by design**; anything else **did
not run**. An empty scan treats every skip as unexplained, so the derivation itself fails closed.

**The lesson is about the shape of fail-open fixes, and it is easy to get wrong in the safe-feeling
direction.** "Did not run must not read as success" is correct. "Therefore every not-run signal blocks"
is not — it collapses two different facts (*this was deliberately not applicable* and *this should have
run and did not*) that the tool has to tell apart. The discrimination is the work; treating the whole
category as failure only looks like rigour.

Two more things came out of the same pass. The sweep found a **second** fail-open script —
`organize-done.mjs` printed a failed auto-commit to stdout and exited **0** *after renaming files*,
and `git diff --cached --quiet` could not tell exit 1 from exit 128, the same exit-code confusion as
the bash `[ -lt ]` returning 2. And **job age is now reported but deliberately not thresholded**: the
median run here is 58.9 minutes, so any invented "over N minutes = hung" fires constantly. Report the
number and the name; let the caller compare against a sibling. I spent an hour today doing that
comparison by hand.

## 2026-08-27 — a reviewer found the hole by reading and called it latent; the guard found it by running and called it red

PR #1075's Reviewer raised, as a non-blocking **F3**, that a `Scene::planted()` which fails to plant
its link would `return` and **pass** — *"the doc says it 'skips loudly', but `eprintln!` is captured by
default, so on a CI leg it would be silent."* It graded this **"latent, not active"**, having verified
both platforms plant successfully today.

An existing repo guard, `fsutil::tests::skip_notices_never_use_a_captured_print_macro`, disagreed —
and it was not speculating. It scans the whole repo, found both `eprintln!` skip notices in the new
`catalog_staging_containment.rs`, and **failed the Windows and macOS legs of the same PR**. Not
latent. Red, now.

Both found the same hole; only one got the severity right, and the difference is that the guard runs.
Its message is worth quoting because it is doing the arguing for us:

> these skip notices use a print macro whose output libtest SWALLOWS for a passing test — so they
> announce a leg that verified nothing to nobody, on the only harness that matters
> […] **Better still, if the staging mechanism is supposed to work on that platform, use
> `fsutil::require_staged` and let the leg go RED under CI instead of printing into a green log.**

**The sharpest version of the lesson:** the test in question is the **sensitivity control** for a
security fix — its whole job is to prove the escape still happens when the fix is disabled. A control
that silently returns green because it could not set itself up proves nothing, and proves it
*invisibly*. That is worse than having no control, because the green reads as coverage. The PR's own
author had already made this exact argument one step earlier, when it strengthened the fixture to
plant at the **real** temp path instead of a stand-in: *"a stand-in is unreachable by any regression
and every assertion about it would be unfalsifiable."* The same reasoning applies to its own skip
path, and neither the author nor the reviewer carried it that one step further.

Also worth keeping: the guard **states what it does not catch** — a notice that never says "skip",
one assembled into a variable first, a test module not called `mod tests`. A guard that publishes its
own blind spots is the opposite of the day's other defect, where green tests sat beside claims nobody
had run.

## 2026-08-27 — the ticket that exists because coverage claims went unmeasured shipped an unmeasured coverage claim

The lineage, in order, all on the same two files:

1. **CPE-1919 round 2** — contrast ratios written as measured were estimated (`3.53`/`1.90` vs the real
   `2.81`/`2.44`).
2. **CPE-1919 round 3** — *"there is no general theme-parity guard in this repo"*, false for the `dark`
   block, disproved by deletion. **CPE-1962 was filed to close that gap.**
3. **CPE-1962's own PR** — its new guard header says the `hc-dark` deletion is now caught by
   *"the first two"* files. Its Reviewer ran that deletion against the second one: **fully green, 22/22.**
   Only `hc-contrast.test.ts` changed behaviour. Third unmeasured coverage claim, in the guard header
   whose stated job is recording **measured** coverage, in the PR filed because of the first two.

Plus a fourth of the same family in the same diff: a red-proof tally written as `(1 failed, 23 passed)`
in a file with 23 tests, where 22 pass — **and the same PR records 22 correctly elsewhere**, so the
diff contradicts itself.

**Why it keeps happening is worth naming precisely.** Every author here was careful, and each one
measured the thing they were *changing*. What none of them measured was the sentence *about other
files* they wrote while doing it — because that sentence feels like description, not like a claim.
It is a claim, it is cheap to check (delete the token, run the other file's tests), and it lands in a
comment that the next reader trusts precisely because a green test sits beside it.

**The countermeasure that works is not "be careful" — it is that every coverage sentence names the
deletion that proves it.** The reviews that caught all four did the same thing each time: they deleted
something and ran the other file. That is a five-minute check and it has now caught four defects in
three PRs.

Worth recording on the other side of the ledger too: this PR's new guard, on its very first run,
found `--log-warn` missing from **both** high-contrast blocks — the log viewer's WARN badge at 10.5px
measuring **3.54 / 3.28 / 2.94** in hc-dark, below AA's 4.5:1 floor on every surface, in the
*high-contrast* theme. An independent from-spec WCAG implementation reproduced every digit. The guard
is worth having; its header just has to be as measured as its assertions.

## 2026-08-27 — I read a stack trace off the wrong spec, and drew a boundary from two data points

I filed **CPE-1965** off a shard-4 failure, quoting `WebDriverError: element not interactable` as the
symptom and arguing at length that it was **not** CPE-1960's shape. The worker checked the timestamps:
that line is at **01:39:01.382**; `organize.smoke.ts` does not start until **01:39:28.210**. It belongs
to an **earlier spec**, where a `waitForClickable` poll absorbed it and it failed nothing. The actual
failure was `expected a PNG group for the seeded CPE-1143-photo.png` — a `waitForExist` timeout.

I had grepped the job log for error-shaped lines near the failing spec's name and taken the nearest
one. In an interleaved 14-spec shard log, "nearby" is not "related."

**The second error is worse, because I had just written the lesson down.** I compared `main` at 01:31Z
(shard 4 clean) with #1074 at 01:38Z (one extra failure) and concluded #1074 carried something new.
The worker enumerated **all 103 GUI-smoke runs** in the window, resolved the shard-4 job for each, and
downloaded **69** completed logs:

| install | wdio | jobs | failures | rate |
|---|---|---|---|---|
| `added 479 packages` | 9.30.0 | 51 | 1 | 2.0% |
| `added 489 packages` | 9.31.4 | 18 | 2 | 11.1% |
| **total** | | **69** | **3** | **4.3%** |

Fisher p ≈ 0.15 — **not a version effect**. One of the three predates the lockfile bump entirely, so it
is not a CPE-1960 casualty. And **`main` itself carried the failure at 00:14Z**, forty minutes before
the run I called clean. *"#1074 has one extra"* was a **sampling artefact** — the identical mistake
that produced three wrong characterisations of CPE-1960, made by me, in the ticket I filed to stop it
happening again, hours after writing *"sampling a handful of CI jobs and calling the pattern
deterministic is a guess with a table around it."*

**Knowing the rule is not the same as applying it, and the tell is cheap:** any claim of the form "this
branch has something main doesn't" is a **rate comparison**, and a rate needs the denominator. Two runs
is not a denominator. The enumeration cost the worker one pass of `gh api`.

The diagnosis it reached instead is better than either of mine: a **reflow between the driver computing
a click point and dispatching it**. `.preview` grows `120px` → `45vh` when the first plan lands ~120 ms
after mount, sliding the centred dialog's rule pills **up ~98 px**, so the click lands in `.preview`
— whose ancestor has `on:click|stopPropagation` and swallows it **silently**. Measured driver gaps
across 69 logs: 27–159 ms; all three failures at 115/116/117 ms. `waitForClickable` is no protection —
it passed in every failing run.

## 2026-08-27 — the first run of the new stale-checks rule found the exposure real, across the whole queue

CPE-1970 says: before merging, check whether the PR's checks predate a guard that has since landed on
`main`. The cheap instrument I wrote for it was *"#1074 added a `CI verdict` job when it merged — if a
PR's rollup lacks that check, its run predates the merge."*

**First application: all nine open PRs lack it.** Every one of them would have merged without ever
being judged by a gate that landed 45 minutes earlier. The exposure is not hypothetical and it is not
one PR — it is the default state of a queue that moves slower than `main` does.

**Two things about how that check went are worth keeping.**

**First, my initial query was wrong and said the same thing.** I filtered on `contains("CI verdict")`
and got zero across the board — which looked like confirmation. It was not: `#1073` *does* carry a
check with "verdict" in the name, the **GUI smoke** one (CPE-1753), and my filter's case and wording
happened to miss both it and the real target. I nearly reported a correct conclusion reached through a
broken measurement, which is the same defect as a wrong conclusion and harder to catch, because the
answer looks right. Checking *why* the count was zero is what separated them.

**Second, the exposure was dischargeable without waiting.** `ci-verdict`'s only failure mode is a job
behind `lockfile-preflight` that was **skipped**. So its verdict is **derivable** from a rollup that
already exists: list the five job families on each PR and confirm none is `SKIPPED`. They are all
`SUCCESS` or still running. The gate never judged these PRs and its judgement is knowable anyway.

That distinction is the useful one, and it generalises past this gate: **"the guard did not run" is not
automatically "we must re-run the guard."** Ask first whether the guard's verdict is a function of data
you already hold. When it is, derive it and say so in the merge record. When it is not — a guard whose
input is the diff itself, or one that measures something the rollup does not report — a fresh run is
the only honest answer. The cost difference is an hour of CI per PR, times nine.

## 2026-08-27 — I read an empty board as a green one, hours after merging the fix for that

Checking the queue with my own `jq` one-liner, #1081 came back `pend=0 fail=0` — which is what a fully
green PR looks like. It was not green. Its head had just been force-pushed and **no checks had
registered yet**: `statusCheckRollup | length` was **0**. Zero pending and zero failed, because there
was nothing there at all.

`scripts/ci-poll.mjs` has carried the correct verdict for this for hours — *"pending … an empty board
is NOT a green one"* — and #1078's Reviewer had listed it in its matrix that same evening. I did not
use ci-poll for that check; I wrote a quick `jq` filter, and the quick filter had the defect the real
tool exists to prevent.

**The pattern is now three-for-three in this session and it is always the same shape:** an ad-hoc query
returns a number that agrees with what I expected, and the number is an artefact of the query rather
than a fact about the world. Earlier tonight a case-sensitive `contains("CI verdict")` returned zero
across nine PRs and read as confirmation of a real problem; before that, two data points read as a
boundary that a 69-job enumeration disproved. Every time, the tell was available in one extra call —
`| length`, a case-insensitive grep, a denominator.

**The rule I actually need is narrower than "be careful":** *when a hand-rolled query agrees with the
conclusion I already had, spend one more call establishing that the query can produce a different
answer at all.* An empty result and a negative result are different facts, and only one of them is
evidence. Where a real tool already answers the question — and here one did, with the right verdict
already written into it — use the tool.

## 2026-08-27 — two independent parties vouched for a number neither had derived

PR #1084's Reviewer found a stale count in `archive.rs`: the reconciliation paragraph claimed *"8
`create_dir_all` calls"* when CPE-1913 had removed two, so the real count is **6**. It reported that
precisely, and in the same breath said `File::create` = **12** "is still right". I passed both to the
round-2 worker in those terms.

The worker counted. **`File::create` is 11.** CPE-1913 replaced row 16's `fs::File::create(&out)` with
`claim_destination_handle` + `create_beneath`, so rows 2-14 own all eleven and row 16 owns none. **Both
wrong numbers are exactly the two that CPE-1913 changed** — the same commit, the same reason, sitting
one line apart, and the Reviewer caught one and vouched for the other.

**That is the day's central defect in its cleanest form.** The Reviewer was not careless — it had just
*derived* the `create_dir_all` count from source. Having done that work, the adjacent number felt
checked. It was not; it was **adjacent to something checked**, which is a different thing and feels
identical. Then I relayed the vouching in a brief, which is how a claim acquires a second author
without acquiring a second measurement.

**The narrow rule that would have caught it:** when you derive one number out of a group that a single
change touched, derive **all** of them. A stale count and a fresh count look the same on the page, and
the reason they are in the same paragraph is usually the reason they went stale together.

The comment itself says why this matters, and it is worth quoting because it was right and still did
not save it: *"the count line exists so the next reader can check that claim in one subtraction instead
of trusting it."* Three readers trusted it.

## 2026-08-28 — a net count concealed two errors pointing opposite ways

CPE-1952's round 2 established a corrected recipe for enumerating `temp_dir()` sites and reported the
naive rule finding **10** where the corrected one finds **15**. CPE-1964's worker re-derived it and got
**14**, correcting the ticket: the extra was a doc-comment line. Honest, and I recorded it as a
one-number correction.

Its round 2 re-derived **both halves** rather than carrying 14 forward, and the difference is not one
number at all. **−4 is a net of two opposite errors:**

- the naive rule **misses five** real sites — `console.rs:733` and `:796` (**both swarm sites**, the ones
  the ticket existed for), plus `fsutil.rs:5578`, `src-tauri/src/lib.rs:11904` and `:14416`;
- and it **adds one spurious** hit, `archive.rs:2088`, a doc-comment line.

**The recipe hid the defect and padded the count back up.** A reader comparing 10 against 14 sees a gap
of four and reasonably infers four missing sites; the real shape is five missing and one invented,
and the invented one is what makes the gap look smaller than it is.

**The general point: a difference between two counts is not evidence about either.** Whenever a
correction is expressed as a delta — "the naive rule finds N fewer" — the errors can be flowing in both
directions and cancelling, and the cancellation is invisible in exactly the summary a reader will
quote. Derive **both sides**, and report **what moved in each direction**, not the net.

This is the fourth stale-number finding of the shift and the first where the number was arguably
*right* while the thing it described was wrong. The earlier three were plain staleness; this one
survived a correction, because correcting a net to a different net does not test the decomposition.

## 2026-08-28 — the anchor validator validated a copy

CPE-1966 built a contrast-sweep harness whose best feature was that it **refuses to run** unless it
reproduces five known WCAG anchors — `#000/#fff = 21.00`, `#767676/#fff = 4.54`, and two compositing
values. That validator fired for real during development: the author's first draft had **3.95 from
memory** where 50% black over white is **3.98**. A measurement tool that will not start until it can
prove it measures correctly. It is the right idea.

Its Reviewer then multiplied the **probe's** `ratio()` by 1.6 and got:

```
  #000/#fff  21    50% black/white  3.98    #767676/#fff  4.54
  1306 raw readings -> 786 distinct sites, 384 enforced
  light: pixel cross-check — 59 grounds screenshot-verified, 0 disagreeing by more than 1/255
PASS — every enforced site clears its bar in both schemes.          EXIT=0
```

**All five anchors green. Both independent cross-checks clean. PASS, exit 0. Every number 60% wrong.**

The validator exercises the **module-level** maths in `engine.mjs`. Every site ratio is computed by a
**second, independent implementation inside the probe source** that runs in the page — and no anchor
ever touches it. The harness had two copies of the same arithmetic and validated the one that does not
produce the answers.

**Two more safeguards failed the same way in the same PR.** `--verify-pixels` — the second independent
path, and the PR's strongest claim — reported *"59 disagreeing by more than 1/255"* and **exited 0**,
because the exit condition never consulted it. And the fixture-provenance check, which exists so the
harness cannot measure a DOM the app no longer renders, is a raw `includes` over unstripped script
text: **a comment mentioning the old class name satisfies it** — CPE-1933 rule 2, in the file that
cites CPE-1933.

**The pattern, stated once:** a safeguard is not validated by the fact that it *can* fire. All three of
these fire correctly on the input their author had in mind. What none of them had was a test that the
safeguard is **wired to the thing it is guarding** — the validator to the code that computes, the
cross-check to the exit code, the provenance check to code rather than prose.

**The cheap discipline that would have caught all three: sabotage the thing being guarded, not the
guard.** Breaking the guard proves it can fail. Breaking the *subject* and watching the guard stay
green is what proves the wire exists — and it is the same "force the predicate to lie" half of
CPE-1929's pair, applied one level out.

## 2026-08-28 — two PRs in a row: the fix was right, the guard around it overstated

Back to back, two independent reviews landed the same verdict in different subsystems:

- **CPE-1966 / #1087** — *"the engineering is good and most of it holds up under attack. Three things do
  not, and all three are in the machinery the brief said to weigh hardest."*
- **CPE-1954 / #1088** — *"the security fix itself is correct, genuinely demonstrated, and I would
  approve it on the code alone. Two findings are about the guard layer around it overstating what it
  enforces."*

Neither fix was wrong. In both cases the **claim about the guard** was, and in both cases the claim sat
next to a green test that read as vouching for it.

**The sharpest instance was a claim about layering.** #1088 protected a parse two ways — the
constructor made `pub(crate)` (the compiler refuses the old spelling: `error[E0624]`) and a repo-wide
scanner for the `serde_json::from_str::<CatalogIndex>` back door, since the type is still `pub` +
`Deserialize`. The comment said *"each covering what the other cannot"* and *"neither alone is the
invariant."*

Its Reviewer wrote **one line** — `type Idx = sidecar_host::catalog::CatalogIndex;` — and defeated
**both at once**. It compiles, forms a real escaping path at runtime, and the scanner stays 16/16 green.
**Neither *together* is the invariant.** Seven more vectors survive all three regexes: `use … as`, a
generic turbofish, a `#[serde(flatten)]` wrapper, return-position inference, `Self::from_json` inside
the crate, a `TryFrom` impl, and a `Vec<CatalogIndex>` annotation.

**Two layers is not evidence of depth.** Both of those layers key on the *spelling of a type name*, so a
rename defeats them together — they are one layer wearing two coats. The test for "defence in depth" is
not *how many checks* but *whether an attack that defeats one also defeats the other*, and that is a
question you answer by trying it, not by counting.

**The available structural fix makes the point.** Nothing outside that crate names the type at all, so
the `Deserialize` derive can go behind a private wire type and **the compiler becomes the invariant** —
no scanner, no claim, nothing to go stale. A guard you can delete is better than a guard you have to
describe.

## 2026-08-28 — a third answer to the shadowed-guard rule: neither reorder nor delete, but a seam

CLAUDE.md's CPE-1929 standard says a guard that fails both sabotages — green when disabled, unchanged
when its predicate lies — is shadowed, and **the fix is reorder or delete; leaving it shadowed is the
one wrong answer, because it reads as coverage.**

CPE-1961 found `batch_media`'s `links > 1` census failing both sabotages at **2,430 / 0**, and then
argued a case the standard does not cover:

> the path probe reads the **name** before the open; the census reads the **object** after it — so a
> link planted **between** them is visible only to the census.

**Reorder was impossible** (the two checks read different things at different times, and the order is
forced by what each can see) **and delete would have been wrong** (the census is the only thing that
can see that window). The guard was not redundant — it was **unreachable by the existing tests**,
because nothing in the suite could plant anything in the interval between the two checks.

The answer was a **`between_containment_and_open` seam** plus a test that uses it. Both sabotages now
red at **2,430 / 1**.

**So the rule needs a third branch, and the distinction is worth stating precisely.** Two green
sabotages mean *nothing in the suite can reach this guard*. That has three possible causes, not two:

1. **Redundant** — an earlier check answers the same question, so nothing *can* reach it → **delete**.
2. **Misordered** — a later check asks the more trustworthy question → **reorder**.
3. **Unreachable-by-test** — the guard answers a question no other check answers, but the input that
   would trip it can only arise in a window the tests cannot open → **build the seam.**

Case 3 looks exactly like case 1 from the outside, and the way to tell them apart is the question the
standard already asks about reordering: *what does each check actually see, and when?* If the shadowing
check reads a different thing at a different moment, it is not shadowing — it is merely earlier.

Same PR, the other direction: it also declared a *new* guard **untestable by construction** (disable →
green, lie → 79 failed) and **said so at the site with both numbers**, which is what the standard asks
for when a backstop genuinely cannot be reached. Both dispositions in one diff, argued separately.

## 2026-08-28 — the report claimed a leg ran, because the count came from intent rather than work

CPE-1966's round 2 fixed three safeguards that stayed green while permitting what they existed to
prevent. A **narrow** re-review — scoped to only the round-2 changes — found the same class again, in
the legs the PR exists to add.

Three of the four measurement legs could measure **nothing** and still print
`PASS — every enforced site clears its bar in both schemes.`, exit 0:

- **states**: `const stateRules = []` → PASS, 844 readings instead of 1306. And it can happen with
  nobody editing anything, because the loop wraps its query in a bare `catch { continue; }` — a CDP
  change makes every rule skip **silently**.
- **base**: `const all = []` → `0 raw readings -> 0 distinct sites, 0 enforced`, then PASS. Caught under
  `--verify-pixels` only **incidentally** (the pixel leg is fed from `all`), so the documented local
  invocation — the package.json script, no flag — passes on zero measurements.
- **time**: disabled → PASS, **and the log still prints `3 CSS animations x 21 frames`.**

**That third one is the one worth keeping.** The count is read from `animMeta.count` — the metadata
describing what the harness *intended to sample* — not from readings that exist. So the report does
not merely fail to notice the leg did nothing; **it actively asserts the leg ran, with a plausible
number.** A silent zero invites suspicion. A confident "3 animations × 21 frames" closes the question.

**The rule: every number a report prints must be derived from work done, not from the plan.** Counts
sourced from configuration, targets, or intent will keep reporting the shape of the job after the job
stops happening. The tell is asking, of each figure, *what would this print if the loop body never
ran?* — and `animMeta.count` prints the same thing either way.

Two smaller notes from the same pass. `--json` returned **0 unconditionally**, evaluating no verdict at
all. And the round-2 JS comment stripper failed **both** ways — four shapes silently deleting real code
(`return /[//]/;` → `return /[ `, because `prevSignificant` is a single character so every keyword
matches `[\w$)\]]`) and three letting a comment survive. One path tokenized the **entire HTML
document**: a single apostrophe in prose outside every `<script>` shifted the output by **11,872
characters**. Latent only because the swallowed region happened to be copied verbatim.

**And the narrow re-review is the reusable part.** A round-2 fix is new code that no review has seen;
scoping a pass to *only what changed* cost a fraction of a full review and found two blockers the
original reviewer could not have — it was reading round 1.

## 2026-08-28 — a doc cited a guard by name, and the guard had never existed

`ArchiveReport`'s doc comment has said, since **CPE-1775**, that its count/reason invariant is enforced
by `skipped_count_matches_the_recorded_reasons_on_every_streamed_skip_path`. CPE-1935's worker went
looking for it: **two grep hits — the sentence itself, and its copy carried into `bindings.gen.ts`.**
The test has never existed.

**This is the purest form of the CPE-1933 defect the repo has found.** The other instances were claims
that were *once* true and went stale, or claims about another file's behaviour that nobody ran. This
one was **never true at any revision** — and it survived because a named test in a doc comment is the
most convincing possible citation. It reads as a pointer to something you could go and check, which is
exactly why nobody did.

**It also propagated.** The sentence was copied verbatim into the generated TypeScript bindings, so the
false claim now had two homes and would have read as corroboration to anyone who grepped for it.

**The cheap check, and it is genuinely cheap:** any comment naming a test, a function or a job as its
enforcement is a **derivable** claim — grep for the name and count the hits. Two hits where one is the
comment itself means the citation is empty. That is one command, and it would have caught this in 2026
when it was written.

It was replaced with a real source-derived guard, which is the right ending.

## 2026-08-28 — the entry-verdict rule: scope first, severity second

CPE-1935 and CPE-1938 (#1084) appeared to hold opposite positions on per-entry failures. #1084 kept an
`Abort` for a transient `ENOENT` where `main` had degraded to a per-entry Skip, and its Auditor flagged
that as *"the same 'one planted junction = total denial' shape it just fixed, re-entered through
another door."* CPE-1935's ticket asked for the opposite. I briefed it as a conflict and asked for a
rule.

**It found the two were never in conflict — they were being told apart by the wrong question.**

> **Scope — what is this evidence about?** The **one name the archive asked for** ⇒ *entry* verdict.
> The **extraction folder**, **a path component many entries travel through**, or **the archive
> container** ⇒ *run* verdict.
>
> **Severity — did anyone choose not to write?** A guard chose ⇒ `Skip`. The filesystem refused ⇒ `Fail`.
>
> **The leaf is the archive's business; the chain is the run's.**

`entry_component_action` walks *directory components*, which every sibling beneath them travels
through, and `create_dir_beneath` **creates** missing levels — so a refusal there is the destination
being mutated under a run in progress, and #1084's `Abort` was right all along. It stayed unchanged.

**The generalisable part is the diagnosis, not the answer.** One enum was carrying two orthogonal
questions in three arms, so every argument about it was really an argument about which question the
arms encoded. When two careful people reach opposite conclusions about the same enum, the enum is
usually the problem.

## 2026-08-28 — a gate reported from a platform where the code does not build

PR #1089's body reported `crates/server --lib` Linux at **2,414 / 0 / 13** and clippy *"clean … under
the WSL toolchain."* Its Reviewer tried to reproduce and found the tree **does not compile on Linux or
macOS**: a rename left a `#[cfg(unix)]` block referring to a variable that no longer exists, so Windows
builds and Unix does not. With the fix applied the real number is **2432**; **2,414 is unreproducible
either way.**

**This is a different defect from the stale numbers this shift has been finding, and worse.** Those
were figures that had been true and drifted. This is a **reported measurement of work that cannot have
happened** — the platform in question could not produce any number at all. On a security PR, in a repo
that spent the day removing exactly that class of claim.

**The mechanism is almost certainly innocent and that is the point.** A Linux run taken before the
rename, or from a stale worktree, or carried forward from an earlier round, all produce the same
artefact: a plausible number attached to a run that never happened on this code. Nobody fabricates
these; they **survive an edit** the way a stale comment does, and a test count looks far more like
evidence than a sentence does.

**Two cheap checks, both of which this would have failed.** A gate table should be reproducible from
the branch as pushed — so *re-run it after the last edit*, not before. And when a number does not move
across a round that changed code, ask whether it was re-taken or copied; **2,414 appearing beside a
tree that cannot compile is the tell.**

Worth pairing with the same PR's other finding: **three newly-added doc sites named the exact Win32
call the PR itself had measured as refusing this operation** — including the *public* doc of the new
primitive — while the implementation correctly used the NT form and carried a warning against
"simplifying" it back. The reader most likely to attempt that simplification is the one reading the
public doc, and it told them the wrapper was what's used.

## 2026-08-28 — the evidence and the claim were about different code paths

PR #1089's headline is a handle-relative rename that resolves no path. Its evidence is a race harness
whose two arms go from live to **zero** on both platforms, reproduced by two independent gates. Both
halves are true. **They are about different code.**

The new primitive is reached only from one of two destination shapes. **Both racer arms take the other
one**, which commits by path on both platforms — and the Security Auditor measured that shape still
aliasable at **19 / 2,000 on Windows**, against a control of 0. So the PR's zeros are real zeros for a
path its headline is not about, and the sentence *"fully closed on Windows"* is false for the two legs
whose numbers it prints beside it.

**Nobody misreported anything.** The site comment states the split correctly; the PR body and the
ticket summary do not, and those are what get read. The zeros came from the arms that existed, which
were written for the *previous* ticket — and the new primitive, the thing every future ticket in this
family will build on, **ended up covered by no race arm at all.**

**The check is one question, asked of the evidence rather than the code: which code path does this
number exercise, and is it the one the headline names?** A harness inherited from the previous ticket
answers the previous ticket's question. When a fix introduces a new path, the arm has to be added, or
the claim has to shrink to what was actually measured.

## 2026-08-28 — a hardening that handed an attacker a new denial primitive

The same PR reused an existing carryover helper so the destination's ACL, attributes and alternate data
streams survive an overwrite. Sound, and it fails the entry rather than silently downgrading. But the
helper carries an 8 MiB budget for streams, and **writing an alternate data stream needs only write
access to the file.**

Before, five legs — backup, restore, extraction, download, revert — truncated and wrote, and never read
streams at all. After, **any process that can write a file can permanently break every one of those
operations on that name.** And the refusal is a *failure*, not a *policy* verdict, so the archive leg's
match arm returns instead of skipping: **one planted stream on one pre-existing name aborts the entire
extraction.**

**The general shape: inheriting a helper inherits its refusals, and a refusal that was proportionate on
the leg it was written for may not be on the legs you just gave it.** The original leg was a
user-confirmed single-file overwrite where failing loudly is right. A bulk extraction is not that.
Ask, for every leg newly reached: **who can trigger this refusal, and what does it cost them to do it?**
Here the answer is "anyone with write access, once" and the cost is an entire archive.

The tell was cheap to spot and nobody spotted it: the refusal's own message still said *"this
**confirmed overwrite** will not copy across"* — on four legs that are not confirmed overwrites. **A
message naming an operation the caller did not perform is a sign the code reached somewhere it was not
written for.**

## 2026-08-28 — "untestable by construction" is where shadowing hides

CPE-1929's rule catches a guard that is unreachable because an earlier guard answers the same question.
Its tell is a pair of green sabotages. #1089 carried a guard labelled **"untestable by construction"**
with the pair recorded honestly beside it: disabling it left the suite green, forcing its predicate to
lie changed nothing.

The Security Auditor worked out why, and it is not untestability. **The only instrument that can make
the guard fire is thread-global, so arming it trips the guard's own twin, three lines earlier in the
same function, and that arm returns first.** It is shadowed — by itself. The recorded "79 tests red
when the predicate lies" were all hitting the earlier copy.

**"Untestable by construction" and "shadowed" produce identical evidence, and only one of them is a
reason to stop looking.** The label is what did the damage: it explains the green pair, so the pair
stops being a question. The rule should therefore read — *when you cannot test a guard, name the reason
it cannot be reached, and check whether that reason is another guard.* Here the honest comment is
"shadowed by the destination-handle check, deliberately kept as a fail-closed backstop", which sends the
next reader to the shadowing check instead of leaving them to conclude the guard is inert.

Keeping it is still right: it fails closed, and deleting it fails **open**. This is the fourth distinct
answer the family has produced — redundant → delete; misordered → reorder; unreachable-by-test → build
the seam; **shadowed but load-bearing → keep, and say what shadows it.**

## 2026-08-28 — a guard whose verdict depends on the checkout's line endings

PR #1090 replaced a comment citing a test that had never existed with a real guard. The guard has two
opposite failures on the two platforms CI runs:

- **On LF it is red — against itself.** Its comment stripper only cuts at `//`, so the guard's own list
  of forbidden fragments is scanned as code and it reports its own line as the offender. Both Linux and
  macOS jobs fail.
- **On CRLF it is blind.** It locates the end of each exempt helper with `find("\n    }\n")`. **In a
  CRLF file that pattern occurs zero times** — the CRLF form occurs 230 — so both spans fall back to
  `src.len()` through an `unwrap_or`, and every line after byte 251,835 of 713,733 counts as "inside a
  helper". **Roughly 65% of the file, including all three stream extractors and the entire test module,
  is unguarded on Windows.**

**One `unwrap_or` turned "I could not find the end of this span" into "the span runs to the end of the
file",** which is the widest possible exemption. That is the fail-open family again, and this time it is
*inside* a guard: "did not run" reading as "ran and found nothing", the exact rule CLAUDE.md states.

**Two checks, both cheap.** Any scanner over Rust source must be run once against an **LF** checkout and
once against **CRLF** — this repo has both, because CI is Linux and macOS and development is Windows. And
**a span locator must fail loudly when it cannot find its end**, never widen to the whole file.

## 2026-08-28 — the red-proof described a mutation the code cannot express

The same guard's doc said it *"fails if `skipped`/`failed` is incremented or `errors` pushed anywhere but
inside these two helpers"*, and red-proofed that as *"adding `self.failed += 1;` to any extractor leg
turns this red."*

**Extractor legs have no `self`.** They hold a local `report`. The Reviewer planted `report.failed += 1;`
and `report.errors.push(...)` directly inside an extractor and the test **stayed green**. The guard can
only ever catch a new `self`-receiver method inside the one `impl` block — and on Windows, only one
defined before a particular line.

**The red-proof was not weak, it was impossible** — it named an edit the codebase's own shape forbids.
And because a red-proof is prose until someone runs it, nothing objected. CLAUDE.md already says *change
the referenced source and watch the test fail*; the sharper form is: **the mutation you name must be one
the code can actually express, and you must have performed it.** Write the result at the site — "planted
X at line N, test failed" — not the intention.

**Related and worse in the same PR:** its headline finding was that a comment cited a test which had
never existed in any commit. In fixing that, it **planted a fresh phantom citation** — in the module's
canonical rule block, pointing at the very test the PR had just renamed away. Along with seven other
sites still asserting the rule the PR inverted. A comment that names a test is derivable in one `git
grep`; the ones that rot are the ones nobody thinks to grep because they read as background.

## 2026-08-28 — a "pending" check whose job had already finished

`ci-poll` reported #1088's oldest pending check as `Server crates (macos-latest)`, running 38 minutes,
and helpfully suggested comparing it against a sibling to tell slow from hung. Both readings were wrong.
Pulling the job's **steps** showed all 49 completed — including `Complete job` — every one `success`, at
08:54:34Z. **The job was finished; only the check-run status was stale.**

The genuinely outstanding work was the *other* pending check, Windows, sitting at step 21 of 24 with
three crates left. So the poll's "oldest pending" pointed at the one thing that needed no waiting at all.

**The technique, and it costs one API call:** when a check looks slow, do not compare its elapsed time
against a sibling — **read its steps.** `gh api repos/:owner/:repo/actions/jobs/<id> --jq '.steps[]'`
answers three questions the check-level view cannot: whether it is progressing, *which* step it is on,
and whether it has in fact already finished. Elapsed time alone cannot distinguish a slow job from a
finished one from a wedged one; the step list distinguishes all three immediately.

Worth remembering that "compare against a sibling" was advice this crew wrote into the tool earlier this
same shift, after a different misread. It is decent advice and it is strictly weaker than just looking.

## 2026-08-28 — the documented "known gap" was the blocker, wearing a label

PR #1087's round-2 blocker was a comment stripper that deleted source: a regex literal after a keyword
whose character class contained `/*` made the scanner invent a comment and eat to the next `*/`. Round 3
fixed it properly — root-caused to tracking the previous *character* instead of the previous *token* —
and shipped 29 tests plus a written list of three **KNOWN GAPS**, each declared to *"fail toward KEEPING
source rather than deleting it"*, each pinned by a passing test.

**Gap 1 was the same defect, reached through a different prefix.** A regex after `)` or `]` is read as
division — and when its class contains `//` or `/*`, the emitted `/` walks straight into the comment
branches that sit *earlier* in the scanner than the regex branch. Parseable JavaScript in, unparseable
out, **144 characters deleted.** The narrow re-review built the input and measured it.

**Three separate things made it invisible, and they are the transferable part:**

1. **The gap list was written from the *cause* rather than measured.** "A regex after `)` is read as
   division" is true, and "division emits verbatim" is true, and the conclusion is still wrong, because
   the emitted text re-enters the scanner. Nobody ran a deleting input through it.
2. **The pinning test used the benign form of the case.** The table's gap-1 entry is `/re/`, which really
   does keep. So the case was pinned, the oracle ran, everything was green — and the shape that fails was
   simply not in the table. **A case table proves what is in it; it says nothing about what is missing,
   and a green table reads like coverage of the whole class.**
3. **The comment promised the oracle would catch a regression** — *"if any of these ever flips to
   deleting, the `parses()` oracle below reds it"* — but the oracle only iterates the table.

**The rule: a declared limitation is a claim, and claims get measured.** Writing "this fails safe" next
to a green test is the CPE-1933 shape exactly — the test vouches for the sentence without testing it.
Either construct an input for each declared gap and record which way it actually errs, or do not
characterise the direction.

Worth noting the pattern across this shift: **the narrow re-review — scoped to only what changed since
the last round — has now found a blocker on this PR three times running**, each one invisible to the
full review that preceded it, because each was created by the previous round's fix.

## 2026-08-28 — the fix that made the promise true instead of narrowing it

#1087's round-4 blocker offered two ways out. **(1)** Correct the false "known gap" text and pin the
deleting shapes as cases. **(2)** Change the scanner so the deleting case cannot happen. The worker took
(2), and the shape of it is worth keeping.

The bug: a regex literal after `)` was read as division, and when its character class contained `//` or
`/*` the emitted text walked into the comment branches. The instinct is to special-case the class
contents. **The actual fix was to stop asking the wrong question.** A `)` is now decided by **what its
`(` opened** — a per-frame paren stack plus a small set of control keywords — never by the `)` itself.
That is what real tokenizers do, and it is the same correction round 3 made one level down: *decide on
the previous **token**, not the previous character.* The same mistake, one scope up.

**What made it verifiable, and this is the part round 3 lacked.** Emptying the keyword set now reds
**6 of 48** tests — the four named cases, an explicit `−144 → 0` measurement, **and the parse oracle
itself**, because four cases become genuinely unparseable. In round 3 the oracle iterated a table whose
`)` entry was the benign form, so it could not fire. **An oracle only proves something when the table
contains a case that can break it.**

And the surviving gaps are now **derived rather than declared**: the remaining deleting shapes sit in
their own group with `expect(out.length).toBeLessThan(input.length)` — deletion *asserted*, not inferred
— and a separate test filters that group by "does this parse?" and requires the result empty. So the
safety property ("everything that still deletes was already broken input") is a test rather than a
sentence, and a parseable shape landing there reds on the day it lands.

**Prefer the fix that makes the claim true over the fix that narrows the claim to fit the code** —
where it is affordable. Here it cost a small state machine and removed a class instead of documenting
one. The tell that (2) was available: the false claim was false for a *reason*, and the reason was a
question being asked at the wrong scope.

## 2026-08-28 — on resume, ask the remote what landed, not the agents

The Claude Code process running this shift exited with five agents mid-flight. Their transcripts and
worktrees survived and all five resumed cleanly, so nothing was lost. The part worth writing down is the
first thing done before resuming any of them: **read the actual PR head shas off the remote.**

Two of the five had pushed before the cut and three had not — and there was no way to tell which from
the agent list, the task notification, or my own notes, all of which recorded what each agent had been
*asked* to do. `gh pr list --json headRefOid` answered it in one call: two heads had moved to the new
round's sha, three were still sitting on the sha their reviewer had rejected.

Resuming a worker whose push already landed means a second push of the same change; resuming one that
never pushed and assuming it did means a PR that sits red forever while the Foreman waits for CI on work
that does not exist. **Both failures come from treating a dispatch record as evidence of an outcome.**

Same shape as the rest of this shift, applied to the crew instead of the code: **the report is not the
artifact.** A gate table is not the gate, a red-proof written in prose is not a red-proof performed, a
declared limitation is not a measured one — and an agent's brief is not a description of what is on the
branch. Every one of those is checkable in about one command, and the check is the cheap half.

## 2026-08-28 — the same over-generalisation, one level up, in the fix for it

Round 3 of #1087 shipped a gap list saying *"all of which fail toward KEEPING source"*. One of them
deleted. Round 4 fixed that properly, replaced the sentence, and wrote a new one: *"neither deleting case
is **reachable from valid JavaScript**"* — and this time made it **derived**, with a test that filters the
deleting-gap table by "does this parse?" and requires the result empty.

The narrow re-review found two parseable inputs that delete. `for await (…)` — because `await` sits in the
set of keywords a regex may follow, so it overwrites the `for` that the new paren rule depends on. And a
mis-read regex that swallows an unmatched `(`, desynchronising the very stack round 4 introduced.

**The derivation was real and the generalisation was not.** The filter proves *no entry in this table
parses*. That is a statement about the table. The prose next to it makes a statement about JavaScript.
Between those two sentences is every shape nobody wrote down — which is exactly what round 3's list did,
one level less abstract. **Round 4's own file header names this failure shape**, four hundred lines above
the sentence that commits it.

**The rule that would have caught both, and it is not "test harder":** when a test derives a property over
an enumerated set, **write the claim in terms of the set.** *"No case in `DELETING_GAPS` parses"* is
provable, useful, and cannot rot into a false statement about the world. *"Not reachable from valid
JavaScript"* is a claim about an infinite set, and a table cannot get there no matter how many rows it
has. **The bug is in the quantifier, not the coverage** — and a green test underneath makes the wrong
quantifier read as proven.

Neither shape was a regression (both reproduce identically on round 3), and the only production caller
goes through the parse-checked wrapper, so in-tree this was a loud throw rather than silent corruption.
**The blocker was the sentence.**

Worth recording the count plainly: **the narrow re-review — scoped to only what changed since the previous
round — has now found a blocking defect on this PR four rounds running, and each one was created by the
previous round's fix.** Three of the four were false or over-broad claims rather than broken code. A fix
that lands with a new claim attached needs the claim reviewed as carefully as the diff.

## 2026-08-28 — a comparison that ERRORS is not a comparison that is FALSE

#1091's publish-time guard refuses a catalog version that is not strictly greater than the published
one. Its Security Auditor found two ways to make it print *"strictly newer"* and exit **0**.

**One.** `if [ "$candidate" -le "$bound" ]` — bash's `test` builtin parses integers with `strtoimax`,
and **on overflow it writes `integer expected` to stderr and returns 2**. A non-zero `[` is falsy, the
refusal branch is skipped, and the function falls through to its success `printf`. The script runs under
`set -uo pipefail` with **no `-e`**, so nothing aborts and the step sees exit 0. `CatalogEntry.version`
is a `u64`, so **every value in `[2^63, 2^64-1]` is a legal published version this guard reads as
"yours is newer".**

**Two.** `jq '[.entries[]?.version] | max'` — jq's total ordering sorts numbers **below** strings, so a
single `"version": "1"` anywhere in the index makes `max` return `1` and the guard bound against it.

**The generalisable half is the first one.** An `if` has two branches and a failed comparison has a third
state, so shell folds *error* into *false* — and which branch "false" lands you in decides whether the
folding is safe. Here false meant proceed. **Any `[ ]`, `test`, `expr` or arithmetic comparison whose
false branch is the permissive one needs its operands validated before the comparison, not after** —
because the comparison itself cannot tell you it failed. The digit-only regex the script already had
looked like that validation and was not: it constrained the *characters*, never the *magnitude*.

**And a note the Auditor left that is worth more than the bug.** `[ x -le y ]` does **not**
arithmetic-evaluate its operands; `[[ x -le y ]]` **does**. So anyone "modernising" that line to the
double-bracket form turns an overflow fail-open into **command execution**. A latent hazard that is
invisible unless someone writes it at the site — which is now done.

Ranked honestly, and the ranking belongs in the PR: defeating this guard reverts to pre-PR behaviour,
and forging the API response still cannot forge a signed catalog. **It is not a new attack surface. It
is a guard that does not hold under the conditions its own comment claims it holds under** — and that
comment is what licensed shipping an unverified fetch on the release path in the first place.

## 2026-08-28 — the count in the diagnostic can lie

Same audit, smaller finding, and it keeps recurring in different clothes. The guard's permissive branch
prints *"N asset(s) enumerated"* — with `N` derived from a **joined string** rather than
`.assets | length`. An asset array of nameless objects prints "0 asset(s) enumerated" when there were
assets, on the one branch that proceeds.

That is this shift's *counts must come from work done, not intent* rule pointed at a diagnostic instead
of a gate. A number in a log is not load-bearing until the day someone reads it to decide whether a
silent branch was correct — and the permissive branch is exactly where that day arrives. **Derive the
count from the thing being counted, even when it is "only" a message.**

## 2026-08-28 — 22 of 33 tests skipped, and the report said 33

#1091 shipped a 33-case table driving a shell guard through every failure path. On a machine without
`jq` — which is most Windows boxes here — **22 of the 33 skip, and the skipped set contains every
executed failure path and both direction tests.** You get 11. The Reviewer had to download a real jq to
see the other 22.

**The `it.skipIf` is honest.** They report as *skipped*, not as passing. Nothing lies. But a human reading
their own terminal writes "33 tests" in the PR body, because the file has 33 cases and none of them
failed — and the number that reaches the reader is a number nobody measured. CI is ubuntu-latest, which
ships jq, so CI genuinely runs all 33; the gap only exists between what a developer sees and what they
report.

**The fix is not to remove the skip** — a developer without jq should still get the 11. It is to make the
run say which number it is: a loud summary naming how many skipped and why, and a **failure rather than a
skip when the tool is absent *and* `CI` is set**, so the one environment that must run everything can
never quietly run a third of it.

Same PR, same report, second instance: `npx vitest run` was quoted as *"5298 passed"* when 5298 is the
**total** — 5274 passed, 24 skipped. Also not fabricated, also loose, also a number nobody measured
reaching a reader who will act on it.

**The rule: a suite's headline number must state passed and skipped separately, and any conditional skip
must be loud about its condition.** This shift has spent two days on guards that could not run and
reported nothing; a test that does not run and reports *skipped* is the well-behaved version of exactly
that — and it still produces a wrong sentence in a PR body unless the harness makes the distinction hard
to drop.

Worth noting how it was caught: the Reviewer's machine also lacked jq, saw 11/22, and **went and got jq
rather than reporting the 11**. That instinct — when the number you measure disagrees with the number
claimed, find out why before writing either down — is the whole of the finding.

## 2026-08-28 — "reuse the existing classifier" was the wrong instruction, and measuring first caught it

CPE-1910's ticket and my dispatch brief both said the same thing, emphatically: the suite already has a
`log-signature` check that distinguishes an environment death from a real failure, so **reuse that signal
rather than inventing a second classifier.** Two layers keyed on one fact are one layer — that rule has
been right all shift.

The worker measured before building, and the measurement inverted the design. Of 30 failures across 312
shard jobs, **24 were a real, reproducible regression that classifies as `ENVIRONMENT SIGNATURE ONLY`** —
because `expect-webdriverio` waits throw a plain `Error`, never an `AssertionError`, so a genuine
regression carries the exact "no AssertionError anywhere in this run" line while reporting a healthy
14/14 specs. **A retry keyed on that verdict alone would have silently re-run real regressions**,
including the one another ticket had just root-caused.

The shipped trigger is the verdict **AND** the spec-file count — both facts the suite already recorded,
so still no second classifier, but no longer one fact either.

**The lesson is about the instruction, not the code.** "Reuse the existing signal" is good advice that
silently assumes the existing signal answers the question you are about to ask of it. This one answered
*"did anything throw an AssertionError"*, which had been a fine proxy for *"was this environmental"* right
up until a class of real failure stopped throwing one. **Before reusing a classifier for a new decision,
measure what it actually separates on today's data — not what its name says it separates.** A signal
inherits the population it was validated against, and nobody revalidates it when the population changes.

Two more things from the same measurement worth keeping:

- **The defect may already be fixed.** All three genuine socket deaths predate a ticket that merged five
  hours before the work started: 3 of 31 shard-2 jobs fatal before, **0 of 40 after**. The worker shipped
  the retry as an honest *backstop for the second death* and said so, rather than claiming a fix for
  something already at zero. Reviewing whether a backstop for a 0-of-40 defect earns its complexity is a
  fair question — the point is that it can now be asked, because the number is on the page.
- **The louder half was unasked for and is probably the bigger win.** The summary block counts the
  *existing* in-process respawns too: **6 of 40** sampled jobs had already used one, all recovered, and
  **nothing anywhere said so.** A silent recovery mechanism hiding its own rate is the shape that hid a
  month-long outage in this repo once already.

## 2026-08-28 — I quoted the section above the failure as if it were the failure

#1087's contrast job went red on CI with *"PIXEL CROSS-CHECK FAILED — 3 ground(s) painted a colour the
computed-style path did not predict."* I read the four rows printed immediately above that line, saw
deltas of `+0.05` to `+0.24`, and told the worker this looked like antialiasing or device scale — small,
safe-direction, probably subpixel.

**Those four rows were the THIN MARGINS section.** They are a different report that happens to print just
before the verdict. The actual pixel disagreements were **12 and 37** — an order of magnitude past
anything subpixel, and the worker said so rather than building on my framing.

**The mechanics of the mistake are worth more than the mistake.** A log is not a structure; adjacency in
a terminal is not containment. I had the failing line and I took the nearest numbers as belonging to it,
because they were numbers, they were close, and they were consistent with a story I had already formed.
The fix costs nothing: **when reading a failure out of a log, find where the failing section begins, not
where the failing line ends.** Everything above it belongs to something else until proven otherwise.

Second-order and more useful: **a plausible cause offered by a Foreman is a prior a worker has to
overcome.** I handed over "probably antialiasing" with a caveat to measure — and the caveat is the only
reason this went well. Had I stated it flatly, the likely outcome is a derived-looking tolerance built to
absorb a 0.24 that was never the problem, and a real 37 shipping green underneath it.

## 2026-08-28 — the premise in the comment was the defect, again

The actual cause of that CI failure: the sampler read an element's ground colour as the **mode of 45
interior samples**, resting on a premise written in the code — *"glyphs are a minority of an element's
interior pixels."*

Measured, it is false. A 25×14 tab-usage span sampled **28 distinct colours across 45 points**, the mode
winning with **13 of 45**. Six grounds had the predicted colour appear **zero times** and passed only
because whichever antialiased blend happened to win landed within 1/255 of the prediction. Which blend
wins is decided by font rasterisation — so Windows gave 59/0 and ubuntu-latest gave 60/3 **on the same
commit**, and neither number was measuring what it claimed.

**The sentence was the defect.** That is the fourth time this shift: a comment asserting a property
nobody checked, with a green run standing next to it. The others were a gap list, a red-proof naming an
impossible mutation, and a quantifier over an infinite set. This one is the purest of them — a single
declarative clause about pixel statistics, load-bearing for every ratio the harness reports.

**And the fix was in the model, not the threshold.** Glyph *fill* is now suppressed for the screenshot
via `-webkit-text-fill-color` — chosen deliberately narrower than `color` so `currentColor`, borders,
outlines and shadows are untouched — and every ground comes back **45/45, one distinct colour, 118/118
unanimous**. The guard then got *tighter*: flatness is now a fatal condition, which is the strongest form
available and spends no margin at all. A tuned epsilon would have made the same red go away and left the
sampler still measuring the wrong thing.

## 2026-08-28 — the evidence was in the string, and the parser threw it away

#1089's staging files are named `<name>.<pid>-<nanos>.cpe-tmp`. A cleanup sweep decides what it may
delete using a stamp validator and a 300-second age floor, and the round widened that sweep from "this
destination's own siblings" to "any `.cpe-tmp` in the directory".

The defence written at the site was that the validator and the age floor bound the widening. The
reviewer's objection is exact: **they establish "shaped like ours **and** stale". They do not establish
"ours" — and the ownership evidence is right there in the string being parsed.** `is_valid_temp_stamp`
reads the `<pid>` half solely to prove it is digits, then discards it.

**The worst case it constructed is ours, not an attacker's.** A bulk write puts N files in one folder;
one stalls on a hung socket and its staging file's mtime freezes. The sweep memo is a **one-slot**
`Option<PathBuf>`, so a depth-first walk that alternates `A/f1`, `A/sub/g1`, `A/f2` re-enters `A` and
re-sweeps every time. Past 300 seconds the next commit unlinks the **live** sibling — and because
staging handles are opened with `FILE_SHARE_DELETE`, this succeeds on **Windows too**. The writer keeps
filling a nameless object and the commit fails. Under the old narrow scope it was impossible: you needed
two saves of the *same* destination.

**The fix is three lines and loses nothing**: skip a candidate whose pid is this process's, unless it is
the target's own. Residue from a killed process — the only thing the widening exists to collect — never
carries our pid.

**The transferable part: when a guard's own naming scheme already encodes the fact it needs, check
whether the validator is reading it or merely stepping over it.** Here the answer was in the filename
that the code itself had written, parsed, and dropped on the floor. And the doc's word choice hid it —
*"bounded by the stamp validator and the age floor"* is true and sounds like ownership, which is why
nobody looked at what the stamp actually contained.

**And the blocking finding in the same round was the round's own theme.** #1089 round 2 existed to fix
comments that outran their evidence. It widened this deletion and left the **call-site** comment saying
*"scoped to this destination's own siblings, on the same terms `stage_and_replace_at` calls it"* —
both clauses now false, while the function's own doc, the next paragraph, and the new enum's doc all say
the opposite. The call site is what a reader hits first, and it is the one that states how much gets
deleted. **A widened scope has to be corrected everywhere it is described, and the description nearest
the call is the one that matters most.**

## 2026-08-28 — the round that fixed an over-broad claim wrote a new one inside the fix

#1087's round 4 shipped a false safety sentence and round 5 corrected it — properly, with the claim
rewritten in terms of the set the test enumerates. Round 5's own new fix then carried this comment:

> *"This restores balance; it does NOT recover the KINDS inside the swallowed text … **which only matters
> on input that already failed to parse**."*

The reviewer refuted it with a 38-character input that parses. **Two rounds running, the fix for an
over-broad claim arrived with a new over-broad claim attached** — and both times the new one was narrower,
more specific, more obviously reasoned, and still wrong.

**Why this keeps happening is worth naming.** When you fix a mechanism you understand it better than
anyone has yet, and that understanding is exactly what produces a confident sentence about what the
mechanism cannot do. The sentence is generated by the same reasoning that produced the fix, so it inherits
the fix's blind spots and none of its testing. **A limit written by the person who just closed the
mechanism is the least-tested sentence in the diff and reads as the most authoritative.**

The practical rule: **a fix's own stated limit is a claim in the same diff, and needs an input before it
needs a reviewer.** Not "review the comment" — *construct the case the comment says cannot exist,* and
paste the result beside it. The reviewer did that on its first attempt, twice.

## 2026-08-28 — a negative over a generated space is only as good as the generator

The same round claimed *"no third family"* on the strength of **1,904 structured inputs plus 36,861 fuzzer
outputs, 0 desyncs."* The reviewer found a third family with a **27-character** input on its first try.

The generator was not lazy — 66 regex contexts × 14 literals is real coverage of the shapes its author had
in mind. But the missing family fails for a reason outside the model the generator was built around: the
mis-read regex terminates on the **opening `/` of the following regex literal**, so the comment branch is
reached before any paren state is consulted. **No amount of sampling from a space defined by "paren
shapes" reaches a bug whose mechanism is not about parens.** The sample size is what makes it persuasive
and it is the least relevant number in the claim.

Two things follow. **Report what a sweep establishes, not what it suggests** — "0 desyncs across N inputs
of these shapes" is true, useful, and cannot rot; "no third family" is a claim about JavaScript that no
sweep can reach. Same correction this PR already made for its case table, one level out. And **commit the
sweep** — this one lived only in the author's scratch, so the next round could neither re-run it nor
extend it, and the reviewer had to rebuild an equivalent from scratch to check it. An uncommitted
measurement is a number without an artefact, which is the shape this shift keeps finding.

Worth noting the honest arithmetic the reviewer put beside its blocker: the fix it is objecting to
**repairs 45 shapes and breaks 4.** It asked for the four, explicitly did not ask for a revert, and said
so in the same breath as the finding. **A net-positive fix with a real regression in it is still a
regression, and saying both numbers is what makes the objection actionable rather than discouraging.**

## 2026-08-28 — the recovery path broke the thing it recovers, and the fixture could not have caught it

#1092 adds a retry for a gui-smoke shard whose WebDriver session dies before asserting. On retry it
archives the previous attempt's artefacts into a subdirectory — `.results/*.json` moved into
`attempt-1/`.

`.results/` holds two kinds of `.json`: the per-spec reporter chunks the loop is written for, and the
**shard manifest**, written by an earlier workflow step. The loop moves both. The artifact upload is
`path: .results/*.json` — flat, non-recursive — so **a retried shard uploads its results but not its
manifest**, and the downstream verdict job, which requires a manifest from every shard, reports:

> *MISSING SHARD: shard 2 of 4 never reported a manifest. Its spec files did not run.*

**On the exact scenario the feature exists to handle, the shard goes green and the leg reds anyway, with
a message that is false** — the shard ran twice and passed. Strictly worse than the diagnosable failure
it replaces.

**Two things make this worth writing down.**

**The hazard was already known one file over.** A sibling module filters manifests out of the same
directory by prefix, and its test has a case for exactly that co-location. The new code is the third
reader of `.results/` and the first to forget that the directory holds two populations. *A directory
with mixed contents needs the discriminator at every reader, and the existing filter is the notice that
a second population exists — grep for it before writing the third reader.*

**And the fixture could not have caught it.** The integration test drives the real scripts through a
real retry — genuinely good — but its `.results/` only ever contains reporter chunks. **The test
exercised the code path perfectly and the data was unrepresentative**, which is the failure mode a
passing integration test is least likely to advertise. The fix is not another assertion; it is seeding
the fixture with what production actually puts there. **When a test builds its own input, the question
is not "does this path run" but "does this input look like production's."**

Closing note on scope, from the reviewer, worth keeping as a template: *"the two halves of this PR are
not equally justified."* The silent-recovery reporting is an unambiguous win — **8 of 45 jobs had
recovered in-process with nothing anywhere saying so.** The retry itself is a backstop for an event now
measured at **0 of 45**, adding ~500 lines to the critical path of every run — **and the blocker above
is the evidence for that cost**, a break on the rarely-exercised path that survived both authoring and
review. Still worth landing, but the reviewer said which half earned it and which half is insurance.
That distinction belongs in the PR body, not just in the review.

## 2026-08-28 — the worker declined both gates' suggested fix, and was right

#1091's Security Auditor found a bash comparison that fails open on integer overflow, and handed over a
fix: cap the operand at 19 digits before comparing. The Reviewer, independently, agreed. I forwarded it
as the fix to take.

The worker took the finding and declined the fix, with a reason: **a 19-digit cap still leaves a hole
for values in `[2^63, 9999999999999999999]`** — all 19 digits, all above `i64::MAX`, all still
overflowing — **and it narrows away legal `u64` versions at the top of the range.** It wrote an exact
length-then-digit comparator that converts nothing instead, so the whole legal range compares correctly,
and gave "the largest will not fit" its own exit code rather than folding it into "no numeric version
anywhere", on the grounds that they are different facts.

**Two gates converging on a fix is evidence about the bug, not about the fix.** They agreed because they
found the same defect, and the cap is the first thing that occurs to anyone looking at an overflow —
it makes the reproduction stop reproducing. That is not the same as closing the class, and the gap it
leaves is in the digit range the original bug actually lives in.

**The rule: a suggested fix in a review is a hypothesis, not a spec.** The finding is the deliverable;
the suggestion is a courtesy. A worker who implements it unexamined converts a reviewer's five minutes
of thought into shipped code with none of the measurement the finding itself got — and here that would
have left the guard failing open on exactly the values that produced the report.

Worth recording the measurement that came with it, because it sharpens the `[[ ]]` warning written last
tick. On bash 5.3: **`[[ 9223372036854775808 -le 5 ]]` returns 0 — it *wraps* rather than erroring**,
which is strictly worse than `[`'s loud-but-falsy failure, because nothing is printed at all. And
`v='a[$(touch PWNED)]'; [[ $v -le 1 ]]` returns 0 **and creates the file**, while `[ "$v" -le 1 ]` errors
and creates nothing. Both halves of the hazard are now measured rather than asserted, at the site.

## 2026-08-28 — fix the fixture before you add the assertion

#1092's blocker was a retry that swept the shard manifest into an archive directory, so the verdict job
announced that a shard which had run twice and passed had never run at all. The one-line fix was obvious.
What the round did *first* is the part worth keeping.

The integration test already drove the real scripts through a real retry — genuinely good coverage — and
it could never have caught this, because its `.results/` only ever held reporter chunks. **The round
rebuilt the fixture before writing the new assertion**: it now runs sharded 1-of-1 and seeds the
directory by **executing the real manifest writer, in CI's order**, then asserts the manifest landed.
Every pre-existing count in the file stayed the same, which is the evidence the change was to the
*input*, not to the expectations.

**The ordering matters more than it sounds.** Add the assertion first and you get a test that passes
because you also wrote the setup line it needs — a closed loop where the fixture and the assertion were
authored together to agree. Fix the fixture first and every *existing* case in the file becomes a fresh
test against more realistic data; only then does a new assertion mean anything. Here that ordering also
turned up the need to split the ratchet helper into the shard job's mode and a real verdict-join mode,
which no assertion would have prompted.

**Second thing from the same round, and it is CLAUDE.md's own rule being taken literally.** The port
handshake needed the two driver port numbers that `wdio.conf.ts` owns. The default move is to copy them
with a *"same as wdio.conf.ts"* comment — which this repo has spent the shift learning is an untested
provenance claim. The round did not derive it either. **It moved the constants to a shared module and
imported them from both sides.** *Where the duplication is removable, remove it* — deriving a claim is
the fallback for when you cannot.

And one small thing worth stealing: `waitForPortFree` **returns a boolean** so that *"did not settle"*
cannot be read as *"settled"*. A timeout that returns nothing is the same fail-open family this shift has
found ten times; a timeout that returns `false` makes the caller decide in the open.

## 2026-08-28 — the universal in the test name, the list in the test body

#1091 round 2 closed a log-injection hole by sanitising every place a remote-controlled string reaches
the Actions log. Its header enumerates three: the API body on two exits, curl's and jq's stderr, and the
release tag on the permissive exit-0 path. The test block that covers them is titled **"nothing fetched
can become a workflow command in the job log"** — and it is backed by exactly those three.

There is a fourth. The exit-10 path prints `$tag` raw. Same variable, same source, same forged API
response, sanitised eleven lines earlier on a different exit. **And it is the worst of the four to
miss**: exit 10 is a *failure*, and `::stop-commands::` injected there silences annotations for the rest
of a job whose entire purpose is being loud when it does not publish.

**The title is a universal and the body is a list, and the gap between them is where the fourth site
lived.** That is the same defect this shift has now found in a gap list, a red-proof, a quantifier over
JavaScript, a sweep over generated inputs, and a fixture — five different costumes, one shape: *a claim
about all of something, evidenced over the instances somebody wrote down, with a green run underneath
making it read as proved.*

**What makes this instance the most useful of the five is that the fix is structural and cheap.** The
test should range over the exit codes the script can produce, not over three chosen paths. The script
already knows its own exits — they are a closed set, each with a distinct code the suite already asserts
on. **When a test claims "nothing X", find the enumerable thing the claim ranges over and iterate it.**
If nothing enumerable exists, the claim has to shrink to the list. Here one did exist, and nobody looked
for it.

Worth noting how the reviewer found it: not by reading the enumeration, but by **stubbing only `gh` and
letting the real curl hit the real URL** — which 404s today because of the outage this very ticket is
about. The attacker capability required turned out to be *less* than the mitigations already assume. A
test environment that is realistic in one more dimension than the author's found a site the author's
list had no reason to contain.

## 2026-08-28 — 400,000 draws that were 120 programs

#1087's round 5 answered "is there a third failure family?" with a fuzz sweep: **36,861 parseable
generated inputs, 0 desyncs.** Its reviewer refuted the conclusion with a 27-character input, and I
recorded the lesson as *a negative over a generated space is only as good as the generator*.

Round 6 found the sharper reason. **The generator's LCG lost precision in JS doubles, so 400,000 draws
deduped to roughly 120 distinct programs.** The sweep was not merely aimed at the wrong space; it was
sampling the same hundred-odd strings tens of thousands of times and counting each one.

**The size was the entire persuasive content of the claim, and the size was an artefact of the
arithmetic.** Nobody could see it from the outside: 36,861 is exactly what a healthy fuzzer prints. The
number was not wrong about how many inputs ran — it was wrong about how many *questions were asked*, and
those are the same word in a report and different facts.

**One line would have caught it: report distinct inputs, not draws.** A generator that cannot say how
many unique programs it produced is not reporting coverage, it is reporting effort. The rewritten sweep
does — **29,285 distinct** — and is now **committed with a `--compare <ref>` mode**, which is the other
half: round 5's lived in scratch, so neither the next round nor the reviewer could re-run it, and the
reviewer had to rebuild an equivalent from scratch to disprove it.

## 2026-08-28 — the new family turned the existing table's property red, which was the correct answer

Same round, and this is the good news half. Told to add the newly-found deleting family to the
`DELETING_GAPS` table, the worker tried it and **the table's derived safety property went red — correctly.**
That table's whole invariant is *no entry in it parses before stripping*; the new family deletes **valid**
JavaScript, so it cannot live there without falsifying the one thing the table proves.

The wrong move is obvious and was available: relax the property, or quietly widen the table's meaning.
Instead the family got its **own** table with its own three assertions (the input parses, the output does
not, the checked wrapper throws) — **and the oracle now asserts which tables it sweeps**, so a family
held back from both is visible rather than merely absent.

**That last clause is the real fix.** Every version of this defect on this PR — five rounds of it — was
something true about a written-down list being read as true about the world. An oracle that enumerates
*which lists it covers* is the first structure in six rounds that makes an omission show up as an
omission rather than as silence.

Also worth keeping from the same round, because it is the opposite of what a worker under pressure
usually does: the author **volunteered that its own new majority check would not have caught the earlier
glyph bug** — agreement with the suppressor off is 51%, one point above the bar — and used that to argue
for keeping the older, stricter flatness check fatal rather than letting the new one replace it. A
weaker check that arrives with an honest account of what it misses is worth more than a stronger one
that arrives without.

## 2026-08-28 — the reviewer ran the converse, and it was the more informative direction

#1092's round 2 pinned a filter that must skip one file type and keep another. Its red-proof reverted the
filter and watched two tests red — correct, and what was asked for.

Its reviewer re-ran that, then ran the direction nobody had: **forced the filter over-broad so it archived
nothing**, and found that case caught too — by a **pre-existing** assertion, not by either new test. So the
filter is pinned in both directions, and the new tests only cover one of them.

**That is worth generalising, because "skips too much" is the failure a filter's own test almost never
looks for.** You write a filter to exclude something; you prove it excludes it. Nothing in that loop asks
whether it now excludes more than intended, and a filter that drops everything satisfies the assertion
you wrote. Here the safety net already existed by luck of an older test. **When a change adds an
exclusion, red-proof both edges: revert it, and widen it.** The second one takes the same thirty seconds
and is the half that finds an over-eager predicate.

The same review is a good example of enumerating instead of trusting. Asked whether the fixture was
representative, it did not check the two file types the author named — it **enumerated every writer into
that directory** and established there is no third, ruling out the obvious candidate by reading config
(`reporters: ["spec"]` with no `outputDir`, so wdio writes no JSON of its own). It also confirmed CI's
step order **from the workflow rather than from the comment claiming it**, and checked the two filename
prefixes cannot collide. Three separate places where the cheap move was to accept a stated fact.

And it verified the one new number written into a code comment — a CI job's suite-step duration — **against
the API**: 119 seconds exactly, whole job 181. After a shift in which numbers in comments have been wrong
four times, checking the one that was newly added is exactly the right instinct.

## 2026-08-28 — three doc sites and the one an operator actually reads

#1092's reviewer found a supporting sentence that was true for one port and false for the other, and
named **three** places carrying it: a function doc, a sibling module's doc, and the README. The worker
fixed those and found a **fourth** — the same false claim inside `settleDriverPorts`'s own **WARNING log
line**.

**That is the copy a human reads at 3 a.m. when the thing has actually gone wrong**, and it was the one
site nobody enumerated. Three docs corrected and the runtime message left lying would have been close to
the worst outcome available: the paper is right, and the only sentence anyone reads under pressure still
misleads.

**The generalisation: when a claim is wrong, sweep the *emitted* strings, not only the comments.** A
grep for the doc's wording finds docs. The message a user or operator sees is usually phrased
differently — it is shorter, it names the runtime values, it lives in a `printf` — so it survives the
sweep that fixes everything around it. Log lines, error messages, toast text and CLI help are all
documentation with a different audience and no reviewer.

Two details worth keeping from how it was done. It **verified the reviewer's line numbers before acting
on them** (all three checked out) and then removed the line citation entirely in favour of the symbol,
with the reason written at the site: *a line number in prose is a provenance claim nothing checks, and
this one rotted inside a single round while the code comments citing the symbol did not.* And when it
rewrote the WARNING it **read both port numbers from the shared constant** rather than typing them, so
the "zero live port literals outside `driverPorts.ts`" property the reviewer had just verified stayed
true. Fixing a claim without breaking a neighbouring invariant is the kind of care that does not show up
in a diff summary.
