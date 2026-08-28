---
id: CPE-1954
title: `catalog-sign verify` is the one path-forming index read that does not go through `VerifiedIndex` — and it is the one input that never passes `sign_bundle`
type: bug
priority: Low
status: In Progress
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

`sidecar/host/src/bin/catalog_sign.rs:60` parses a catalog index with `CatalogIndex::from_json`
rather than `VerifiedIndex::open`, then does `dir.join(format!("{}.json", entry.id))`. It is the only
remaining path-forming read of an index that does not go through the verifying constructor CPE-1940
introduced and CPE-1949 added the `entry.id` charset rule to.

## The instance is NOT closed by `sign_bundle`, and that is the point

The Foreman's first framing was that CPE-1949's `sign_bundle` check closes the instance and only the
*class* remains. PR #1063's worker corrected it, and the correction is the load-bearing part:

> `sign_bundle` guards what **this repo publishes**. `catalog-sign verify <dir> <pubkey>` reads a
> directory the maintainer points it at, under a key they supply on the command line — an inspected
> third-party or downloaded bundle never passes through `sign_bundle` at all. So the traversal read
> survives for exactly the input that diagnostic exists to handle.

A maintainer verifying a bundle they did not build is the whole use case, and it is the use case
still unguarded.

## Why it stayed out of PR #1063

Unchanged and still valid: `VerifiedIndex::open` folds in the schema check, so verifying a
**future-schema** bundle would return "no index" rather than a verify result. Arguably more correct,
but it is a publishing-UX call with its own error wording, and it does not belong bolted onto a
security fix.

Severity is genuinely low — read-only, maintainer-run, and its verify-then-parse **ordering is
already right**, so this is not CPE-1940's defect recurring.

## Why it is still worth doing

Closing it makes *"every path-forming read of a catalog index goes through `VerifiedIndex`"* a
**statable, guardable invariant** instead of "all but one". An invariant with one exception is one a
future reader has to rediscover, and this repo has spent a week finding the cost of that shape.

## Acceptance criteria

- [x] Switch `catalog_sign.rs`'s verify path to `VerifiedIndex::open`.
- [x] **Handle the schema case deliberately.** A future-schema bundle must produce a message that says
      *the schema is unsupported*, not "no index" — that regression is the only reason this was
      deferred, and shipping it would trade a small hole for a confusing diagnostic.
- [x] **Demonstrate the traversal read first** on a third-party-shaped bundle with a hostile `entry.id`
      — the input `sign_bundle` never sees. Assert on the filesystem. If something upstream already
      constrains it, record that and close honestly.
- [x] **Then make the invariant guardable.** Add a check that no other site parses an index and forms a
      path from `entry.id` without going through `VerifiedIndex` — the pattern CPE-1933 established
      (read the source, assert on it) is the right shape, and `workflow_scan.rs` / the shared
      `cases.json` from PR #1060 are the worked examples. Red-proof it by reintroducing a bare
      `from_json` + `join` and confirming it reds.
- [x] Enumerate rather than recall: confirm there is no *third* site (CPE-1932).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1063's Security Auditor (non-blocking observation) and
its worker's correction of the Foreman's framing. That PR returned **SEC PASS**.

Related: **CPE-1949** (the charset rule, PR #1063), **CPE-1940** (`VerifiedIndex` and the
verify-before-use ordering, PR #1058), **CPE-1933** (derive claims rather than assert them — the shape
the guard here should take), **CPE-308** (the catalog pipeline).

## Work Log

### 2026-08-28 — implemented (branch `cpe-1954-catalog-sign-verifiedindex`)

**1. What the pre-fix path actually ran, established by execution rather than by reading names.**
`catalog-sign verify` ran three of the five checks and skipped two:

| check | pre-fix |
|---|---|
| index signature under the operator-supplied key (`verify_index`) | **ran** |
| per-manifest signature (`trust::verify_signature`) | **ran** |
| sha256 content binding (`CatalogEntry::matches`) | **ran** |
| supported index schema (`CatalogIndex::is_supported`) | *skipped* |
| `entry.id` is a single safe path component (`is_valid_entry_id`) | *skipped* |

Two checks named in the brief do not exist in this subsystem at all and so could not be skipped:
there is **no filename binding** (these are raw detached ed25519 signatures over the exact bytes —
no trusted comment, no `file:<name>`; that is the *updater's* minisign scheme), and **no version
floor** (anti-rollback lives in `apply_bundle_with`; `verify` applies nothing).

**2. The gap, demonstrated on the filesystem.** `tests/catalog_sign_verify_gate.rs` builds a
third-party bundle whose single `entry.id` is `../outside/evil`, signed by a key the operator names
on the command line. Against the pre-fix binary:

```
$ catalog-sign verify <root>/bundle <attacker-pubkey>
OK: index + 1 manifest(s) verify under the key      (exit 0)
$ ls <root>/bundle
catalog-index.json  catalog-index.json.sig          (no manifest at all)
```

Every byte it appraised came from `<root>/outside/`. A future-schema bundle was likewise accepted,
exit 0. **Honestly narrowed:** the other arms were *already* refused pre-fix — untrusted key, absent
signature, non-hex signature, non-UTF-8 signature, tampered bytes, missing files, junk pubkey, and a
non-UTF-8 index (that last one only incidentally, via `from_utf8_lossy` making the JSON
unparseable). So this was two missing checks, not a wholly ungated path.

**3. The fix.** Full `VerifiedIndex` routing — no partial alternative was needed.
`VerifiedIndex::open_reported` is added as the implementation; `open` becomes `.ok()` over it, so the
two can never drift about what is acceptable (pinned by
`open_and_open_reported_accept_exactly_the_same_inputs`). `IndexRefusal` carries the reason so the
schema case gets its own wording ("this tool is too old to appraise it", naming both versions)
rather than the bare "no index" that caused the original deferral. No second id check was added at
the `join`: it would be unreachable and would read as coverage (CPE-1929).

**4. The invariant, made guardable in two layers.** `CatalogIndex::from_json` is now `pub(crate)`, so
rustc refuses the convenient spelling from outside the module — reintroducing it in `catalog_sign.rs`
is `error[E0624]: associated function 'from_json' is private`, measured. The type is still `pub` and
`Deserialize`, so the back door (`serde_json::from_str::<CatalogIndex>`) is closed by
`src/lib/catalogIndexOneDoor.test.ts`, which enumerates every tracked `.rs` file with `git ls-files`
(353 today; refuses to render a verdict under 200), blanks comments with the shared
`stripRustComments`, and allows exactly one file. Red-proofed: injecting a turbofished parse into
`catalog_republish_downgrade.rs` reddens it, naming the file and the shape.

**5. Enumeration (CPE-1932), per-site verdicts.** Three `.rs` files mention `CatalogIndex` and eight
non-Rust files mention `catalog-index.json`. Verdicts: `catalog.rs` — the door, OK. `catalog_sign.rs`
— **was** the second door, now routed. `catalog_republish_downgrade.rs` — parsed an index but formed
no path (read `.sha256`); converted to `VerifiedIndex::open` anyway. `src-tauri/src/lib.rs` — already
through `VerifiedIndex` (CPE-1940/1952), no `join`. `catalog-freshness.yml` / `release.yml` /
`catalog-freshness-check.sh` / `catalog-version.sh` — read `.entries[].version` with `jq`, form no
path from `entry.id`. The three TS guard tests read workflows, not indexes. **No third path-forming
site.**

**6. CPE-1929 pair** (baseline 122 lib + 29 integration green; `Compiling sidecar-host` observed on
every run). Disable the entry-id refusal → **5 red**. Force `is_valid_entry_id` to return `true`
always → **7 red**. Both change behaviour, so nothing shadows it. Also measured the schema refusal:
disabled → **3 red**. Numbers written at the site in `catalog.rs`.

**Verification.** `cargo clippy --locked --all-targets -- -D warnings` clean (forced re-check via
`cargo clean -p`); `cargo test --locked --no-fail-fast` 122 lib + 29 integration green;
`npm run check` 0 errors; `npm test` 5097 passed with 19 pre-existing local failures in four
bash-executing files (`catalogPublish*`, `releaseVerifyWiringGuard`) — reproduced identically with
the change stashed, so not a regression.

**Not done, deliberately.** `release.yml`'s "Verify the signed bundle" step still only checks that a
`.sig` is present, not that it verifies — its comment names CPE-1954 as the enabler. Wiring it needs
the public key in CI and is a release-plumbing change with its own blast radius; worth its own
ticket now that the verify path is trustworthy.
