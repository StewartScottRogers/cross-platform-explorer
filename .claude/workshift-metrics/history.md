# Workshift learnings — rolling log

Distilled, human-readable defaults learned from past shifts. Appended at each end-of-shift; read at
kickoff to seed model/parallelism choices. Newest last. Raw per-agent detail lives in `ledger.jsonl`
(gitignored). Format + rationale: `README.md`.

<!-- Each entry: -->
<!-- ## YYYY-MM-DD — <shift one-liner> -->
<!-- - Shipped: CPE-NNN, CPE-NNN … -->
<!-- - Tuned defaults: <class>: <model>, <N>-wide, ~<median> median, <stuck/retry note> -->
<!-- - Notes: <anything that changed a default> -->

_(no shifts recorded yet — first real workshift will seed this)_

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

## 2026-07-27 (workshift — short, furlough wind-down)

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
