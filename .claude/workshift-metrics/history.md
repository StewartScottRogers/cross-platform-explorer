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
