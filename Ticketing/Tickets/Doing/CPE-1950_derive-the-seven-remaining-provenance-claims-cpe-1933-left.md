---
id: CPE-1950
title: derive the remaining cross-file provenance claims CPE-1933 classified but did not derive
type: task
priority: Medium
status: Open
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
