---
id: CPE-1591
title: "Check archive safety… reports a password-protected zip as \"No zip-bomb risk\" without ever scanning it"
type: Bug
status: Backlog
priority: Medium
component: Backend
tags: [ready]
created: 2026-08-10
---
## Why
Found while writing the deep Archives doc page (CPE-1587, epic CPE-1569) and verifying "Check archive
safety…" against the real code (`crates/server/src/archive_safety_scan.rs`,
`src/lib/components/ArchiveSafetyDialog.svelte`).

`analyze_archive_safety_with_limits` (`crates/server/src/archive_safety_scan.rs`) opens the zip's central
directory (which succeeds without a password), then loops:

```rust
for i in 0..zip.len() {
    ...
    let Ok(entry) = zip.by_index(i) else { continue };
    entries.push(EntrySizes { ... });
}
```

For an **AES-encrypted** entry, `ZipArchive::by_index()` needs the password and returns `Err` — the same
fact the app's own extract path already accounts for elsewhere (`archive.rs`'s comment: "AES-encrypted zips
can't be LISTED without the password either — the zip crate needs it just to construct the per-entry
reader"). Here, that `Err` is silently `continue`d past, so **every** entry in a password-protected zip is
skipped, leaving `entries_scanned: 0`.

Because the archive *did* open successfully (only the per-entry reads failed), `unreadable` stays `false` —
unlike the CPE-1320 fix for a genuinely corrupt/unopenable zip, which sets `unreadable: true` precisely so
the dialog doesn't render a misleading "safe" verdict. Here, the report collapses to `entries_scanned: 0,
report.dangerous: false, unreadable: false` — the exact shape `ArchiveSafetyDialog.svelte` renders as the
**"No zip-bomb risk detected." safe banner**, for an archive that was never actually scanned for zip-bomb
risk because nobody supplied the password.

## Scope
`analyze_archive_safety`/`analyze_archive_safety_with_limits` need to distinguish "opened fine, zero
scoreable entries" from "opened fine, but every entry needed a password we don't have" — the latter should
report as `unreadable: true` (or a new dedicated tri-state) rather than silently rendering as safe, mirroring
the CPE-1320 precedent for corrupt archives. `ArchiveSafetyDialog.svelte` currently has no password input at
all (`Check archive safety…` runs on mount with no prompt), so the fix likely needs either: (a) detecting the
"needs a password" case specifically and showing a clear "password-protected — can't check safety without the
password" state, or (b) accepting an optional password the same way extract does. Pick whichever is simpler
and matches the dialog's existing single-scan-on-mount shape.

## Acceptance criteria
- A password-protected zip's safety check never renders the plain "No zip-bomb risk detected." safe banner
  when it couldn't actually read any entries.
- A new backend test proves an AES-encrypted zip full of a would-be-dangerous entry doesn't silently report
  `dangerous: false` (i.e. it now surfaces the "couldn't scan" state instead).
- `cargo test -p cpe-server` and the existing `ArchiveSafetyDialog.test.ts` suite stay green (updated for the
  new state if needed).

## Notes
Docs-audit find, same class of bug as CPE-1320 (which fixed the *corrupt*-archive case) but for the
*encrypted*-archive case, which CPE-1320 didn't cover. See the new `src/docs/explorer-archives.md` (CPE-1587)
for the full, verified behavior this ticket is fixing.
