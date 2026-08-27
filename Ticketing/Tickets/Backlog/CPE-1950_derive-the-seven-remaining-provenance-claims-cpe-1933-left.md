---
id: CPE-1950
title: derive the seven remaining cross-file provenance claims CPE-1933 classified but did not derive
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
and decays silently. The enumeration found **12 hard hits**. CPE-1933 derived the two highest
blast-radius ones (release plumbing, and the keyboard registry) and deliberately stopped there
rather than doing all twelve shallowly.

These seven are the remainder. They are all **classified and understood** — none is a mystery, and
each already has a comment at the site. What they lack is a runtime derivation.

## The remainder, in blast-radius order

1. **`src/lib/components/RepoBrowser.svelte:2`** — MED, security-adjacent. `PROVIDER_HOSTS`
   (`github/github.com`, `gitlab/gitlab.com`, `bitbucket/bitbucket.org`, `codeberg/codeberg.org`) is
   claimed to mirror `clone_host()` in `src-tauri/src/lib.rs:11965`. Nothing checks it. These host
   strings also build the lookalike-host anchoring regexes, so a backend-side provider addition
   silently stops `stripRepoUrl` stripping its host and a malformed repo id reaches `forge_browse`.

2. **`src/lib/replayFold.test.ts:7`** — MED. Claims to "port the Rust reconstruction tests
   (`crates/server/src/replay.rs` + `replay_view.rs`) **verbatim as the ORACLE** … so the TS fold and
   the Rust fold **provably agree**". It reads nothing at runtime — the oracle is a hand-copy. Change
   a Rust reconstruction rule and each suite goes green on a different behaviour.

3. **`src/lib/batchMedia.ts:227,236`** — LOW-MED. `lexical_normalize` and the `fold_case` platform
   gate reimplemented in TS from `crates/server/src/batch_media.rs`. This is the collision preview for
   a **destructive** batch operation: if normalisation drifts, the dialog predicts a different
   collision set than the backend executes and the user gets a silent overwrite.

4. **`crates/s3/src/provider.rs:544`** (+ `crates/s3/Cargo.toml:50`) — LOW-MED. `xml_nesting_too_deep`
   ported "near-verbatim" from `crates/webdav/src/lib.rs:433`. Two independent copies of a parser DoS
   guard with no shared crate. The *known* bypass battery is ported to both sides, so this is about
   **future** hardening not propagating. Consider extracting the guard rather than deriving it.

5. **`src/lib/entrySearch.ts:159-163,205`** — LOW. `MAX_ABS_YEAR = 99_999` and the `days_from_civil`
   port duplicated from `crates/server/src/date_filter.rs:167` with only a comment holding them
   together. Frontend and backend could disagree on which files a `before:`/`after:` token selects.

6. **`gui-smoke/specs/vault-create.smoke.ts:27`** and **`trash-titlebar.smoke.ts:39`** — LOW. Fixture
   name/passphrase literals duplicated across the runner/worker boundary from `wdio.conf.ts`, which
   already **exports** most of them (`SHRED_DIR_NAME`, `VAULT_CREATE_PARENT_DIR`,
   `VAULT_CREATE_BLOB_NAME`, `TRASH_TITLEBAR_FILE_NAME`). Drift surfaces as a red spec rather than a
   silent pass — except that `trash-titlebar` has a documented skip path, giving drift a route to
   "skipped", and the known-failing ratchet can hold such a red as an exemption. Cheapest fix in the
   list: import the exported constants instead of restating them.

7. **`scripts/dev-harness/revert-heldback-copy/main.ts:33`** — LOW. Two long user-facing strings
   "lifted from `revert_engine.rs`'s own branch … this is the actual wire text", character for
   character including the curly apostrophe. Once the Rust copy is edited the harness reviews text
   the user will never see while still claiming it is the wire text. The harness exists *specifically*
   so a human reviews real wire copy, so the claim is the whole point of the file.

## Acceptance criteria

- [ ] For each of the seven: **derive** it (read the referenced source at run time and assert), or
      **delete the claim**, or mark it **genuinely unavoidable at the site** with the reason.
- [ ] Red-proof every derivation — change the referenced source, watch the test fail. A derivation
      that does not actually re-read its source is the same defect with extra steps.
- [ ] Anchor scanners on **code, not prose**. A comment quoting the old value must not be parsed as
      the real thing (see CPE-1933's `code_lines` in `release_workflow_wiring.rs`, and the hole
      PR #1056's Reviewer found in `MacroRunConfirm.test.ts`).
- [ ] #6 needs no new machinery — import the constants `wdio.conf.ts` already exports.
- [ ] #4 may be better solved by **extracting** the shared guard than by deriving a copy of it.

## Notes

Filed 2026-08-27 by CPE-1933's worker under that ticket's own scope-control clause ("if the
enumeration turns up more than you can properly derive, do the high-blast-radius ones and file the
rest"). The standard these follow is in `CLAUDE.md` → **Guards and ratchets** → *"Derive provenance,
don't claim it"*.

The sweep also found **108 SOFT** hits — comments describing design kinship with no drift-able value
("mirrors `loadPath`'s own HOME short-circuit") — which are **not** defects and need no action, and
**4 already-derived** sites. One site, `gui-smoke/lib/samplesNav.ts:44`, records in place that its own
old byte-identity claim was deliberately broken by CPE-1679; it is the model for handling provenance
decay correctly and is worth citing.

Related: **CPE-1933** (the sweep), **CPE-1917** (built the deriving pattern), **CPE-1872** (where the
stale claims lived), **CPE-1929** (shadowed guards), **CPE-1932** (rules from memory).
