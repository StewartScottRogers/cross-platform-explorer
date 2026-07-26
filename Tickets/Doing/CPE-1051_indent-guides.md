---
id: CPE-1051
title: "Indent guides — cpe_server::indent_guides (per-line guide depth)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-724
estimate: 2h
---

## Summary
Child of CPE-724 (Code intelligence preview). Add a **pure, dependency-free** heuristic that computes the
indent-guide depth for every line of a source file, so the future preview can draw continuous vertical
indent guides. Backend-only, verified by `cargo test` on the 3-OS matrix — no GUI, no user resource.

## Design (buildable)
New module `crates/server/src/indent_guides.rs`, registered with `pub mod indent_guides;` in
`crates/server/src/lib.rs`.

```rust
/// Guide depth for each line (index i == line i+1). Depth = number of indent steps of leading whitespace.
pub fn indent_levels(source: &str, tab_width: usize) -> Vec<u16>
```

Algorithm:
1. For each line, measure the **leading-whitespace column width**: spaces count 1, a tab advances to the
   next multiple of `tab_width` (mixed tabs/spaces handled by column math, not raw char count). Treat
   `tab_width == 0` as 1 to avoid div-by-zero.
2. Convert the column width to a **depth** = `col / tab_width` (integer). This is the raw per-line depth for
   non-blank lines.
3. **Bridge blank lines**: a blank or whitespace-only line gets the **min of its nearest non-blank
   neighbours' depths above and below** (so a vertical guide doesn't visually break across a blank line
   inside a block). A blank run at the very start or very end (no neighbour on one side) takes the single
   available neighbour, or 0 if neither exists.
4. Return `Vec<u16>` (saturate to u16).

O(n) with a single forward pass to record non-blank depths + a second pass to bridge blanks. Std only.

## Acceptance Criteria
- [ ] Depth increases with nesting (e.g. 0,1,2 for progressively-indented lines at `tab_width` 4).
- [ ] Tab-indented and equivalent space-indented sources yield the **same** depths (tab expands to
      `tab_width`).
- [ ] A blank line **between** two depth-2 lines reports depth 2 (bridged); leading/trailing blank lines
      report the single available neighbour's depth (or 0).
- [ ] Empty input → empty vec; `tab_width == 0` doesn't panic.
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default and
      `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a clean headless CPE-724 slice. Independent module;
only shared touch is a one-line lib.rs `pub mod` (serial-merge coordination only).
