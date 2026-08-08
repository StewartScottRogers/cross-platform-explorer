---
id: CPE-1445
title: "SVGZ (gzip) .svg bypasses the raw-byte nesting guard → uncatchable stack overflow + uncapped decompression OOM"
type: Bug
status: Done
priority: High
component: Backend
tags: [ready, security]
epic: CPE-718
created: 2026-08-07
---
## Vector (found in the CPE-1437 resource-exhaustion sweep, 2026-08-07)
A file named `*.svg` whose bytes begin with the gzip magic `1F 8B`. Prod path: browsing a folder →
thumbnail grid → `thumb_source.rs:~100` `rasterize_svg(&bytes, edge)` → `thumb_svg.rs:~173`
`xml_nesting_too_deep(bytes, …)` scans the **compressed** bytes, sees no `<` tags, returns `false`
(guard bypassed) → `thumb_svg.rs:~178` `usvg::Tree::from_data` detects the gzip magic
(`usvg parser/mod.rs:~98`) and calls `decompress_svgz` (`~:129`, a `read_to_end` with **no cap**), then
hands deeply-nested XML to `roxmltree` which recurses per nesting level.

## Concrete pathological inputs
- **Stack overflow:** a ~2–50 KB gzip of `<svg…>` + `"<g>".repeat(100_000)` + a rect + closes.
  Decompresses to a few MB of XML nested 100k deep → roxmltree blows any thread stack (the 2 MiB
  `spawn_blocking` stack included — gzip removes the file-size ceiling on depth that made raw SVG
  survivable). Uncatchable → whole process crashes.
- **OOM:** `"A".repeat(4GB)` gzipped to ~4 MB → `decompress_svgz` allocates ~4 GB.
Both stay under the 128 MiB file gate because the source file is tiny.

## Fix direction
In `rasterize_svg`, detect the gzip magic and decompress **with a bounded `.take(cap)`** BEFORE running
`xml_nesting_too_deep`, running the existing depth cap on the **decompressed** bytes; reject anything over
the cap (graceful Err). Closes both the nesting-guard bypass and the decompression bomb in one place.
This is distinct from CPE-1437 (clip/mask chains) and cheaper than the durable isolation in CPE-1444, but
**must serialize behind CPE-1437** — both edit `thumb_svg.rs`. Add gzipped-deep-nested + gzip-bomb fixtures
to the `thumb_svg` tests and the small-stack panic-safety probe.

## Effort / blast radius
S / tiny — one function, additive guard.

---

## Work Log — 2026-08-07 (Done)

Implemented on branch `cpe-1445-svgz-gzip-guard`, on top of current `main` (which already includes
CPE-1437's stack-hardening + CPE-1444's combined hops×nesting cap, merged as #712).

### Verification findings — per guard, raw vs. decompressed, capped vs. uncapped

Read all of `crates/server/src/thumb_svg.rs` (1380 lines pre-fix) end to end before touching anything, per
the ticket's "verify first" instruction, since CPE-1437/1444's own SVGZ handling turned out to be partial:

1. **`xml_nesting_too_deep`** (the ONLY guard that ran on the caller's own thread, ahead of everything
   else — deliberately, since it's the sole provably-non-recursive byte scan): called directly on the raw
   `bytes: &[u8]` passed into `rasterize_svg`, **with no gzip awareness at all** — no magic-byte check, no
   decompress branch. **Bug (A) NESTING-GUARD BYPASS was still fully open on main**: a gzipped deeply-nested
   SVG sails straight through this guard (sees no `<` tags in the compressed bytes) exactly as the ticket
   describes.
2. **`reference_chain_too_deep`** (added by CPE-1437, runs on the guaranteed-large-stack thread): DOES
   check `bytes.starts_with(&[0x1f, 0x8b])` and decompress SVGZ input — but via `resvg::usvg::decompress_svgz`,
   which (confirmed by reading `usvg-0.45.1/src/parser/mod.rs:129-138` from the local cargo registry
   checkout) is a bare `GzDecoder::new(data)` + `read_to_end(&mut decoded)` with **no size cap of any kind**.
   **Bug (B) DECOMPRESSION OOM was open for this call.**
3. **`usvg::Tree::from_data`** (the actual usvg entry point, called on the still-RAW `bytes` at the end of
   `rasterize_svg_on_a_guaranteed_stack`): also auto-detects the `1F 8B` magic and calls the SAME uncapped
   `decompress_svgz` internally (`usvg-0.45.1/src/parser/mod.rs:96-99`) — a **second, independent uncapped
   decompression of the same file** on top of #2 above, since the pre-fix code always passed the original
   (still-compressed) bytes through unchanged. **Bug (B) was open here too, doubly.**

Conclusion: this was NOT the "already fully closed" case — CPE-1437/1444 partially engaged with SVGZ (item
2) but left the nesting-guard bypass (A) completely open and the decompression bound (B) uncapped in BOTH
places it occurred. Both sub-bugs from the ticket needed real fixes.

### Fix

Moved gzip handling to the very front of `rasterize_svg`, once, for the whole function, per the ticket's
"FIX" direction:

- New `MAX_DECOMPRESSED_SVG_BYTES: u64 = 32 * 1024 * 1024` (32 MiB) — sized the same way
  `doc_text::MAX_DECOMPRESSED_PART_BYTES` (8 MiB, CPE-1446) is: comfortably above any legitimate
  hand-authored/tool-exported SVG's XML text (real SVGs are almost always well under a few hundred KB) and
  kept under `thumb_source::MAX_SOURCE_FILE_BYTES` (128 MiB, the existing raw-file-size gate), so
  decompressing a legitimate SVGZ can never use more memory than reading an equivalently-sized plain `.svg`
  already would.
- New `decompress_svgz_bounded(bytes) -> Result<Vec<u8>, String>`: `flate2::read::GzDecoder` wrapped in
  `Read::take(MAX_DECOMPRESSED_SVG_BYTES + 1)` (the `+1` makes "exactly at the cap" vs. "one byte past it"
  distinguishable), `read_to_end`'d, then rejected if the result exceeds the cap. `flate2` only inflates as
  much of the compressed stream as the reader actually pulls, so this can never materialize more than
  `cap + 1` bytes regardless of how large the stream's true/claimed logical payload is — closes (B)
  provably, not just empirically. No new dependency: `flate2 = "1"` is already a direct `cpe-server`
  dependency (used throughout `archive.rs`/`thumb_font.rs`, and this exact `Read::take` shape mirrors
  `thumb_font.rs:120`'s existing `ZlibDecoder::new(raw).take(MAX_WOFF_TABLE_BYTES)` pattern).
- `rasterize_svg` now: detects the `1F 8B` magic, decompresses once via `decompress_svgz_bounded` (`?`
  propagates a graceful `Err`), and threads the **decompressed** bytes through everything downstream —
  `xml_nesting_too_deep` on the caller's thread, then `rasterize_svg_on_a_guaranteed_stack` (which now
  never sees the gzip magic, so `reference_chain_too_deep`'s own gzip branch is a no-op and
  `usvg::Tree::from_data`'s internal `decompress_svgz` never fires). Closes (A): the nesting guard now
  genuinely scans the real XML. Closes (B) fully: exactly ONE bounded decompression happens, not two
  unbounded ones.
- `reference_chain_too_deep`'s own gzip branch (normally unreachable now, since its input is
  pre-decompressed by the caller) was also switched from the uncapped `resvg::usvg::decompress_svgz` to
  `decompress_svgz_bounded`, as defense-in-depth for any future direct caller of that function.

### Tests added

`crates/server/src/thumb_svg.rs` unit tests (fast, in-crate):
- `decompress_svgz_bounded_round_trips_a_small_payload`
- `decompress_svgz_bounded_rejects_malformed_gzip_gracefully`
- `decompress_svgz_bounded_allows_exactly_the_cap_and_rejects_one_byte_over` (boundary: exactly
  `MAX_DECOMPRESSED_SVG_BYTES` allowed, `+1` byte rejected)
- `decompress_svgz_bounded_stops_a_gzip_bomb_without_allocating_the_full_size` (100 MiB logical zeros,
  streamed into the encoder in 1 MiB chunks so the fixture-building itself never holds the full size in
  memory; compresses >1000x; rejected)
- `xml_nesting_guard_bypass_is_closed_by_pre_decompressing_gzip_before_the_scan` (proves the RAW-byte call
  to `xml_nesting_too_deep` does NOT see the nesting — confirming the bypass was real — while the
  decompressed bytes DO get flagged)
- `rasterize_svg_rejects_a_gzipped_deeply_nested_svg`, `rasterize_svg_renders_a_legit_gzipped_svg`

`crates/server/tests/thumb_svg_panic_safety.rs` (the battery, per the ticket's explicit ask):
- `rasterize_svg_never_stack_overflows_on_a_gzipped_deeply_nested_svg_on_a_small_stack` — gzip of the same
  4000-deep `<g>` nesting fixture the plain-SVG regression test uses, run through the 256KB
  `run_on_small_stack` probe → **Err**, not overflow.
- `rasterize_svg_rejects_a_gzip_bomb_without_unbounded_decompression` — a gzip stream that decompresses to
  200 MiB of zeros (>6x the 32 MiB cap) while the compressed fixture itself is built streamed (1 MiB
  chunks) and stays under `logical_size / 1000` bytes → **Err**, promptly, without attempting to
  materialize 200 MiB.
- `rasterize_svg_renders_a_legit_small_gzipped_svg_fine` — a real small SVGZ (gzip of the shared
  `minimal_svg()` fixture) still rasterizes **Ok**.

### Verification (all synchronous, in `crates/server`)

- `cargo build` — clean.
- `cargo build --tests` — clean.
- `cargo clippy --all-targets -- -D warnings` (default features) — clean.
- `cargo clippy --all-targets --features index -- -D warnings` — clean.
- `cargo test --test thumb_svg_panic_safety` — **27/27 pass** (24 pre-existing + 3 new gzip cases).
- `cargo test` (whole crate, debug) — **1724 passed, 0 failed** (one `organize_apply` test flaked once on
  the first run under full-suite parallelism — a pre-existing, unrelated filesystem-collision test, not
  touched by this change — and passed cleanly both in isolation and on a full-suite rerun).

No new dependencies. Graceful `Err` throughout (`decompress_svgz_bounded` never panics on malformed or
oversized input); no change to the existing CPE-1437/1444 six-reference-type combined-cost guard or the
16 MiB guaranteed-stack render, both left exactly as-is.

## Work Log — 2026-08-07 (attempt 2, re-audit fix)

An independent adversarial auditor found a real bypass in PR #713 (attempt 1 above): **DOUBLE-gzip**.
Attempt 1's fix is correct for exactly one gzip layer, but doesn't establish the invariant it implicitly
relied on ("the bytes handed onward past the decompress are never gzip-magic'd") for two or more layers.

### The bypass (confirmed by the auditor with reproducers)

`rasterize_svg` decompressed exactly ONE gzip layer via `decompress_svgz_bounded`. For a **doubly-gzipped**
`.svg` (gzip of a gzip of an SVG document), that single bounded decompress yields bytes that **still start
with `1F 8B`** — the untouched inner gzip stream. Before this fix, those bytes flowed straight through
`xml_nesting_too_deep` (harmless there — same false-negative-on-binary behavior as before, but no longer
the load-bearing guard) into `usvg::Tree::from_data`, which auto-detects the magic and decompresses the
inner layer **itself, unbounded** — exactly the "second decompression" attempt 1's own doc comment claimed
"never fires" (true only for single-gzip). Auditor-confirmed:
- `rasterize_svg(gzip(gzip(deeply_nested_svg(4000))))` on the small-stack probe → `STATUS_STACK_OVERFLOW`
  (0xC00000FD), uncatchable — and since this happens on the 16MiB `rasterize_svg_on_a_guaranteed_stack`
  thread (not just the small probe), it would have crashed **production** too: the single-gzip 4000-deep
  case is caught by `xml_nesting_too_deep` running on the (correctly, singly) decompressed bytes, but the
  double-gzip variant bypasses that guard entirely and the deep nesting reaches `usvg`/`roxmltree` raw.
- `rasterize_svg(gzip(gzip_bomb(200MiB)))` → peak ~270MiB: `usvg` inflated the full inner 200MiB payload
  inside `Tree::from_data`. The 32MiB `MAX_DECOMPRESSED_SVG_BYTES` cap only ever bounded the OUTER layer's
  output size — a sub-1MB file on disk still produced a multi-hundred-MB (and, scaled up, multi-GB) live
  allocation.

Single-gzip tests from attempt 1 all still passed — the fix was right for one layer, just incomplete for
N ≥ 2.

### The fix (attempt 2) — outright reject, not a decompress loop

Per the auditor's recommended (simpler, safer) option: after `rasterize_svg`'s one bounded decompress, if
the result **still starts with `1F 8B`**, reject immediately with a graceful `Err` — a doubly-gzipped `.svg`
is not a legitimate SVGZ file (every real SVG authoring/export tool, and the SVGZ format itself, produces
exactly one gzip layer), so nested gzip has no legitimate use case to preserve. Chose reject-outright over
looping the bounded decompress across layers (with a cumulative cap and a max-layer-count limit): looping
only relocates "how many layers is too many" to a new, equally arbitrary cap, while reject is simpler,
provably correct, and costs nothing for real-world input.

Two enforcement points, both in `crates/server/src/thumb_svg.rs`:
1. **`rasterize_svg`** (right after `decompress_svgz_bounded`): `if decompressed.starts_with(&[0x1f, 0x8b]) { return Err("nested (double-gzipped) SVG rejected".to_string()); }`.
2. **Defense-in-depth** at the actual `usvg::Tree::from_data` call site inside
   `rasterize_svg_on_a_guaranteed_stack`'s render closure: `if bytes.starts_with(&[0x1f, 0x8b]) { return Err(...) }`
   immediately before the `Tree::from_data` call — so the invariant "usvg never receives gzip-magic'd
   bytes" is enforced both where it's established (point 1) and where it's relied upon (point 2), and a
   future change upstream in `rasterize_svg` can't silently reintroduce this bug class without also
   tripping this second guard.

Also confirmed (per the coordinator's point 3) that `reference_chain_too_deep`'s own defense-in-depth gzip
branch (added in attempt 1, unreachable in the normal `rasterize_svg` path) **cannot** be reached with
still-compressed bytes on that path — both enforcement points above run strictly before it — and even if
called directly with a doubly-gzipped stream, its one decompress still leaves the result gzip-magic'd,
which then fails that function's own UTF-8 conversion (gzip's binary bytes are essentially never valid
UTF-8) and returns `false` gracefully; that function never hands anything to `usvg`, so it has no
OOM/overflow exposure from the residual case, only a harmless missed pre-scan rejection. Documented this
reasoning inline in its doc comment.

Also fixed the attempt-1 module doc comment's now-inaccurate claim that bytes past the decompress "never
carry the gzip magic anymore" (true only for single-gzip) and added a full "CPE-1445 attempt 2" section
explaining the double-gzip vector and the reject-based fix.

### Tests added

`tests/thumb_svg_panic_safety.rs` (battery):
- `rasterize_svg_never_stack_overflows_on_a_doubly_gzipped_deeply_nested_svg_on_a_small_stack` — the exact
  auditor reproducer, `gzip(gzip(deeply_nested_svg(4000)))`, on the 256KB probe → **Err**, not overflow.
- `rasterize_svg_never_stack_overflows_on_a_triply_gzipped_deeply_nested_svg_on_a_small_stack` — N=3 →
  **Err** too (the guard rejects unconditionally after one decompress, regardless of remaining layers).
- `rasterize_svg_rejects_a_doubly_gzipped_bomb_without_unbounded_inner_decompression` — `gzip(gzip_bomb(200MiB))`,
  compressed fixture built streamed (no GB-scale allocation in the test itself) → **Err**.
- `rasterize_svg_renders_a_legit_single_gzip_svgz_fine_after_the_double_gzip_guard` — single-layer SVGZ
  still renders **Ok** (the new guard doesn't over-reject legitimate one-layer input).
- `rasterize_svg_renders_a_plain_uncompressed_svg_fine_after_the_gzip_fixes` — a never-gzipped `.svg` is
  untouched by either gzip guard, renders **Ok**.

`src/thumb_svg.rs` unit tests:
- `rasterize_svg_rejects_a_doubly_gzipped_svg_even_when_the_inner_document_is_tiny_and_legit` — proves the
  reject fires purely on layer count, independent of what's inside.
- `rasterize_svg_single_gzip_still_renders_after_the_double_gzip_guard` — boundary regression.

All prior single-gzip / plain-SVG cases from attempt 1 kept green (27 → 32 battery tests total).

### Verification (all synchronous, in `crates/server`, DEBUG)

- `cargo build` — clean.
- `cargo build --tests` — clean.
- `cargo clippy --all-targets -- -D warnings` (default features) — clean.
- `cargo clippy --all-targets --features index -- -D warnings` — clean.
- `cargo test --test thumb_svg_panic_safety` — **32/32 pass** (27 prior + 5 new double/triple-gzip cases).
- `cargo test` (whole crate, debug) — **1726 passed, 0 failed**.

Pushed to the same branch `cpe-1445-svgz-gzip-guard`, updating PR #713. No new dependencies.
