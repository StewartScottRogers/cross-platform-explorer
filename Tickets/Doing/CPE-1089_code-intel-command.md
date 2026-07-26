---
id: CPE-1089
title: "code_intel tauri command — expose outline/folds/indent/minimap to the frontend"
type: feature
component: Backend
priority: high
status: Doing
tags: ready
created: 2026-07-26
epic: CPE-724
---

## Summary
Child of CPE-724 (Code intelligence preview) — the GUI-enablement backend slice. The five code-intel modules
(`code_outline`, `code_folds`, `indent_guides`, `minimap`, `code_breadcrumb`) are pure library code wired
into **nothing** — no `#[tauri::command]`, no specta binding. Add ONE aggregate command so the file previewer
can fetch a file's symbols, fold ranges, indent depths, and minimap in one call, plus regenerate the frontend
bindings. Backend, `cargo test` + specta export — no GUI in this ticket.

## Design (buildable)
1. **Aggregate fn in `crates/server`** — a thin new module `crates/server/src/code_intel.rs`
   (`pub mod code_intel;` in `lib.rs`, distinct anchor) that composes the existing modules (does NOT modify
   them):
   ```rust
   #[derive(Debug, Clone, PartialEq, serde::Serialize)]
   #[cfg_attr(feature = "specta", derive(specta::Type))]
   pub struct CodeIntel {
       pub outline: Vec<code_outline::Symbol>,
       pub folds: Vec<code_folds::FoldRange>,
       pub indent: Vec<u16>,
       pub minimap: Vec<minimap::MinimapRow>,
   }
   pub fn analyze(text: &str, lang: &str, tab_width: usize, minimap_buckets: usize) -> CodeIntel;
   ```
   `analyze` calls `code_outline::outline`, `code_folds::fold_ranges`, `indent_guides::indent_levels(text,
   tab_width)`, `minimap::minimap_rows(text, minimap_buckets)`. Guard inputs: a 0 `tab_width` → treat as the
   module already does; `minimap_buckets` clamp to a sane max (e.g. `min(buckets, 512)`) so a silly value
   can't allocate huge; an empty text → all-empty CodeIntel (no panic).
2. **`#[tauri::command]`** in `src-tauri/src/lib.rs` (thin one-line dispatcher per CLAUDE.md), registered in
   BOTH `generate_handler![]` (~lib.rs:5843) AND the specta command list (~lib.rs:6245), next to
   `read_file_text`:
   ```rust
   #[tauri::command]
   fn code_intel(text: String, lang: String, tab_width: Option<usize>, minimap_buckets: Option<usize>)
       -> cpe_server::code_intel::CodeIntel
   ```
   Defaults: `tab_width.unwrap_or(4)`, `minimap_buckets.unwrap_or(120)`. Pure/in-memory (no fs, no path) —
   the frontend already has the file text loaded, so this takes `text` directly (no second file read). It can
   be sync (fast, in-memory) — but if the codebase convention is async commands, mirror that.
3. **Regenerate specta bindings** so `CodeIntel`, `Symbol`, `SymbolKind`, `FoldRange`, `FoldKind`,
   `MinimapRow`, and the `codeIntel` binding appear in `src/lib/bindings.gen.ts`. Find how bindings are
   generated (grep for how `bindings.gen.ts` is produced — likely a `specta::export`/`ts` test or a script;
   the existing commands like `readFileText` show the pattern). Ensure the `Symbol`/`FoldRange`/`MinimapRow`
   structs' `#[cfg_attr(feature="specta", derive(specta::Type))]` are present (the map says they already are)
   and that the bindings regenerate cleanly.

## ⚠ Notes
- `analyze` is pure over `&str` → clamp `minimap_buckets` (bounded allocation); no unchecked arithmetic; no
  recursion added. No new deps.
- Reuses the 5 modules unchanged. `code_breadcrumb::enclosing_symbols` is NOT needed here (the frontend can
  compute the breadcrumb client-side from `outline`+`folds`, saving a round-trip per cursor move) — leave it.

## Acceptance Criteria
- [ ] `code_intel::analyze(text, lang, tab_width, buckets)` returns outline+folds+indent+minimap; empty text →
      empty (no panic); a huge `minimap_buckets` is clamped (no huge alloc); `cargo test -p cpe-server` green.
- [ ] `#[tauri::command] code_intel` registered in `generate_handler!` + specta list; `cargo build` (app)
      succeeds.
- [ ] `src/lib/bindings.gen.ts` regenerated: `CodeIntel`, `Symbol`, `SymbolKind`, `FoldRange`, `FoldKind`,
      `MinimapRow` types + a `codeIntel(...)` binding present; `npm run check` clean.
- [ ] clippy `--all-targets -- -D warnings` clean (default AND `--features index`); no new deps.

## Work Log
2026-07-26 (workshift, GUI) — Filed by the Foreman as the backend enablement for the code-preview upgrade
(GUI #1). The five code-intel modules exist but reach nothing; this one aggregate command + regenerated
bindings unblock the frontend tickets (CPE-1090 breadcrumb/jump, CPE-1091 minimap/folds/indent).

2026-07-26 (Worker) — Implemented on branch `cpe-1089-code-intel-command`:
- New `crates/server/src/code_intel.rs`: `CodeIntel { outline, folds, indent, minimap }` +
  `analyze(text, lang, tab_width, minimap_buckets)`, composing `code_outline::outline`,
  `code_folds::fold_ranges`, `indent_guides::indent_levels`, `minimap::minimap_rows` unchanged.
  `pub mod code_intel;` added to `crates/server/src/lib.rs` right after `pub mod code_breadcrumb;`.
- `#[tauri::command] code_intel(text, lang, tab_width: Option<usize>, minimap_buckets: Option<usize>)
  -> cpe_server::code_intel::CodeIntel` in `src-tauri/src/lib.rs`, next to `read_file_text`; registered in
  both `generate_handler![]` and the specta command list.
- **Assumption — sync, not async.** `read_file_text`/`read_file_range` are async + `spawn_blocking`
  because they touch the filesystem; `code_intel` takes `text: String` directly (no fs, no path — the
  frontend already has the text loaded from `read_file_text`), so it's a plain sync fn per the ticket's
  own guidance ("can be sync (fast, in-memory)"). Matches existing sync, non-fs-blocking commands in the
  same file (e.g. `tag_counts`, `read_settings`).
- **Assumption — clamp semantics.** `minimap_buckets` is clamped with `.min(MAX_MINIMAP_BUCKETS)` where
  `MAX_MINIMAP_BUCKETS = 512` (the ticket's suggested cap), applied before calling `minimap::minimap_rows`
  — never allocates more than 512 minimap rows regardless of caller input. No special-case branch was
  needed for empty text: `outline`/`fold_ranges`/`indent_levels`/`minimap_rows` already return empty vecs
  for empty source (verified by their own test suites + a new `code_intel` test), so `analyze("", ...)`
  is already panic-free and all-empty without extra code.
- `code_breadcrumb::enclosing_symbols` intentionally left unwired, per the ticket's note.
- Tests added in `code_intel.rs` (`#[cfg(test)] mod tests`): non-empty Rust snippet populates every
  field; empty text → all-empty `CodeIntel`; `minimap_buckets: 100_000` on a 20-line file clamps to 20
  rows (never allocates 512, since there are fewer lines than the cap); `minimap_buckets: 50_000` on a
  1000-line file clamps to exactly 512; an unrecognised language still yields indent/minimap (outline/folds
  empty, since those are language-gated).
- Bindings regenerated with `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"`
  from `src-tauri/` (per `src-tauri/src/bin/export_bindings.rs`'s own doc comment) — confirmed `CodeIntel`,
  `Symbol`, `SymbolKind`, `FoldRange`, `FoldKind`, `MinimapRow` types and an async `codeIntel(...)` binding
  now appear in `src/lib/bindings.gen.ts`. The drift-guard test
  `typed_bindings_are_committed_and_routed_through_busy_cursor` passes against the regenerated file.
- Verified green: `cargo test` in `crates/server` (1004 passed, incl. 5 new `code_intel` tests); `cargo
  clippy --all-targets -- -D warnings` clean in `crates/server` for default, `--features index`, and
  `--features specta`; `cargo check`/`cargo clippy --all-targets -- -D warnings` clean in `src-tauri`;
  `npm run check` clean (0 errors/warnings). No new dependencies added.
- Left in `Tickets/Doing/` (not moved to Done) — PR opened but not yet merged; move to Done follows the
  repo's usual post-merge pattern (see CPE-1048's history).
