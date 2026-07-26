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
