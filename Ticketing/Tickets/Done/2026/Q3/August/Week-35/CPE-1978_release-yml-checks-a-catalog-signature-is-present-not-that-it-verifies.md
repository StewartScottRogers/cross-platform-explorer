---
id: CPE-1978
title: `release.yml`'s "Verify the signed bundle" step checks a `.sig` is **present**, not that it **verifies** — and its own comment names CPE-1954 as the enabler
type: bug
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-28
---

## Summary

Found by CPE-1954's worker (PR #1088) while routing `catalog-sign verify` through `VerifiedIndex`.

`release.yml`'s **"Verify the signed bundle"** step checks only that a `.sig` file **exists**. It does
not run the verifier. **A step named "Verify" that asserts presence is the shape this repo has spent a
day burning down** — a name that reads as a guarantee over a check that is not the guarantee.

The step's **own comment names CPE-1954 as the enabler**, i.e. it was written knowing the real check
was unavailable and expecting someone to come back. **CPE-1954 has now landed**, so it is available.

## Why the worker deliberately left it

Stated plainly, and correctly: wiring the real verification needs **the public key in CI**, and it is
**release plumbing with its own blast radius**. Folding a secrets change and a release-workflow change
into a PR about a signing tool's read path would have been the scope creep this repo keeps refusing.

## What "verify" should mean here, now that it can

CPE-1954 established what `catalog-sign verify` actually checks, by **running the real binary** rather
than reading names:

| check | now enforced |
|---|---|
| index signature under the supplied key (`verify_index`) | yes |
| per-manifest signature (`trust::verify_signature`) | yes |
| sha256 content binding (`CatalogEntry::matches`) | yes |
| supported index schema (`CatalogIndex::is_supported`) | **added by CPE-1954** |
| `entry.id` is a single safe path component (`is_valid_entry_id`) | **added by CPE-1954** |

**Two things this subsystem does NOT have, so do not go looking for them** (the Foreman's brief for
CPE-1954 wrongly implied both, and the worker corrected it): there is **no filename binding** — these
are raw detached ed25519 signatures over exact bytes, with no trusted comment and no `file:<name>`;
that is the **updater's** minisign scheme, a **different subsystem**. And there is **no version floor**
in `verify` — anti-rollback lives in `apply_bundle_with`, and `verify` applies nothing.

## Acceptance criteria

- [ ] **Demonstrate the gap first.** Put a bundle in the release step's path whose signature does **not**
      verify, and show the step **passing**. Assert on the step's exit status, not on its log text.
- [ ] **Decide how the pubkey reaches CI**, and say what it costs. It is a **public** key, so committing
      it in-repo is defensible and avoids a secret — argue that against a repository secret rather than
      defaulting. Whatever you choose: **never commit a signing (private) key**, and do not touch the
      updater `pubkey`/`endpoints` in `tauri.conf.json`.
- [ ] **The step must fail closed.** A missing binary, an unreadable key, a bundle that is absent, or a
      verifier that cannot run must **red the release**, never pass. CLAUDE.md's rule — *distinguish
      "ran and found nothing" from "did not run"* — and this repo shipped **eight** violations of it in
      one day, one of them inside the guard written to prevent it.
- [ ] **Check whether `release-sidecar.yml` carries the same step**, and any other workflow that
      publishes a bundle. Enumerate at run time (CPE-1932) — the sibling workflow has diverged from
      `release.yml` before.
- [ ] **Red-proof it in CI, not only locally.** A workflow change nobody triggered is untested by
      construction (CPE-1933). If a full release run is unaffordable, say exactly how far you got and
      what remains unverified — **do not describe intended behaviour as observed**.
- [ ] Consider whether `crates/updater-verify`'s existing pattern applies: it **reads a workflow's argv
      at run time and executes the real binary with it**, which is this repo's answer to "a comment that
      claims what a workflow does." `release_workflow_wiring.rs` is the worked example.

## Work Log

**2026-08-28 — worked. What was MEASURED, and what was not.**

*The gap, on exit status.* The pre-fix `Verify the signed bundle before uploading it` body was run
against `catalog-out/catalog-index.json` + a `catalog-index.json.sig` holding the ASCII text
`not a signature`. It printed `signed catalog bundle carries 1 entr(y|ies); files to upload:` and
exited **0** — the job would have uploaded it. That is now a permanent executed case in
`src/lib/catalogPublishLoudFailure.test.ts` §8, asserting `r.status === 0`, not a log string.

*The same bundle through the real binary.* `catalog-sign verify catalog-out <trusted pubkey>` →
`FAIL: index signature does not verify under the key`, exit **1**.

*The new step, real `cargo`, real `catalog-sign`, body extracted from `release.yml` itself.* Four
scenarios, run locally against a bundle signed with a throwaway keypair (deleted):
A. good bundle + its key → exit **0** (`OK: index + 1 manifest(s) verify`, then the control run
   refused under the decoy).
B. same bundle, index `.sig` overwritten with `not a signature` → exit **1**.
C. good bundle, a key that did not sign it (a rotation nobody mirrored) → exit **1**.
D. `cargo` absent from `PATH` → exit **1** (`cargo: command not found`, then the step's `::error::`).

*The pubkey decision.* The key is a **literal in `release.yml`**, not a repository secret. It is the
public half; the identical value is already committed as `CATALOG_TRUSTED_KEYS` in
`src-tauri/src/lib.rs` and ships inside every installed binary, so a secret buys no confidentiality
and costs two things: a second copy no diff and no guard can see (a rotation could silently diverge
from what clients trust — the exact failure this check exists to catch), and an unset secret
expanding to the empty string, i.e. failing **open**. `catalogPublishLoudFailure.test.ts` §8 derives
both sides and reds on any divergence; red-proofed in both directions (workflow literal changed →
red; Rust const changed → red; both reverted). No signing key was generated in-repo, committed, or
touched, and `tauri.conf.json` is unmodified.

*Verifying under the CLIENTS' key, not the signing key,* is deliberate: verifying under the key that
just signed would only prove the bundle is self-consistent.

*Fail-closed, including "did not run".* `set -euo pipefail` covers a missing cargo, a build failure
and an unreadable bundle. The case an exit code cannot cover — a verifier that says yes to
everything — is covered by running the check **twice**, the second time under a key that did not
sign the bundle, requiring a refusal. Executed: a stub `cargo` that approves everything makes the
step exit non-zero.

*Enumeration (CPE-1932).* Derived from `allShellUnits()` over every workflow and extracted script,
not from a remembered pair of filenames. **`release-sidecar.yml` has no catalog job and no
`--bin *-sign` invocation** — nothing in the sign family there to diverge. (Round 1 wrote "signs
nothing", which is false and was corrected in review: that workflow does sign — Authenticode via
`cpe-sign.pfx` at `release-sidecar.yml:562`, plus tauri-action's updater signatures. Different
subsystems, with their own gates.) The sign-family invocations on this revision are
`release.yml → catalog-sign` (sign, and now verify) and **`model-snapshot.yml → model-snapshot-sign`
(sign only, no verification before publishing)**.

*Sibling gap found and NOT closed here — now **CPE-1981**.* `model-snapshot.yml` signs
`models-index.json` with the same key and publishes it to the `model-catalog` release with no
signature check at all — the identical shape. It is not closed in this PR because
`model-snapshot-sign` **has no CLI surface that could host a verify path**. Giving it one is a change
to a scheduled publishing workflow with its own blast radius.

**Round 2 (PR #1095 review, F1/F2) — the detector was measured backwards, and this is the round's
real finding.** Round 1 asked the narrow question "does the binary have a subcommand called
`verify`?" and its docblock claimed a missed spelling "fails toward reporting a gap that is already
closed — loud, not silent". **That is the wrong direction.** A `false` from that predicate *excuses*
the signer, so a missed spelling makes the guard **under-report silently**. Measured by the reviewer:
a real verify path spelled `args[1] == "check"` added to `model_snapshot_sign.rs`, workflow still
publishing unverified → **62 passed, 0 red**. Control, same sabotage spelled `"verify"` → **2 red**.

*The choice made:* **widen the detector so it fails closed**, rather than keep it and lean on the pin.
The pin cannot be stronger than the detector it calls — the `"check"` sabotage fooled the pin and the
sweep together, which is CPE-1950's shared blindness, not two legs. The question now has no verb in
it: `couldHostAVerifyPath` excuses a binary only when **all** of six clauses hold over
comment-stripped source — it reads argv *in this file* (F2: delegated parsing is no longer excused),
no string-literal comparison (`== "…"` plus the `starts_with(" / ends_with(" / contains(" / .eq(" /
eq_ignore_ascii_case("` method spellings and `matches!(`), no `"…" =>` match arm, no `--`-prefixed
literal, no arg-parser crate, and no `const`/`static … &str = "…"` declaration. The excuse is also no
longer allowed to be silent: a new test requires every excused signer to be named by a pin whose
title is read out of this test file's own source.

**Round 3 (review) — the fix shipped with a closed safety claim, and three real shapes falsified
it.** Round 2 wrote that a binary passing the clauses "structurally cannot host a verify path of any
spelling", and listed the remaining blind spots as only the no-CLI-surface family, "none reachable by
widening a regex". The reviewer broke both, each as a working verify dispatch on a live CLI surface
with the workflow still publishing unverified, each **0 red**: `args[1].starts_with("verif")`; a
`const VERIFY_CMD: &str = "verify"` compared with `args[1] == VERIFY_CMD`; and
`match args.len() { 2 => exit(0), _ => {} }`. **The const one is the damning one** — it is literally
the `==` dispatch the clause exists to catch, defeated by hoisting the literal, a refactor a reviewer
would routinely suggest.

This is CLAUDE.md's round-9 rule one scope in: *the blind-spot list is a claim of the same kind*, and
round 2's fix turned round 1's defect into a narrower version of itself. Fixed both ways — the claim
now states only what was measured (*no string-literal comparison, match arm, `--` flag, `matches!`,
string constant or parser crate is visible in the file that declares it*), and the clauses were
widened. Measured after: shape 1 → **2 red**, shape 2 → **2 red**, shape 3 → **0 red** (deliberately
open). Verified the tightening does not over-report: `model_snapshot_sign.rs` contains none of the
new patterns.

*Blind spots, now split by why, "at least these", no count.* **Not caught today but reachable** (a
regex or a resolution step away — a to-do list, not a boundary): argv-indexed branching with no
string anywhere (`match args.len()`; not closed because a general argv-arity clause would over-report
on this binary's own `args.len() != 4`); a token reaching the comparison indirectly via `format!`, a
`&[&str]` table, or a helper; and any comparison spelling nobody has written down yet. **Cannot be
caught by scanning at all:** a verify path with no CLI surface — selected by an environment variable,
by `argv[0]`, or by a build feature.

*Also round 3:* the pin-title scan's docblock claimed it was "anchored on `it(` so a title has to be a
real call" and that "nothing in this file does it today". Both false — the regex runs over raw source
and this file's own assertion message embeds `an it("${s.bin} has no CLI surface ...")`, which the
scan counted. It failed to self-satisfy the pin only because `${s.bin}` is literal text, i.e. the
guard was one "make this message concrete" edit from certifying its own excuse. Corrected the
sentences and added a `${` filter. `stripScriptBodiesChecked` is not used here and the reason is
stated at the site: this file is TypeScript, so its `vm.Script` oracle cannot compile it, and calling
`stripJsComments` bare would be that stripper with the leg that makes it trustworthy removed.

*What remains UNVERIFIED.* No release was cut and no workflow run was triggered, so the shipped step
has **never executed on a GitHub runner**. What was executed is the step's own `run:` body, extracted
from `release.yml` at run time, under bash — locally with the real `catalog-sign`, and in CI (vitest)
with a key-sensitive stub `cargo`. The one link that stays inferred is that the CI runner's
`CPE_CATALOG_SIGNING_KEY` secret is the private half of `CATALOG_TRUSTED_KEYS`; if it is not, the new
step fails the next release **loudly** — which is the intended behaviour, but it is a prediction, not
a measurement.

*Security scope.* This closes the publish-time **availability** half — a bundle every installed
client would reject can no longer publish green. It is **not** integrity protection against a
compromised signing key that is still the private half of `5b18…`.

*The negative control's strength is key-dependent (review F4), recorded at the site.* For today's key
the decoy `0b18…` is a valid ed25519 curve point, so `VerifyingKey::from_bytes` succeeds and the
refusal comes from `verify_strict` — the control really does exercise the signature path. That is a
property of this key: on a throwaway `d21f…` the decoy `021f…` is *not* a valid point, and the
refusal would come from key parsing instead. It fails closed either way (`trust.rs` returns `false`
rather than panicking), so the step is correct in both cases; the note at the workflow site says to
re-check on a rotation and switch to flipping a `.sig` byte if the new decoy is off-curve.

## Notes

Filed 2026-08-28 by the sprint Foreman from CPE-1954's worker (PR #1088), which found it while
enumerating the readers of `catalog-index.json` and left it deliberately, with its reason.

Related: **CPE-1954** (PR #1088 — the verifier this unblocks, and the definitive list of what it
checks), **CPE-1940** (`VerifiedIndex`, the fail-closed baseline), **CPE-1951** (the catalog's monotonic
version bound — the other half of release-time catalog correctness), **CPE-1933** (a claim about a
workflow that nobody executes is untested by construction), **CPE-1932** (enumerate, don't recall).

## Closing record — merged as PR #1095 (`16fd3282`), 2026-08-28

### The gap, demonstrated on exit status rather than argued

A `.sig` file containing the ASCII text `not a signature` made the step named **"Verify the signed bundle
before uploading it"** exit **0**, and the job would have uploaded the bundle. The same bundle through the
real binary: `FAIL: index signature does not verify under the key`, exit **1**.

A new step (`id: sigverify`) now runs `catalog-sign verify catalog-out "$CATALOG_PUBKEY"` **before** the
upload, and the terminal outcome gate reads its outcome.

### The pubkey decision — argued, not defaulted

The public half is a **literal in `release.yml`**, not a repository secret:

- It is public by construction — the identical value is already committed as `CATALOG_TRUSTED_KEYS` in
  `src-tauri/src/lib.rs` and ships in every installed binary, where it is fed to `VerifyingKey::from_bytes`.
- A secret buys **no confidentiality** and costs two things: a second copy **no diff or guard can see** (a
  rotation could silently diverge from what clients trust — the exact failure this step catches), and an
  **unset secret expands to `""`**, i.e. it fails **open**.
- Verification is under the **clients'** key deliberately. The signing key could only prove
  self-consistency; only the clients' key catches a rotated `CPE_CATALOG_SIGNING_KEY` nobody mirrored.

Both values were confirmed byte-identical, and the guard reds in **both** directions (proved by moving
each). No signing key was created in-repo or committed; `tauri.conf.json` untouched.

### Fail-closed, including the case an exit code cannot cover

`set -euo pipefail` covers missing cargo, build failure and an unreadable bundle; an unusable
`CATALOG_PUBKEY` is refused **before cargo is invoked at all**. The remaining case — **a verifier that says
yes to everything** — is covered by running the check a **second time under a key that did not sign the
bundle and requiring a refusal.**

**That control was audited harder than it was written.** The Reviewer computed ed25519 point decompression
and confirmed the decoy `0b18…` **is a valid curve point**, so `VerifyingKey::from_bytes` succeeds and the
refusal comes from `verify_strict` — the control genuinely exercises the signature path and cannot be
satisfied by the same trivial success as the positive run. `trust.rs:41-53` returns `false` rather than
panicking on an unparsable key, so it fails closed either way. Recorded at the site: this is
**key-dependent** — a future rotated key's decoy may be off-curve (measured on a throwaway `d21f…`, whose
decoy `021f…` is not a valid point), in which case the control no longer proves the signature path ran.
The note says to switch to flipping a `.sig` byte on rotation.

### What the three rounds actually cost, and what they bought

The code was right in round 1. **Every round after it was about the sentences around the code** — the shift's
dominant finding, in miniature:

- **Round 1** shipped a guard that failed **open**, with a docblock claiming the opposite direction: *"a
  verify path spelled another way reads as absent here. That fails toward reporting a gap that is already
  closed — loud, not silent — which is the safe direction."* Backwards. A `false` from the detector
  **excuses** the signer. Measured: a real verify path spelled `args[1] == "check"` gave **62 passed, 0
  red**; the same sabotage spelled `"verify"` gave **2 red**. The declared backstop worked for exactly one
  spelling.
- **Round 2** widened it — and correctly refused to lean on the pin, because **the pin calls the same
  detector**, so it is the excuse written down rather than a second opinion (CPE-1950 shared blindness).
  `hasVerifySubcommand` became **`couldHostAVerifyPath`**, a question with **no verb in it**: a binary is
  excused only when it reads argv in that file, has no `== "…"`, no match arm on a string, no
  `--`-prefixed literal and no arg-parser crate. But it shipped a **closed** claim — *"structurally cannot
  host a verify path of any spelling"* — and **three real shapes falsified it**, each a working dispatch on
  a live CLI surface: `starts_with("verif")`, a `match` on `args.len()`, and — the damning one — a `const`
  holding the literal compared with `==`, which is *literally* the comparison the clause exists to catch,
  **defeated by hoisting the literal into a `const`**, a refactor any reviewer would suggest.
- **Round 3** widened the clauses (method spellings, `matches!(`, and `const`/`static` string declarations
  — verified free, zero hits in the target file) **and demoted the claim to what was measured**. Blind spots
  split into **not caught today but reachable** (argv-arity branching; indirect tokens via
  `format!`/tables/helpers; any comparison spelling nobody has written down) vs **cannot be caught by
  scanning** (env var, `argv[0]`, build feature), written as **"at least these"**, no count. The
  `match args.len()` case was **re-measured and left open on purpose**: a general arity clause would
  over-report on the target binary's own legitimate arity guard, i.e. it would break the one binary the
  sweep excuses.

Final sabotage table, reproduced independently by the Reviewer:

| shape in `model_snapshot_sign.rs` | round 1 | final |
|---|---|---|
| `starts_with("verif")` | 0 red | **2 red** |
| `const` literal compared with `==` | 0 red | **2 red** |
| `match` on `args.len()` | 0 red | **0 red — deliberately open, measured** |

A second finding of the same family: the pin-title test's docblock claimed it was *"anchored on a real call,
not prose in a comment"* and that *"nothing in this file does it today"*. Both false — the regex over raw
source returns 57 entries, **one of them a non-call occurrence inside this test's own assertion message**. It
did not self-satisfy only because the string carried an interpolation rather than a literal name, **so the
guard was one "make this message concrete" edit from certifying its own excuse.** Fixed with an
interpolation filter and the residual stated.

### The sibling sweep — enumerated, and one half left open on purpose

Derived at run time via `allShellUnits()` over every workflow and script:

- **`release-sidecar.yml`** — no catalog job and no `--bin *-sign` invocation, so nothing in the sign family
  to diverge. (It **does** sign — Authenticode via `cpe-sign.pfx` at `:562`, plus tauri-action's updater
  signatures — which an earlier draft got wrong in all three places it appeared. Corrected, with the other
  subsystems and their gates named so the short form cannot be re-derived.)
- **`model-snapshot.yml`** — signs `models-index.json` with the same key and publishes it to the
  `model-catalog` release with **no verification at all**, not even the presence check. Left open because
  `model-snapshot-sign` has **no `verify` subcommand** — derived from the `[[bin]]` its own
  `--manifest-path` declares, comments stripped. Filed as **CPE-1981**, whose Foreman-written claim that
  "the guard reds the day a verify subcommand appears" was itself corrected the same day: it pins **one
  spelling** and fails open.

### Red-proofs, all reproduced by the Reviewer at their stated counts

Workflow literal moved → **1 red**, names both values. Rust const moved → **1 red**, same message. The real
verify invocation replaced by a shell **comment carrying identical text** → **6 red** — the CPE-1933 rule-2
proof that comment text is counted by nothing.

### Measured vs not — stated plainly

Locally, with real `cargo` and real `catalog-sign` over a throwaway-signed bundle, running the step body
**extracted from `release.yml` at run time** (and independently re-extracted by the Reviewer with its own
structural extractor): good bundle → **0**; corrupted `.sig` → **1**; wrong key → **1**; cargo absent → **1**.

**Not measured:** no release was cut and **no workflow run was triggered — the step has never executed on a
GitHub runner**; and that the runner's `CPE_CATALOG_SIGNING_KEY` is the private half of
`CATALOG_TRUSTED_KEYS` **remains inferred**.

**Operational note:** if the runner's signing key is not the private half of `5b18…`, the next release's
catalog job **goes red** rather than publishing a dead catalog. That is the correct direction.

**Security scope, so it is not misread later:** this closes the **publish-time availability** half — a bundle
clients would reject can no longer publish green. It is **not** integrity protection against a compromised
signing key that is still the private half of `5b18…`. `docs/security/threat-model.md` says so.

### Gates at merge

`npm run check` 0/0 · `npx vitest run` **359 files / 5,398 passed / 2 skipped** (the 2 are the pre-existing
jq-dependent skips, byte-identical on `origin/main`) · `cargo clippy --all-targets -D warnings` on
`sidecar/host` clean · **no Rust source changed** (verified from the diff), so the `src-tauri` two-mode
clippy was justifiably not re-run · CI `completed success — total_count=26 pending=0 skipped=1 coverage=ok`.

**Family:** CPE-1954 (PR #1088 — the verifier this unblocks, and what it actually checks, established by
running it), CPE-1981 (the `model-snapshot.yml` half), CPE-1940 (`VerifiedIndex`, the fail-closed baseline),
CPE-1951 (the catalog's monotonic version bound), CPE-1933 (derive provenance, don't claim it), CPE-1932
(enumerate, don't recall), CPE-1950 (a shared oracle catches divergence, not shared blindness).
