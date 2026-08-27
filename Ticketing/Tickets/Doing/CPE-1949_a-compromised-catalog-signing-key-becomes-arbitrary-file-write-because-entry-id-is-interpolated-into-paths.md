---
id: CPE-1949
title: a compromised catalog signing key becomes **arbitrary file write**, because `entry.id` is interpolated into five paths with no charset check
type: task
priority: Medium
status: In Progress
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Defence-in-depth finding from PR #1058's Security Auditor, raised while confirming that PR closed the
pre-verification traversal (CPE-1940 F-B) correctly.

After `VerifiedIndex` proved the index signature, `entry.id` is still interpolated into paths with no
validation: `write_entry`, plus four `staging.join(format!("{id}…"))` sites.

    signing key compromised, id = "../../.."  ->  arbitrary file write anywhere the app can reach

**Exploiting it requires the catalog signing key**, and the auditor was explicit that this makes it a
hardening item rather than a live hole — with that key an attacker can already ship an arbitrary
malicious agent through every gate, so the traversal adds *where*, not *whether*. It also judged
CPE-1940's decision to scope it out as **the right call**: fixing it inside that diff would have meant
adding a sanitiser to the path that PR had just proved does not need one, muddying a clean reordering.

## Why it is still worth doing

The blast radius is meaningfully different:

    without this: key compromise -> install a malicious agent
    with this:    key compromise -> arbitrary file write anywhere the app can reach

That is the difference between a bad agent and a compromised machine, and the mitigation the auditor
proposed is one cheap check at one place.

## Acceptance criteria

- [ ] Validate `entry.id` against a strict charset at **`VerifiedIndex::open`** — the auditor's
      suggestion is `[A-Za-z0-9._-]+`, rejecting `.` and `..` outright. One place, so it cannot be
      forgotten at a call site, and it sits where every consumer already funnels through.
- [ ] **Reject, do not sanitise.** A rejected id is a refusal the publisher can see and fix; a
      sanitised one silently writes to a path nobody chose. Refuse the whole index rather than
      dropping one entry, unless there is a reason not to — say which and why.
- [ ] **Demonstrate before and after.** With a key you control, publish an index with
      `id = "../../pwned"`, show the write landing outside the catalog dir, then show it refused.
      Assert on **the filesystem** — that the escaped location does not exist — not on a verdict.
- [ ] **Do not weaken CPE-1940's ordering.** The check must run inside `VerifiedIndex::open`, after
      signature verification, not become a new pre-verification parse.
- [ ] Confirm every real published `entry.id` passes. Read them off the live catalog, not off the
      schema (PR #1053 found this repo's assumptions about published artifact names were wrong twice).
- [ ] Red-proof: disable the check and confirm the traversal test reddens on the harm assertion.

## Also from the same audit — decide, do not silently inherit

Three measured residuals the auditor recorded without blocking. Each needs a recorded decision, not
necessarily a change:

1. **The absent-map route survives.** CPE-1940 made a *damaged* `versions.json` fail closed; **deleting**
   it still yields `applied=["claude"]` with the ancient payload written and the map rewritten to
   `{"claude":1}` — measured. Absent ⇒ first run is intentional and documented. Severity is genuinely
   low: `agents.rs:301` never consults `versions.json`, so a local attacker who can delete it can
   equally drop an old signed manifest+`.sig` straight into the catalog dir. **Decide whether the
   baseline should be anchored to something that cannot simply be removed**, and record the answer.
   Note PR #1058's body reads broader than what shipped — it closes damage, not deletion.
2. **`apply_bundle` / `apply_bundle_with` remain `pub` with no production callers**, and the latter
   still takes `&mut VersionMap` — so a future caller could reintroduce the fail-open with
   `load_versions(..).unwrap_or_default()`. Nothing guards that. Narrow the visibility, or pin it.
3. **The staging dir is `temp_dir()/cpe-catalog-stage-<pid>`** — predictable, outside the project, and
   `create_dir_all` succeeds onto a pre-existing junction. Pre-existing, untouched by #1058, and
   adjacent to the whole CPE-1896/CPE-1913 containment family.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1058's Security Auditor (SEC PASS), which enumerated
every route to `applied` and to disk, probed **31** hostile `versions.json` shapes, and compile-tested
four `VerifiedIndex` bypass attempts before raising these.

Related: **CPE-1940** (the reordering, PR #1058), **CPE-1924** (the single-comparison design),
**CPE-1941** (the publish-side route to stale content), **CPE-1896** / **CPE-1913** (the containment
family the staging-dir item belongs to).

## Work Log

**2026-08-27** — moved to `Doing/`. Branched off `cpe-1940-catalog-fail-closed`, because PR #1058
was still open at start and at push; basing on it meant `VerifiedIndex` existed to extend rather
than being reimplemented.

**2026-08-27, later — rebased onto `main` after #1058 merged.** Recorded because it is the riskiest
step in this ticket and an unrecorded rebase is how a reviewer and a worker end up disagreeing about
what a file contains. #1058 **squash-merged**, so a plain `git rebase origin/main` tried to replay
its two commits against content already in `main` and conflicted in four files. Those commits were
not resolved, they were **dropped** — they had become the squash:

    git rebase --onto origin/main 8727d190     # 8727d190 = the old base, #1058's head

Result: one commit, five files, all CPE-1949's, +542/-91 (down from the +1318/-221 the stale base
was showing). Straight after that rebase the merge base of the branch and `origin/main` was
`6312b87b`, #1058's squash commit — the clean statement of why nothing was lost: the two commits
were not deleted, they were replaced by the squash they had become. (That is a *moving* fact, not a
standing one: a second rebase onto a newer `main` moved the merge base to `3d4276f8`. `6312b87b`
remains an ancestor of HEAD, checked with `git merge-base --is-ancestor`, which is the durable
form of the same claim.) Re-verified afterwards, because a
rebase is exactly where these get reordered: the id check still sits at the **bottom** of
`VerifiedIndex::open` (verify → utf8 → parse → schema → id), and the traversal red-proof was re-run
on the rebased tree on **both** Windows and Linux, both failing on the harm assertion. Rebased onto
`main` a second time before the review fixes below; no conflicts.

### The check

`is_valid_entry_id` + `MAX_ENTRY_ID_LEN = 64` in `sidecar/host/src/catalog.rs`, enforced in exactly
one place: the **bottom** of `VerifiedIndex::open`, after verify → utf8 → parse → schema. Nothing
moved above the signature check, so CPE-1940's ordering is intact. The whole index is refused, not
the offending entry — a refusal is something the publisher sees, and filtering entries would make
`VerifiedIndex::index()` return a document nobody signed, which is the newtype's entire invariant.

`sign_bundle` refuses the same shape, so a mistyped manifest id fails the release build instead of
shipping an index every client rejects (a silent catalog outage).

### Before / after, measured on the filesystem

Staging `root/stage`, catalog dir `root/nest/out`, `id = "../pwned"`, index signed with a key the
test owns (the key-compromise case, not a forgery).

    before  write_entry(out, "../pwned", …)  ->  root/nest/pwned.json EXISTS   (outside the catalog dir)
    after   apply_bundle_at(…) on the same signed bytes
            root/nest/pwned.json             ->  does not exist
            root/pwned.json (staged payload) ->  byte-identical, untouched
            read_dir(out).count()            ->  0
            report.index_ok = false, applied = []

The `exists()` assertions run **before** any assertion about the report, so the regression reddens on
the escaped file rather than on a missing flag.

### Red-proof

`if !index.entries.iter().all(…)` → `if false`. Windows **and** Linux both fail on the harm
assertion, not a verdict:

    a signed index escaped the catalog dir: …\nest\pwned.json was written    (Windows)
    a signed index escaped the catalog dir: /tmp/.tmpcVh9jQ/nest/pwned.json was written  (Linux)

Second sabotage: `pub(crate) fn apply_bundle_with` → `pub fn` reddens
`the_mut_version_map_entry_points_stay_shut`.

### Real ids — read off the live catalog, not the schema

Downloaded the `catalog-index.json` asset from **all 65 releases that publish one** and extracted
every `entries[].id`: 780 rows, **12 distinct** — `aider claude codebuff codex gemini grok mistral
opencode pi qwen tau vtcode`. Identical to the 12 `id` fields in `sidecar/ai-console/agents/`, which
is what `catalog-sign` publishes. All 2–8 chars of `[a-z]`; every one passes, pinned by
`every_id_this_repo_has_ever_published_passes_the_charset` both as a predicate and through a real
signed `VerifiedIndex::open`.

Incidental: no release after **v0.57.32** carries a `catalog-index.json` at all (the signing step is
gated on the key secret). Consistent with CPE-1893's freshness backstop; not touched here.

### Residual decisions

1. **Absent `versions.json` — no change, and now measured + pinned.**
   `a_deleted_version_map_is_a_first_run_and_the_map_is_not_the_weakest_link` reproduces the route
   (delete the map ⇒ ancient signed bundle applies, map rewritten to `{"claude":1}`) and then shows
   why anchoring it elsewhere would not help: `agents.rs` loads `<id>.json` + `.sig` on
   `verify_manifest` alone and **never opens `versions.json`**, so an attacker with write access to
   the catalog dir plants the old signed manifest directly. An old first-party signature stays valid
   forever and no local baseline can revoke it — the real remedy is signature expiry/revocation, a
   different and much larger design. Anchoring the baseline would lock a door that is already open.
2. **`pub(crate)` visibility — landed, and now pinned.** Verified on `main`+#1058:
   `apply_bundle_with` is `pub(crate)`, `apply_bundle` is a test-module helper, neither has any
   caller outside `catalog.rs`. Nothing guarded that, so `the_mut_version_map_entry_points_stay_shut`
   now does, plus a runtime source walk (with a not-working floor) for the
   `load_versions(..).unwrap_or_default()` pairing. `save_versions` staying `pub` is **fine**: it
   cannot manufacture the fail-open *read*, and anything that can call it can call `std::fs::write`
   on the same path.
3. **Staging dir — out of scope, wants its own ticket** (reported to the Foreman, not filed here).
   `temp_dir()/cpe-catalog-stage-<pid>` is a different attacker model (local FS write, no key) and
   the remedy is containment machinery, not a charset check — folding it in would muddy this diff
   exactly as CPE-1940 correctly declined to fold in this one. It is also more work than the brief
   assumed: `open_beneath` is `pub(crate)` **inside `cpe-server`**, so `src-tauri`'s
   `do_fetch_catalog` cannot reach it at all, and **`remove_file_beneath` does not exist anywhere in
   the repo** (grepped `--include=*.rs --include=*.md`, zero hits).

### Review round — four fixes after APPROVE + SEC PASS

1. **The one branch this PR rewrites had no test.** `launcher.html`'s final `else` (`indexOk:false`
   with no `error`, no `offline`, no `versionMapUnreadable`) was the only branch in that chain
   without a case, while every sibling got one in CPE-1911/1924/1940. Added
   `an index refused as a whole is amber and blames no specific cause — in particular, not the
   signature`, which pins the **absence** as hard as the presence (no `/signature/i`, `/signed/i` or
   `/schema/i`) and checks it did not fall through to a sibling. Red-proofed by restoring the old
   copy: exactly 1 of 84 fails, on `expected 'The catalog couldn't be verified, so…' to match
   /published catalog was refused/i`.
2. **The code comment's cause list was one short** — a signed but non-UTF-8 or unparseable index is
   a fourth reachable cause. Rewritten as an enumerated list read off `VerifiedIndex::open` rather
   than recalled: **five failure exits, four distinct causes** (the parse step has two), counted with
   a grep rather than by eye, since the first draft of the fix said "four early returns" and that was
   itself wrong.
3. **The docs over-claimed.** *"rejected before any part of it was read, so nothing was fetched or
   written at all"* is false: `do_fetch_catalog` fetches the index and its `.sig` and writes both
   into a staging temp dir **before** `VerifiedIndex::open` (`lib.rs:10317–10320`), discarding them
   on refusal. Rewritten to say what is actually true — the listing is downloaded, and nothing that
   follows from *trusting* it happens — and the fourth cause added there too.
4. **This Work Log was stale about the rebase**, the riskiest step here; the entry above now records
   it, including the `--onto` and why the two commits were dropped rather than resolved.

**On the new guard's own limits** (raised in review, agreed, and *not* fixed here — they belong in
the follow-up): the `load_versions(` + `.unwrap_or_default()` sweep matches **per line**, so a
rustfmt-wrapped two-line pairing slips past it, and the `files.len() >= 5` floor is loose against the
23 `.rs` files actually in `sidecar/host/src`. Both still catch the failure mode they were written
for — a single-line reintroduction, and a walk that returns nothing at all — so the guard is not
false comfort, just narrower than its name suggests. Widening it means a brace-matching or
multi-line scan, which is a different kind of change from a one-charset-check diff.

### Verification

`sidecar/host` 119 lib tests + clippy `--locked --all-targets -D warnings` clean; `sidecar/ai-console`
386 + clippy clean; `src-tauri` 285 + clippy clean **both** with and without `sidecar-platform`;
`npm run check` 0/0; launcher + `sectionDocs` + `ratchetBaselines` 152 pass.
**Real Linux run** via `~/lintools` in WSL: 118 lib tests pass and clippy clean, with all seven new
tests confirmed executing there by name. That toolchain could not build `sidecar-host` before today
(`libdbus-sys`); it now can — `pkgconf` + `libpkgconf7` + `libdbus-1-dev` + `libsystemd-dev` were
unpacked into `~/lintools/sysroot` by the same rootless `dpkg-deb -x` technique, and
`~/lintools/bin/pkg-config` was fixed to set `PKG_CONFIG_SYSROOT_DIR`. Left in place for the crew.

No dependency changes, no `specta::Type` changes (no bindings regen), no new ratchet, no new docs
section (so no `sectionDocs.ts` change), no `tauri.conf.json` changes.
