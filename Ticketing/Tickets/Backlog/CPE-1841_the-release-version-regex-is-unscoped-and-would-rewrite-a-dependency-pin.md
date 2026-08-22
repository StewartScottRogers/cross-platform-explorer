---
id: CPE-1841
title: the release version regex is unscoped, so it would rewrite a dependency pin to the app version
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-21
closed:
---

## Problem

`scripts/release.ps1` bumps the version in three manifests with an un-anchored, unlimited `-replace`.
It is not scoped to the top-level key or the `[package]` block, so **any** version-shaped string in the
file is rewritten to the new app version.

Measured during the CPE-1834 UAT, on both `main` and that PR's branch — this is pre-existing and neither
introduced nor fixed there:

- A `Cargo.toml` carrying a long-form dependency table:
  ```
  [dependencies.somepkg]
  version = "1.2.3"
  ```
  → the dependency's `1.2.3` was rewritten to `9.9.9`, the app version.
- A `package.json` carrying a nested `"someTool": { "version": "3.2.1" }` → rewritten to `9.9.9`.

## Why it matters, and why it is not urgent

**Dormant today.** The real `src-tauri/Cargo.toml` uses only inline `{ version = "…" }` dependency
syntax, and the real `package.json` has a single top-level `"version"` key. Nothing currently trips it.

**But it is a trap on the release path.** The moment anyone adds a long-form dependency table — the
ordinary way to express a dependency with features — a release would silently rewrite that pin to the
app's version number. The build would then either fail confusingly or, worse, resolve a different
dependency version than intended. And the release script is the least-exercised code in the repo,
run when nobody is watching.

It also interacts with the five-files-in-sync rule that already gets missed: a bump that changes more
than the version line makes a dirty tree read as expected noise.

## Acceptance criteria

- [ ] Each replacement is scoped to the key it means: `package.json`'s **top-level** `"version"`,
      `tauri.conf.json`'s **top-level** `"version"`, and `Cargo.toml`'s `version` **inside `[package]`**
      only.
- [ ] A dependency pin, a nested tool version, and a version-shaped string in a description or URL are
      all left alone. Test each.
- [ ] Red-proof: craft a manifest containing both the real version and a decoy, run the bump, assert only
      the real one changed. Then revert the scoping and confirm the test reds.
- [ ] Still a one-line diff on the real manifests — CPE-1834's UAT measured `1 1` per file with CRLF and
      the trailing newline preserved, and that must not regress.
- [ ] If a manifest ever fails to match at all, the script must fail loudly rather than silently writing
      an unchanged file — check whether it does today and fix it if not. A release that reports success
      having bumped nothing is the same "fails by succeeding" shape this repo keeps closing.

## Notes

Found by the independent UAT during CPE-1834, which was an encoding-only ticket and correctly did not
absorb this. That PR fixed a genuinely subtle adjacent bug: the read side was lossy as well as the write
side, and the two cancelled out, so a write-only fix would have turned an accidentally-safe round trip
into guaranteed double-encoded corruption.

One more thing that UAT flagged and could not exercise, worth checking while in this file:
`File.ReadAllText(path, encoding)`'s underlying `StreamReader` **still auto-detects and strips a BOM**
even when an explicit encoding is passed, so a BOM'd source manifest would not behave the way CPE-1834's
reasoning assumes. Moot today because the mojibake guard asserts no repo file carries a BOM, but it is an
untested assumption sitting under the release path.
