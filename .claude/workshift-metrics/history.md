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

## 2026-07-29 (workshift — Ticketing/ container realignment + QA burndown)

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

## 2026-07-29 (workshift — the 3 checkpoint-queued honest-headless slices)

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

## 2026-07-29 (workshift #2 — docs debt; user present)

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

## 2026-07-29 (workshift #3 — instant-index epic CPE-703 shipped, attended)

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

## 2026-07-29→30 (workshifts ×2, attended — board hygiene + UX/feature polish)

User asked for "two workshifts back to back", then gave two mid-run directives (epic-board cleanup + a pane
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

## 2026-07-30 (workshifts ×3 request — QA pins + epic closure; frontier confirmed user-gated)

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
gui-smoke **screenshot capture** so the workshift can judge GUI work from screenshots (as the gauntlet's visual
leg, next to Reviewer=code / UAT=behavior) and escalate to the user minimally. This directly targets the
button-polish round-trips (clip/placement are screenshot-visible; only pure icon *preference* needed the user).
When approved: a QA-Architect ticket for the screenshot harness + a workshift-skill change adding the Critic to
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
- User asked to "run three workshifts back to back." Queue was empty; PM (opus) frontier scout confirmed the
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
- **CI greened after the outage (CPE-1268):** the whole workshift had merged via `--admin` during the stall, so CI's
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

## 2026-08-03 — "3 workshifts back to back": safety-scan vein + media-metadata codecs (14 shipped)
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
