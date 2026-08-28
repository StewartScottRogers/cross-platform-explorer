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

> **Superseded in round 2 — this paragraph's central claim was false.** A **type alias** defeats both
> layers at once, along with seven other spellings, all demonstrated. `CatalogIndex` no longer derives
> `Deserialize` at all (it moved to a private wire type) and `from_json` is fully private, so rustc —
> not the scanner — is now the invariant. See the round-2 entry below.

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

> **Round 2 correction to the two numbers above.** *Integration tests are **26**, not 29* — the run
> is `122 lib passed (1 ignored) + 26 integration passed (2 ignored)`; the 29 was a miscount, the
> substance unaffected. And the "19 pre-existing local failures" were **not failures of that code**:
> they were a **missing `bash` on the PATH** of the shell round 1 measured in. Those four files
> execute shell scripts, so with no `bash` they cannot run at all. Re-measured in a shell that has
> one: `npm test` → **358 files, 5260 passed, 2 skipped, 0 failed**. "Not a regression" was the right
> verdict; "pre-existing local failures" under-diagnosed the cause.

**Not done, deliberately.** `release.yml`'s "Verify the signed bundle" step still only checks that a
`.sig` is present, not that it verifies — its comment names CPE-1954 as the enabler. Wiring it needs
the public key in CI and is a release-plumbing change with its own blast radius; worth its own
ticket now that the verify path is trustworthy.

### 2026-08-28 — round 2 (Reviewer findings on PR #1088)

The Reviewer would approve the security fix on the code alone; both findings were about the **guard
layer around it overstating what it enforces**. Rebased on `origin/main` (#1086 had landed).

**R1. The scanner did not close the back door its comment said it closed — fixed structurally, not
by softening the words.**

Round 1 held the invariant in two layers: `pub(crate) fn from_json` (rustc refuses the convenient
spelling from `src/bin/*`, which compiles as a separate crate — that half is real, `error[E0624]`)
plus `catalogIndexOneDoor.test.ts` sweeping tracked `.rs` files for
`serde_json::from_str::<CatalogIndex>`. Three sites said the two together *were* the invariant.

They were not, and the Reviewer showed it: a **type alias** defeats both at once.
`type Idx = sidecar_host::catalog::CatalogIndex; let a: Idx = serde_json::from_slice(b)?;` compiles,
forms `bundle/../outside/evil.json` at run time, and leaves the scanner **16/16 green**. Reproduced
here as a throwaway integration test carrying **seven** of the Reviewer's eight vectors — alias,
`use … as`, generic-helper turbofish, `#[serde(flatten)]` wrapper, return-position inference,
`TryFrom` impl, `Vec<CatalogIndex>` annotation (the eighth, in-crate `Self::from_json`, is not
expressible from a `tests/` crate). Result: **compiles, 1 passed**, seven `REVIEWER-ATTACK reached:`
lines. That is the CPE-1933 shape — an untrue assertion beside a green test that reads as vouching
for it.

**Took option (b), close it structurally**, because it turned out as cheap as it looked and a guard
the compiler enforces needs neither a scanner nor a claim. Nothing outside `sidecar/host` names
`CatalogIndex` (Reviewer-confirmed), and inside it only comments do, so:

- `CatalogIndex` and `CatalogEntry` **no longer derive `Deserialize`**; the derive moved to private
  `WireIndex` / `WireEntry`, field-for-field identical and `#[serde(rename = …)]` to the public names
  so serde's operator-facing error text is unchanged. `CatalogIndex::from(WireIndex)` runs only
  inside `from_json`.
- Both public types are **`#[non_exhaustive]`**, closing the field-by-field construction route from
  any other crate.
- `from_json` is now **private**, not `pub(crate)` — that kills vector 6 (`Self::from_json` from
  another module in this crate). It had no caller outside `catalog.rs` and should never have one.

**The Reviewer's attack re-run against the fix: all seven vectors are now compile errors** —
`error[E0277]: the trait bound 'CatalogIndex: serde::Deserialize<'de>' is not satisfied` ×6 (one is
`DeserializeOwned`, from the generic helper), `error: could not compile 'sidecar-host' (test
"zz_reviewer_attack_probe") due to 6 previous errors`. A separate probe constructing the two structs
by field: **`error[E0639]: cannot create non-exhaustive struct using struct expression` ×2**. Both
probes deleted after measuring.

`catalogIndexOneDoor.test.ts` is **reframed, not deleted**. It no longer claims to close a back
door — there is none left to close. It now (a) asserts the three source facts rustc's enforcement
rests on (no `Deserialize` on either public type; both `#[non_exhaustive]`; `from_json` not `pub` and
not `pub(crate)`), so a diff that re-widens any of them reds *with the reason attached* instead of
silently restoring the eight vectors; (b) keeps the text sweep as a cheap tripwire, explicitly
labelled non-exhaustive; and (c) **pins the eight known-uncovered vectors as `it.each` cases asserting
the sweep does *not* catch them**, so the blind spot is a test rather than a paragraph. Red-proofed:
re-adding `Deserialize` to `CatalogIndex` → **3 red** (the two `CatalogIndex` arms + "the only
Deserialize in the module is on a private wire type"). 16 tests → **30**, all green.

Said plainly at both sites and in the test header: what *nothing* here can catch is a caller who
declares their **own** struct, parses catalog bytes into it, and interpolates its `id`. That is a
reimplementation of the subsystem, not a door onto this type.

**R2. The drift test pinned 6 of 8 arms; the two missing were the ones that mattered.**
`open_and_open_reported_accept_exactly_the_same_inputs` had no `NotUtf8` and no `Unparseable` case.
Added both — `NotUtf8` as a lone `0xFF` inside a field serde **ignores** (`"note"`), so
`from_utf8_lossy` would turn it into U+FFFD and leave a document that still parses; "it wouldn't have
parsed anyway" is therefore not a defence.

Sabotage, both directions, `Compiling sidecar-host` observed on each: reimplement `open`
independently with `from_utf8_lossy` (keeping the schema and id checks).
- With round 1's case list: **121 passed, 0 failed** — the divergence is invisible, exactly as the
  Reviewer measured.
- With the two new arms: **1 red**, `open_and_open_reported_accept_exactly_the_same_inputs`, message
  `signed + not UTF-8: the two constructors disagreed about what is acceptable`.

`open` is the constructor `do_fetch_catalog` and `apply_bundle_*` gate on, so that was the arm worth
having. The case list is now one entry per `IndexRefusal` variant plus the accepting one; the reason
is written at the test.

**R3.** Both bookkeeping numbers corrected in the round-1 entry above (26 integration tests, not 29;
the 19 "failures" were a missing `bash`).

**Also confirmed, so the narrowing is known safe:** `src-tauri` builds clean with
`--features sidecar-platform` (it uses only `VerifiedIndex::open`, `MemBundle`,
`apply_bundle_source_at`, `ApplyOutcome` — it never deserialises or constructs either type).

**Round-2 gates.** `cargo clippy --locked --all-targets -- -D warnings` clean, forced with
`cargo clean -p sidecar-host` and `Checking sidecar-host` observed. `cargo test --locked
--no-fail-fast` (Windows): **122 lib passed / 1 ignored + 26 integration passed / 2 ignored, 0
failed**. `cargo check --locked --features sidecar-platform` on `src-tauri`: clean. `npm run check`:
**0 errors, 0 warnings**. `npm test`: **358 files, 5260 passed, 2 skipped, 0 failed** — measured in
Git Bash on Windows, which *has* `bash` on the PATH, hence the four script-executing files run here.
(WSL cannot link `tests/keyring_roundtrip.rs` — `undefined symbol: sd_listen_fds`, an environment
gap, not a code one; the authoritative cargo numbers above are the Windows run.)
