---
question: "Which untrusted-input parsers lack adversarial fuzz coverage, and what did fuzzing them find?"
date: 2026-08-07
status: current
tags: [security, fuzzing, panic-safety, dos, untrusted-input, archive, svg, font, webdav, jwt, wire, cpe-1398, cpe-1411, cpe-1413]
---

# Untrusted-parser adversarial fuzz sweep — found 4 real DoS/hang bugs

**The vein: adversarially fuzz every parser of UNTRUSTED bytes (opened file contents / network payloads).**
Static "no panics found on inspection" is NOT sufficient — only actually attacking the recursive parser finds
these. Highly productive: this sweep found FOUR real DoS/hang bugs + one upstream crate bug.

## Findings (all this session)
- **WebDAV `parse_multistatus` (CPE-1398, crates/webdav):** deep-nested XML → uncatchable stack-overflow crash
  (roxmltree recurses per level). FIXED with a non-recursive `xml_nesting_too_deep` guard (cap; xmlparser::Tokenizer
  in webdav). LESSON: first hand-rolled fix had a QUOTE-UNAWARE bypass (`<a b="/>">` miscounted as self-closing) —
  a security guard needs a re-review that TRIES to evade it.
- **SVG `thumb_svg.rs` deep nesting (CPE-1413, crates/server):** same usvg→roxmltree recursion → stack overflow.
  FIXED with a quote/comment/CDATA/PI/DOCTYPE-aware non-recursive `xml_nesting_too_deep` (MAX=64), run before usvg.
  Reviewer built 6 evasion payloads incl. the webdav quote-bypass shape — NONE bypassed (worker made it
  quote-aware from the start, learning from CPE-1398). roxmltree 0.20 internalized its lexer (xmlparser NOT a dep
  of it anymore) so hand-rolled was fine here.
- **SVG mutual `<use>`/`<symbol>` reference cycle (CPE-1414, DEFERRED):** 2-hop cycle stack-overflows a 256KiB
  stack; usvg only guards direct self-reference. SAFE on prod 2MB Tokio spawn_blocking stacks → low risk.
  `#[ignore]`d reproducer; needs a non-recursive cycle detector (deferred — fragile like CPE-1398's bypass).
- **ISO `archive.rs:iso_entries` infinite-loop HANG (CPE-1411, FIXED):** iso9660 0.1.1's dir iterator never
  advances on a parse error → `Err(_) => continue` re-reads the same bytes FOREVER. Fixed `continue`→`break`.
- **sevenz-rust 0.6.1 overflow panic (CPE-1415, upstream, contained):** unchecked `u64+u64` on attacker bytes.
  Panic is CONTAINED (all call sites are spawn_blocking → task boundary catches → Err; no panic=abort). `#[should_panic]`
  tests pin it + flip red on a future upgrade. Optional catch_unwind mitigation = CPE-1415.

## What's now COVERED (adversarial batteries exist — don't re-scout)
- `crates/server/tests/parser_panic_safety.rs` (34 &[u8] entrypoints) + `binary_data_preview_panic_safety.rs`
  (pe/midi/wasm/torrent/spreadsheet/sqlite/rar/camera_raw/dicom/**font-glyph** since CPE-1412).
- `crates/server/tests/archive_panic_safety.rs` (zip/tar/gz/7z/iso listing+extract — CPE-1411).
- `crates/server/tests/thumb_svg_panic_safety.rs` (CPE-1413). webdav + jwt batteries in their own crates.
- Font glyph (CPE-1412, ab_glyph SFNT/glyf) — fuzzed, NO bug found (held up).

## Still open / low-yield
- CPE-1416 (wire.rs read_envelope unbounded read → memory DoS) — in progress.
- Low-yield (scout ruled out): http_embedder (serde_json typed, bounded), net_share parsers (local OS output),
  code_intel/outline (dependency-free line scanners, no recursion), vault_crypto (post-AEAD-auth plaintext).

## Reusable pattern
`common::run_battery` + `assert_no_panic` (catch_unwind). For STACK-OVERFLOW probes: run on a
`std::thread::Builder::stack_size(256*1024)` thread — an overflow is UNCATCHABLE by catch_unwind, so a failed
`.join()` is the crash detector. Guard the recursive-parser DoS with a NON-recursive pre-scan (depth cap) BEFORE
the vulnerable parser; make the scan quote/comment/CDATA/PI-aware or use a real tokenizer.
