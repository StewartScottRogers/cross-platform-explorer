---
id: CPE-1950
title: derive the remaining cross-file provenance claims CPE-1933 classified but did not derive
type: task
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-27
---

## Why this exists

CPE-1933 swept the repo for **provenance claims in comments** — a comment asserting that a concrete
artifact *here* reproduces one that lives in another named file, which is untested by construction
and decays silently. Across a case-sensitive pass and a case-insensitive re-run it found **20 hard
hits**. CPE-1933 derived the highest blast-radius ones (four release-plumbing claims about
`release-sidecar.yml`, and the 34-chord keyboard registry) and deliberately stopped there rather than
doing all twenty shallowly.

These are the remainder. They are all **classified and understood** — none is a mystery, and each has
a comment at the site. What they lack is a runtime derivation.

**Two of them are already factually wrong today**, so the drift has happened and nobody noticed.
Those are the cheapest wins and the best argument for the whole exercise: start there.

## Already wrong — start here

1. **`crates/vfs/src/connect.rs:236`** — MED. Claims
   `crates/vfs/tests/real_server_conformance.rs`'s `remote()` helper mirrors `join_remote` "so the
   real-server-rig E2E job actually exercises the new shape". **False today**: that file's own doc
   (`:126-133`) says `remote()` mirrors the *old* `join_remote`; only `remote_dir()` carries the
   CPE-1737 trailing slash. So the E2E rig against real OpenSSH/vsftpd/mod_dav is silently testing a
   stale path shape.

2. **`src/lib/paths.ts:21`** — MED. Claims `canonicalPath` mirrors `Sidebar.svelte`'s local `norm()`
   "so every path-keyed consumer agrees with the sidebar's own notion of 'the same folder'".
   **False today**: Sidebar's `norm` strips *all* trailing slashes (`"/"`→`""`, `"C:/"`→`"C:"`) while
   `canonicalPath` deliberately preserves root and bare-drive forms. The stated invariant does not
   hold — a path-identity collision of the CPE-1737 class.

## The rest, in blast-radius order

3. **`src/lib/sidecarBundleResources.test.ts:268`** — **HIGH**. "Mirrors
   `crates/updater-verify/src/platform_config_guard.rs` … Keep the two derivations in lockstep."
   Duplicated: the literal `["macos","windows","linux","android","ios"]` token list, the ASCII-only
   case fold, the `segments.length >= 3` shape rule, and the RFC-7396 null-deletion refusal set.
   This is the **updater root-of-trust** guard — a token added on one side leaves a config-injection
   path green on the other. Highest-value item in this ticket.

4. **`src/lib/components/RepoBrowser.svelte:2`** — MED, security-adjacent. `PROVIDER_HOSTS`
   (`github.com`, `gitlab.com`, `bitbucket.org`, `codeberg.org`) claimed to mirror `clone_host()` in
   `src-tauri/src/lib.rs:11965`. Those host strings also build the lookalike-host anchoring regexes,
   so a backend-side provider addition silently stops `stripRepoUrl` stripping its host.

5. **`src/lib/replayFold.test.ts:7`** — MED. Claims to port the Rust reconstruction tests
   (`crates/server/src/replay.rs`, `replay_view.rs`) "verbatim as the ORACLE … so the TS fold and the
   Rust fold provably agree". It reads nothing at run time — the oracle is a hand-copy, so each suite
   can go green on different behaviour.

6. **`crates/server/src/audit_journal.rs:18`** — MED. "Mirrors the frontend `AuditEvent`
   (`src/lib/auditExport.ts`)" — the field set of a **persisted on-disk JSONL record**. There are two
   live definitions (specta generates one into `bindings.gen.ts`; `auditExport.ts` hand-writes
   another) and `SessionHistoryDialog.svelte:64` bridges them with an `as AuditEvent[]` cast, which
   is exactly what makes drift silent.

7. **`src/lib/releaseHangHardening.test.ts:66` and `:72`** — MED. Two CI guards each hold their own
   copy of `HARDENING_FLAGS` (the full apt option string) and `APT_COMMAND_WORD` (the regex CPE-1787
   deliberately widened), re-declared rather than imported from `ciAptGetHardening.test.ts:70`.
   Cheapest possible fix: export and import.

8. **`src/lib/batchMedia.ts:71` and `:227,236`** — MED. Three sites, not two: `colon_is_a_path_character`,
   `lexical_normalize` and the `fold_case` platform gate, all reimplemented from
   `crates/server/src/batch_media.rs`. Note the platform predicate uses *different mechanisms* on the
   two sides (Rust `cfg!(windows)`, TS a `navigator` sniff). This drives the collision preview for a
   **destructive** batch operation: drift means the dialog predicts a different collision set than
   the backend executes.

9. **`crates/server/examples/gen_vault_fixture.rs:25`** — MED. "Must match `VAULT_FIXTURE_PASSPHRASE`
   in gui-smoke/wdio.conf.ts" — a literal passphrase (`open-sesame-1249`) duplicated across the
   Rust/TS boundary, plus the base64 blob it generates.

10. **`crates/server/src/archive.rs:7038`** — MED, rising to HIGH on a dep bump. Reproduces "the
    **exact** two-level wrap `tar-0.4.46/src/entry.rs` puts around a link-creation failure", cited to
    upstream line numbers. A `tar` bump is a one-line lockfile change that reds nothing here, leaving
    the fixture modelling 0.4.46 while production faces a new shape. Pin the version assumption to
    `Cargo.lock` at minimum.

11. **`crates/s3/src/provider.rs:544`** (+ `crates/s3/Cargo.toml:50`) — LOW-MED. `xml_nesting_too_deep`
    ported "near-verbatim" from `crates/webdav/src/lib.rs:433`. Two copies of a parser-DoS guard.
    Probably better **extracted** into a shared crate than derived.

12. **`src/lib/entrySearch.ts:159-163,205`** — LOW. `MAX_ABS_YEAR = 99_999` and the `days_from_civil`
    port duplicated from `crates/server/src/date_filter.rs:167`.

13. **`src/lib/spotlightSources.ts:162`** — LOW-MED. The fuzzy matcher (greedy first-fit subsequence
    + case fold) reimplemented from `spotlight.rs`'s `fuzzy_score`. The folds already differ in kind
    (JS `toLowerCase()` is full-Unicode, Rust's is per-char), so a non-ASCII query can disagree today.

14. **`src/lib/agentMetricsRollup.ts:94`** — LOW. Divisors duplicated from
    `sidecar/ai-console/src/efficiency.rs` — two dashboards can show the same metric in different
    units.

15. **`gui-smoke/specs/vault-create.smoke.ts:27`, `trash-titlebar.smoke.ts:39`** — LOW. Fixture
    literals restated though `wdio.conf.ts` already **exports** them (`SHRED_DIR_NAME`,
    `VAULT_CREATE_PARENT_DIR`, `VAULT_CREATE_BLOB_NAME`, `TRASH_TITLEBAR_FILE_NAME`). No new
    machinery needed — just import them.

16. **`scripts/dev-harness/revert-heldback-copy/main.ts:33`** — LOW. Two long user-facing strings
    "lifted from `revert_engine.rs`", character for character. The harness exists *specifically* so a
    human reviews real wire copy, so the claim is the whole point of the file.

## Acceptance criteria

- [ ] For each: **derive** it (read the referenced source at run time and assert), **delete the
      claim**, or mark it **genuinely unavoidable at the site** with the reason.
- [ ] Red-proof every derivation — change the referenced source, watch the test fail.
- [ ] Anchor scanners on **code, not prose**, and do not hand-roll a comment stripper: use
      `src/lib/shellScriptLines.ts` or its Rust port `crates/updater-verify/src/workflow_scan.rs`.
      A whole-line-comment filter is not enough; a trailing comment walks through it.
- [ ] Items 15 and 7 need no new machinery — import what is already exported.
- [ ] Item 11 may be better solved by **extracting** the shared guard than by deriving a copy.

## Notes

Filed 2026-08-27 by CPE-1933's worker, under that ticket's instruction to prioritise by blast radius
and do the high-blast-radius hits properly rather than all of them shallowly. The standard these
follow is in `CLAUDE.md` → **Guards and ratchets** → *"Derive provenance, don't claim it"*.

The sweep also found **150 SOFT** hits — comments describing design kinship with no drift-able value
("mirrors `loadPath`'s own HOME short-circuit", "Mirrors the repo's established component-test
mocking") — which are **not** defects and need no action, and **7 already-derived** sites. Two are
worth citing as models: `gui-smoke/lib/samplesNav.ts:44` records in place that its own old
byte-identity claim was deliberately retired by CPE-1679, and
`src/lib/releaseVerifyWiringGuard.test.ts:268` explicitly *refuses* to make a provenance claim and
executes both workflows' scripts instead.

Related: **CPE-1933** (the sweep), **CPE-1917** (built the deriving pattern), **CPE-1872** (where the
stale claims lived), **CPE-1929** (shadowed guards), **CPE-1932** (rules from memory).

## Work Log

### 2026-08-27 — round 1: seven closed, nine left, deliberately

Worked in blast-radius order and stopped rather than doing the rest shallowly. **Seven of the sixteen
are closed** (items 1, 2, 3, 4, 7, 9, 15). Every one re-reads its source at run time or removes the
duplication outright; every one was red-proofed by changing the referenced source and watching the
test fail, and **the red-proof result is written at the site**, not only in the PR body.

**TWO claims were already factually wrong.** The ticket predicted items 1 and 2. Item 2 held; **item 1
did not** — see below. Item 7 turned out to be a genuine third-that-is-really-the-second, found while
fixing it. So the count is unchanged at two, but the membership is different from the ticket's guess.

**Correction (PR #1067 review, round 1).** My first draft claimed item 1 was already false and that
the real-server rig had been testing a stale path shape. That was **wrong**, and the wrong text had
been written into two source files. `git show b15c9f7b` (CPE-1737 #908) confirms `fn remote_dir` — which
DOES append the slash — and the `connect.rs` sentence landed in the **same commit**, with
`assert_slashed_directory_path_round_trips` driving `mkdir`/`stat`/`list`/`delete` through it against
real OpenSSH/vsftpd/mod_dav from day one. The comment named the **wrong helper** (`remote()` instead of
`remote_dir()`); its **conclusion was true**. Both doc comments now say that. The refactor stands on
its own merits — a misnamed cross-file reference is still a claim nothing can check — but it is not
evidence of drift, and the PR body's "byte-identical" line was the tell that should have stopped the
first draft.

| # | site | classification | what landed |
|---|---|---|---|
| 1 | `crates/vfs/src/connect.rs:236` | **duplication deleted** (claim was MISNAMED, not false) | `join_remote` is now `pub`; `real_server_conformance.rs`'s `remote()`/`remote_dir()` **call it** instead of reimplementing it. The sentence pointed at `remote()`, but the slashed CPE-1737 coverage came from `remote_dir()` — both it and the sentence landed in b15c9f7b, so the conclusion was always true. Red-proof: the compiler, on every build. |
| 2 | `src/lib/paths.ts:21` | **claim corrected + derived** | Confirmed false at `Sidebar.svelte:261`: `norm("/") === ""` vs `canonicalPath("/") === "/"`. `norm` moved into `paths.ts` as `treePrefixPath` (one definition, Sidebar imports it); `paths.test.ts` derives the REAL relationship — agreement on every non-root input, deliberate divergence at the roots, plus an executed counterfactual showing why (`isAncestorOrSelf`'s `startsWith(a + "/")` needs the root to collapse). Red-proof: `treePrefixPath = canonicalPath` fails 2 of 4. |
| 3 | `src/lib/sidecarBundleResources.test.ts:268` | **derived, two legs** | HIGHEST value item, and it is done properly. Leg 1: the TS platform-token list is now READ out of `platform_config_guard.rs`'s `TAURI_PLATFORM_TOKENS` (comments stripped), so a token added on one side reds with a SECURITY message and nobody has to write a case. Leg 2: new shared oracle `src/lib/platformConfigGuard.cases.json` (24 name cases + 20 refusal cases) executed by BOTH the TS suite and a new Rust `both_implementations_agree_on_every_shared_case`. Red-proofs: `+"visionos"` on the Rust const reds leg 1; making the Rust matcher demand an extra segment reds leg 2 on `Tauri.<t>.toml`. **Stated limit, at both sites:** a shared oracle catches divergence, not shared blindness — see the `<<`-in-a-quoted-string precedent from #1060. |
| 4 | `RepoBrowser.svelte:2` | **derived** | `PROVIDER_HOSTS` is exported and `RepoBrowser.test.ts` reads `clone_host()`'s `if/else` chain out of `src-tauri/src/lib.rs`, comments stripped. Checks **both** directions — the backend-adds-a-provider direction is the one that matters — plus that the self-hosted providers are deliberately absent and that `stripRepoUrl` really consumes each derived host (incl. the lookalike-host anchoring). Red-proof: a `sourcehut` branch reds with `git.sr.ht`. |
| 7 | `releaseHangHardening.test.ts:66,72` | **duplication deleted — and it was ALREADY FALSE** | The comment said "Verbatim from ciAptGetHardening.test.ts". It was not: CPE-1916 widened the command-word lookbehind there (`(?<![\w-])` → `(?<![\w\-/])`) and this copy never followed, so a `/etc/apt/…` path segment counted as an apt invocation in one suite and not the other. Two green suites, both claiming to hold the same regex, holding two. Both constants now live in `src/lib/aptGetHardening.ts`; both suites import. Red-proof: `Retries=3` → `=4` reds **both** (6/6 and 5/26). |
| 9 | `gen_vault_fixture.rs:25` | **derived (text, cross-language)** | New `src/lib/guiSmokeFixtureLiterals.test.ts` reads the Rust example's `PASSPHRASE` and first sealed `TreeEntry` path (comments stripped) and compares them with `wdio.conf.ts`'s exported `VAULT_FIXTURE_PASSPHRASE` / `VAULT_FIXTURE_INNER_NAME`. Red-proof: `open-sesame-1250` reds. |
| 15 | `vault-create.smoke.ts:27`, `trash-titlebar.smoke.ts:39` | **derived, NOT imported** | The ticket suggested importing wdio.conf.ts's exports. Rejected on purpose: that duplication is a documented runner/worker-boundary convention, gui-smoke cannot be run locally, and its CI leg is currently red for unrelated reasons (CPE-1955) — changing what the workers import, unverified, is worse than the claim. Instead the same new root-vitest guard compares the five declarations across the two files, anchored at **column 0 on a real `const`/`export const`** so a commented-out copy cannot match. Red-proof: renaming `SHRED_DIR_NAME` in `wdio.conf.ts` reds. |

**New shared machinery** (so this does not become a fifth hand-rolled stripper):
`src/lib/rustSource.ts` — `stripRustComments` + `rustStringLiteralAfter` lifted out of
`MacroRunConfirm.test.ts` (which now imports them), plus a new `rustStrSliceAfter` for `&[&str]`
consts. Tested in `src/lib/rustSource.test.ts`, including the adversarial "a comment quoting the OLD
list" case. There is no Rust port of it, so unlike `shellScriptLines.ts` it is pinned by nothing
cross-language — it is a *reader*, not a reimplementation, so there is no second copy to drift from.

**Round 1 review found the scanner itself was the biggest defect in the PR** (PR #1067, Blocking 2),
and the finding was correct and reproduced locally. The lifted stripper tracked only `"` strings, and
its doc asserted every desync failed **loudly**. Both false, and false about the files it is pointed
at *today*:

| file | scanned by | surviving `///` lines, old scanner | after |
|---|---|---|---|
| `src-tauri/src/lib.rs` (`path.contains('"')` at :8253) | `RepoBrowser.test.ts` | **142** (8268–8959) | 0 |
| `crates/server/src/fsutil.rs` (`r"\\?\UNC\"` at :3379) | `MacroRunConfirm.test.ts` | **31** (from 3385) | 0 |

And silent, not loud: behind a `'"'`, a commented-out `TAURI_PLATFORM_TOKENS` decoy **beat the real
declaration** — on the updater root-of-trust guard. The three shipped derivations sat outside the
leaked windows: a parity coincidence, not a property. Now handled: char literals, raw strings
(incl. `br#"…"#`), and **nested block comments** (legal Rust, and not in the original limitations list
at all). Plus the reviewer's cheapest suggestion, which is the part that generalises: after stripping,
**no line may begin with `//`** — a two-line invariant that throws on a desync of any cause, including
one nobody modelled. It catches all 173 leaked lines. Red-proofed three ways (disabling char literals
fails 3 tests, raw strings 2, block-comment depth 1); `rustSource.test.ts` now carries a case for each
shape plus a regression leg over all four scanned files.

### Left for round 2 — nine items, and why

Not "ran out of time" in every case; several are judgement calls worth restating.

- **5. `replayFold.test.ts:7`** (MED). The hand-copied Rust oracle is the right shape of problem for a
  *shared case file* (like item 3's), not for a source scan: the Rust tests are `assert_eq!` calls,
  not data. Doing it properly means extracting the fixtures from `replay.rs`/`replay_view.rs` into a
  shared JSON and rewriting both suites to consume it — a real change to two test suites, not a
  bolt-on. **Do it next; it is the largest remaining MED.**
- **6. `audit_journal.rs:18`** (MED). Blocked on a design question this ticket should not settle
  alone: there are **three** live definitions of `AuditEvent` (specta → `bindings.gen.ts`, the
  hand-written `auditExport.ts`, and the Rust record), bridged by an `as AuditEvent[]` cast in
  `SessionHistoryDialog.svelte:64`. The fix is to delete one of the three, not to derive a comment.
- **8. `batchMedia.ts:71,227,236`** (MED). Three sites, and the platform predicate genuinely differs
  in *mechanism* (`cfg!(windows)` vs a `navigator` sniff), so it cannot be derived — only pinned by a
  shared case file with an explicit platform axis. Same shape as item 5; do them together.
- **10. `archive.rs:7038`** (MED→HIGH on a dep bump). Wants a `Cargo.lock`-reading assertion that the
  `tar` version still is 0.4.46, which is a different (and useful) guard from the ones here. Small
  and self-contained — good next pick after 5.
- **11. `s3/provider.rs:544`** (LOW-MED). The ticket's own note is right: **extract** the shared
  parser-DoS guard rather than derive a second copy. That is a code move across two crates and
  belongs in its own ticket.
- **12. `entrySearch.ts:159-163,205`**, **13. `spotlightSources.ts:162`**, **14.
  `agentMetricsRollup.ts:94`**, **16. `revert-heldback-copy/main.ts:33`** (LOW). Left as the lowest
  blast radius.
  **Correction on 13 (PR #1067 review): do NOT split it out — my earlier note was wrong.** I wrote
  that the differing folds are "a live behaviour bug". They are not. The folds do differ (JS
  full-Unicode `toLowerCase` vs Rust per-char, plus a length-mismatch fallback), but
  `basenameMatchPositions` returns `null` when it cannot reproduce the match and
  `rowHighlightPositions` is `… ?? positions`, so **ranking and order come entirely from the
  backend**. It is a cosmetic highlight divergence on non-ASCII queries, behind an explicit
  documented fallback — a LOW nit inside item 13, nothing more. No new ticket.

Scope control was the instruction and it was followed: seven real derivations, each red-proofed,
rather than sixteen shallow ones.

## Closed 2026-08-27 — what the gauntlet actually proved

Merged as PR #1067.

**The sweep that found the work was itself the first lesson.** CPE-1933's original pass ran `grep`
without `-i` and missed **57 candidates across 56 files** — one of them a *fourth* copy of the very
claim it was killing, in the same crate. Comments start sentences: "Mirrors…", "Must match…",
"Verbatim from…". A capital letter is not a hiding place.

**A commented-out decoy beat the scanner.** `rustSource.ts` leaked **142** `///` lines in
`src-tauri/src/lib.rs` and **31** in `fsutil.rs`, and a demonstrated commented-out decoy won against
the updater root-of-trust guard — the single highest-blast-radius derivation in the set. Anchoring on
prose rather than code is not a style preference here; it is how a guard passes while reading a
comment that quotes the old value. The stripper now enforces an invariant that makes any desync
loud: after stripping, **no line may begin with `//`**, or it throws.

**Three corrections landed during review**, all of the same species the ticket exists to eliminate: a
claim that reads as derived and is not. The gates found them by *running* the derivations against
changed sources rather than reading them.

**Merged past a known red.** GUI smoke shard 2 and its verdict job were failing on CPE-1960, verified
by reading job `98714294044` directly rather than trusting the job name.
