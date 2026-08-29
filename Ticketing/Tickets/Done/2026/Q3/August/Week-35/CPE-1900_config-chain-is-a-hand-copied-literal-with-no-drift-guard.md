---
id: CPE-1900
title: CONFIG_CHAIN is a hand-copied literal with nothing tying it to release-sidecar.yml's real --config args
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

`src/lib/sidecarBundleResources.test.ts` guards the shipped app's merged configuration by walking a
`CONFIG_CHAIN` literal — the list of `--config` overlay files `release-sidecar.yml` layers on top of
`tauri.conf.json`. CPE-1873 leaned on that machinery to pin the updater pubkey and endpoints against
an overlay override, and it works: an attacker key injected into any file in the chain now reds the
suite on all three shipped OSes.

**But `CONFIG_CHAIN` is a hand-maintained copy.** `grep -rn CONFIG_CHAIN` finds only the test itself
and a prose comment asking the reader to "keep this in lockstep" with the workflow. Nothing derives it
from, or checks it against, `release-sidecar.yml`'s actual `args:` line.

So adding a fifth `--config` overlay to the workflow **silently narrows the guard while leaving it
green**. The new overlay ships, is merged into the config the user's app runs on, and no test knows it
exists. A convention is not a mechanism.

This is the same root cause as the bug found in CPE-1873's second audit round: the guard's model of
what ships is a **copy** rather than a **derivation**, so anything that changes what ships without
changing the copy escapes it.

## Acceptance criteria

- [ ] Derive the overlay list from `release-sidecar.yml` itself, or assert that the literal matches the
      workflow's real `--config` arguments. Derivation is better; an assertion that fails loudly on
      drift is acceptable.
- [ ] Red-proof it: add a sixth overlay to the workflow without updating the test and confirm the
      suite goes red naming the missing file. Then add it to both and confirm green. Restore.
- [ ] Cover the ordering too, not just membership. RFC 7396 merge is order-dependent — a chain with the
      right files in the wrong order computes a different merged config than the one that ships, and a
      membership-only check would call that correct.
- [ ] Do not regress CPE-1873's pins while restructuring this. The updater pubkey and endpoint
      assertions over the merged config must still red on an injected attacker key in any chain file,
      on all three shipped OSes — re-run that injection after your change.

## Notes

Filed 2026-08-26 by CPE-1873's independent Security Auditor, which flagged it as a note alongside a
proven bypass rather than a merge block. See also **CPE-1873** (the pin), **CPE-1901** (the
`--skip-pin-check` kill switch found in the same pass).

Whoever picks this up should read the auditor's related finding first: Tauri **also** merges
`tauri.<platform>.conf.json` automatically, with no `--config` flag at all, via
`tauri-utils::config::parse::read_from`. That file class is not — and cannot be — in the workflow's
`args:` line, so a derivation from the workflow alone is still incomplete. The correct model of "what
config does the shipped app actually run on" is the workflow's overlays **plus** Tauri's own automatic
per-platform merge.

## Work Log

**2026-08-28 — PR #1105 open, round 2 addressed.** Not merged, so not Done; the Foreman moves this at
merge. `CONFIG_CHAIN` deleted; the chain is now derived, and the auto-merged half is enumerated and
folded into the same merged config.

### What shipped

- **`src/lib/tauriConfigChain.ts` (new)** — derives every `tauri-action` build leg in the repo:
  enumerates `.github/workflows/`, finds every step that `uses:` `tauri-apps/tauri-action`, expands
  its job matrix, resolves `${{ matrix.* }}` in `runs-on` and `args:`, and reads the `--config`
  overlays out of the resolved `args:` **in order**. 6 legs today (`release.yml` x3 +
  `release-sidecar.yml` x3), with floors that refuse a near-empty discovery (CPE-1932).
  - Parsed **structurally**, via the app's own YAML parser (`parseWorkflowFile`), so every `#` comment
    is gone before a string is read — CPE-1933 rule 2 satisfied by construction, no hand-rolled
    stripper. Only the VALUES of `with.args` and `runs-on` are ever scanned.
  - Refuses rather than guesses: an unresolved `${{ … }}`, a product-axis/exclude matrix, a quoted
    `args:`, a dangling `--config`, an unclassifiable runner label, an overlay outside `src-tauri/`.
  - Understands `-c` as well as `--config` (attached and separate forms). A `--config`-only scanner
    would read a workflow switched to `-c` as having no overlays at all — this ticket's own defect.
- **`src/lib/sidecarBundleResources.test.ts`** — `mergedConfig` now takes a derived leg, and
  `configChainForLeg` states **which half is derived and which is enumerated**: the `--config`
  overlays are read out of the workflow; the auto-merged `tauri.<platform>.conf.*` **cannot be**
  (there is no flag to read) and is a directory listing classified by shape, now folded into the
  merged config rather than only refused. Also stated: neither half can see a runner-side patch of
  `tauri.conf.json` or a `TAURI_CONFIG` env var.
- The CPE-1873 updater pin now runs over **every derived leg of both channels**, not the three
  hand-listed sidecar OSes.
- **`src/lib/tauriConfigChain.test.ts` (new)** — the producer's own tests, including a YAML fixture
  with four planted `--config` decoys (whole-line comment, trailing comment, `run:` heredoc body, a
  comment inside `with:`).
- **`.github/workflows/release-sidecar.yml`** — the `verify-updater-pin` job's vitest step named ONE
  test file, which would have run the merged-config assertions and silently skipped the derivation
  they now depend on. Changed to `npm test`, the same fix CPE-1903 applied to the `cargo test` step
  in the same job, for the same reason.

### Measured, not asserted (all local, 2026-08-28; every sabotage reverted, tree clean after each)

| red-proof | result |
|---|---|
| 4th `--config` overlay added to the workflow, file not committed | **13 failed / 49 passed of 62**, naming the missing file |
| same, file present with an attacker `plugins.updater` | **6 failed / 56 passed** — 3 shipped OSes x {pubkey, endpoints} |
| …and the OLD hand-copied guard under that same sabotage | **20 passed / 20, exit 0** — the bug, measured |
| `configOverlaysFromArgs` returns `[...out].sort()` | **3 failed / 38 passed**, ordering leg only; every membership assertion stayed GREEN |
| …`[...out].reverse()` | identical, + 3 in `tauriConfigChain.test.ts` |
| attacker key injected into each of the 7 chain files in turn | all red, each reding exactly the shipped OSes that file governs (base 12/29, `sidecar.conf` 6/35, `unix` 4/37, the four per-OS files 2/39 each) |
| `tauri.windows.conf.json` with an attacker updater block | **5 failed / 36 passed** — the merged-config pin now fires on the auto-merged half, not only the CPE-1903 refusal |
| `Tauri.linux.toml` (unparseable format) / two files for one platform | **8 failed / 33 passed** each — refused, not skipped |
| naive `/--config\s+(\S+)/` line scan over the decoy fixture | **4 of 4 decoys** read as live overlays; with a whole-line comment filter still **2 of 4** |

**Ordering is load-bearing, and that was measured too.** Of the 24 orderings of each sidecar leg's
4-file chain, **16 (windows) / 12 (linux) / 12 (macos)** compute a genuinely different merged config.
All 6 base-first orderings agree, because today's three overlays write disjoint keys and commute — so
what carries the order right now is the base config's position. The guard still pins the full
sequence, because commutativity is a property of today's three files and the first overlay that
collides with another would break it silently. A plain `JSON.stringify` oracle would have reported 21
of 24 "different" on **every** leg; **5 (windows) / 9 (linux) / 9 (macos)** of those differ only in key
insertion order, so the comparison is canonicalised.

### Verification

`npm run check` 0 errors / 0 warnings. Full vitest **363 files / 5486 passed, 2 skipped**.
`cargo test --locked` in `crates/updater-verify` **147 passed / 0 failed** (doc comments there were
updated to name the derivation instead of the deleted literal).

### Not measured / out of scope

- Nothing was run on GitHub Actions; no workflow was dispatched, no release touched. The workflow's
  `npm test` step is argued from `ci.yml`'s `frontend` job (same command, same runner, n=83, max 5.9
  min) — **the release-sidecar job's own wall-clock with it is unmeasured**, and that job's
  `timeout-minutes: 30` was already flagged in-file as an unmeasured analogue.
- No build was run; this is a static-config guard and asserts nothing about `tauri build`'s behaviour
  beyond the RFC 7396 merge semantics the file already modelled.
- CPE-1901 (`--skip-pin-check`) untouched.

### Round 2 — review findings addressed (2026-08-28)

`SEC PASS` + `CHANGES REQUESTED`. Every headline number was independently reproduced by the Reviewer,
including the widening: an attacker `plugins.updater` in `tauri.windows.conf.json` with **no workflow
edit at all** is **5 failed / 36** on the new guard against **1 failed / 19** on the base guard (only
the CPE-1903 refusal) — the merged-config pin genuinely could not see the auto-merged half before.
Sixteen adversarial shapes were thrown at the derivation; all sixteen behaved as documented.

- **F1 (required) — claim scope.** *"A third release channel added tomorrow is guarded on the day it
  lands"* was true only of a channel using `uses: tauri-apps/tauri-action` with overlays in
  `with.args`. Measured to yield **zero legs, silently**: a bare `run: npx tauri build --config …`, a
  local composite action, a reusable-workflow call, and tauri-action's own `tauriScript:` input. The
  floors only catch shrinkage. Both blind-spot lists were also **closed lists**; CLAUDE.md's rule is
  *"at least these"*. Rewritten in both places, with the honest good news kept: a third *tauri-action*
  channel **does** red, because `"the derived leg set covers both release channels"` pins the workflow
  list with `toEqual`.
- **F2 (required) — the ticket's own defect, in the file I edited.** The `verify-updater-pin` job
  header still described check 2 as `vitest run src/lib/sidecarBundleResources.test.ts`, 42 lines above
  the step I had changed to `npm test`. Fixed, and it now points at the step for the reasoning.
- **F3 (code) — `..` traversal.** `assertOverlaysLiveInProjectDir` used a bare
  `startsWith("src-tauri/")`, a string test, so `src-tauri/../../planted.conf.json` was accepted. Mild
  in consequence (the file is still loaded, merged and pinned, so the updater assertions fire) but the
  refusal was reporting clean on the input its own doc comment named. A `..` SEGMENT is now rejected
  before the prefix test. Red-proofed: filter forced to match nothing → **1 failed / 23**. The
  complement is pinned too — `tauri..odd.conf.json` is not a traversal and is still accepted.
- **F4 — a per-leg number quoted without its leg.** *"21 of 24 … of which 5 differed only in key
  order"* was the **windows** leg, stated in the paragraph that exists to correct an oracle's scope.
  Re-measured, reviewer's figures confirmed exactly: `JSON.stringify` says 21 on all three legs;
  canonical says **16 / 12 / 12**; key-order-only **5 / 9 / 9**. All three now given, in all three
  places, with the near-miss recorded in CLAUDE.md as its own rule.
- **F5 — the scanners are now committed.** The 4-of-4 / 2-of-4 / 0-of-4 numbers were prose over a
  deleted scratch file. `NAIVE_SCANNERS` and a test now compute all three every run, and the
  whole-line-filtered survivors are asserted **by name** rather than counted. Red-proofed: deleting the
  trailing decoy from the fixture — the most plausible tidy-up — is **1 failed / 23**, where before it
  was silent and would have left a tautology behind.
- **F6 — nothing counted overlays.** Dropping the FIRST overlay was 40/41, and the single red was the
  order-vacuity test catching it only incidentally. A direct count against `leg.args` (regex sweep, not
  another token walk) now makes it **4 failed / 43**, three of them this assertion, one per sidecar
  leg, each printing both counts and both lists.
- **Workflow change accepted** with the reasoning verified. Reviewer's non-required note: composing the
  two measured analogues (msrv 17.1 + frontend 5.9, x1.5) gives **35**, not the job's current
  `timeout-minutes: 30`. **Left alone deliberately** — that header is CPE-1967's, it already declares
  itself an unmeasured analogue, and the realistic worst case is well under 15 minutes. Re-sizing it
  belongs in a ticket that can measure a real run, not in this one.

Adjacent finding at `sidecarBundleResources.test.ts`'s *"Keep these two literals in lockstep with …
pinned_pubkey.rs"* is being **filed separately by the Reviewer** and was deliberately not touched here.

**Round 2 verification.** `npm run check` 0 errors / 0 warnings. Full vitest **364 files / 5528 passed,
62 skipped**. `cargo test --locked` in `crates/updater-verify` **147 passed / 0 failed**. All three new
legs red-proofed and reverted; `git status --porcelain` clean.

## Closing record — merged as PR #1105 (`5cf4e61d`), 2026-08-28

### The bug, measured rather than argued

`CONFIG_CHAIN` was a hand-maintained copy of the `--config` overlays the release workflow layers on
`tauri.conf.json`, with **nothing tying it to the workflow**. So adding an overlay silently narrowed the
guard while leaving it green.

**The headline, established by running the unmodified base guard under the sabotage** (`git show
56e0d3a7:src/lib/sidecarBundleResources.test.ts`), with an attacker-key overlay appended to
`release-sidecar.yml`'s **real** `args:` line and the overlay file present carrying an attacker
`plugins.updater`:

> **20 passed / 20, exit 0.**

The new guard on the identical input: **6 failed / 56** — exactly 3 OSes × {pubkey, endpoints}.
Independently reproduced by the Reviewer.

### What shipped

`src/lib/tauriConfigChain.ts` — the literal is deleted. Every workflow in `.github/workflows/` is
enumerated, every step that `uses:` `tauri-apps/tauri-action` is a build leg, its **job matrix is
expanded**, `${{ matrix.* }}` resolved in `runs-on` and `args:`, and the overlays read out of the resolved
`args:` **in order**. **Six legs today** (`release.yml` ×3 + `release-sidecar.yml` ×3), so **the plain
channel now gets the updater pin too**.

- Parsed **structurally** via the app's own YAML parser — comments are gone before a string is read
  (CPE-1933 rule 2), and only the **values** of `with.args` and `runs-on` are scanned.
- **Understands `-c` as well as `--config`.** A `--config`-only scanner would read a workflow switched to
  the short form as having **no overlays at all**.
- **Refuses rather than guesses** on unresolved `${{ … }}`, product-axis/`exclude:` matrices, quoted
  `args:`, dangling flags, unclassifiable runner labels, and overlays outside `src-tauri/`, with
  near-empty discovery floors (CPE-1932).

**The second half — the one the ticket warned a workflow-only derivation would miss.** `configChainForLeg`
composes **DERIVED** (workflow overlays) **+ ENUMERATED** (auto-merged `tauri.<platform>.conf.*`, which
**no `args:` line can express**). Those files now enter the merged config, **so the pins see them;
previously CPE-1903 only *refused* them.** Measured: an attacker `plugins.updater` in
`tauri.windows.conf.json` **with no workflow edit at all** gives the new guard **5 failed / 36** — both
windows legs, sidecar *and* plain — against the base guard's **1 failed / 19**, which is only the CPE-1903
refusal. The widening is real and is the more valuable half.

### Membership and ordering, red-proofed separately

RFC 7396 merge is order-dependent, so a membership-only check would call a wrong order correct.

| sabotage | result |
|---|---|
| overlay named but not committed | **13 failed / 49**, each naming the file |
| attacker-key overlay | **6 failed** (3 OSes × pubkey/endpoints) |
| `.sort()` in the extractor | **3 failed / 38 — ordering block only, every membership assertion green** |
| `.reverse()` | same, plus 3 in `tauriConfigChain.test.ts` |

**CPE-1873's pins not regressed:** an attacker key injected into each of the 7 chain files in turn reds
**exactly the shipped OSes that file governs** — base 12/29 (all six legs), `sidecar.conf` 6/35, `unix`
4/37 (linux+macos), the four per-OS files 2/39 each. Three spot-checked independently, including a per-OS
file.

### The author corrected their own evidence mid-flight

A `JSON.stringify` oracle **inflated the ordering evidence**, calling 21 of 24 orderings "different" when
only **16** were — 5 differed in **key insertion order alone**. Canonicalised, correction written at the
site. Independently re-measured: `JSON.stringify` **21 on every leg**; canonical **16 / 12 / 12**;
key-order-only **5 / 9 / 9** — and after review, **all three given per-leg at all three sites**, because a
per-leg number quoted without its leg, *inside the paragraph correcting an oracle's scope*, is the same
defect one scale down. CLAUDE.md gained *"a measurement that varies per case is reported per case, or not
at all."*

The stated limit is at the site **twice**: all six base-first orderings agree, so **what carries the order
today is the base's position**.

### The review's four required changes, and why they were the right shape

**F1 — the derivation's reach was a closed claim.** *"A third release channel added tomorrow is guarded on
the day it lands"* is true only of a channel that `uses: tauri-apps/tauri-action` **and** passes overlays
through `with.args`. Measured at **zero legs, silently**: a `run: npx tauri build --config …` step, a local
composite action, a reusable-workflow call, and `tauriScript:`. **The floors only catch shrinkage** — an
extra channel leaves the count at 6 and reports clean. Both blind-spot lists were closed lists and are now
**"at least these"**, with the honest good news kept: a third **tauri-action** channel *does* red, on the
`toEqual` over the workflow list. *That turns "open door" into "bounded gap", which is what the measurement
supports.*

**F2 — a stale command name in the file the PR edited.** The `verify-updater-pin` job header still named
`vitest run <one file>`, forty-two lines above the step this PR changed to `npm test`. **A comment naming a
command the file no longer runs, in a PR about comments that stop describing what they point at.**

**F3 — a bare `startsWith` accepted `src-tauri/../../planted.conf.json`.** Fixed with a segment test —
**and the complement pinned**, because the lazy passing implementation (reject anything containing two
dots) breaks the legitimate `tauri..odd.conf.json`. The Reviewer forced the filter over-eager and watched
the **complement** test red: *"the pair genuinely constrains both directions; neither can be satisfied by
the cheap wrong fix."* Four further shapes checked — backslash separators and `./..` are refused;
`%2e%2e` is **correctly accepted**, since nothing in the chain URL-decodes, so refusing it would be a false
positive.

**F5 and F6 — two checks that were not checks.** The naive scanners quoted at 4-of-4 and 2-of-4 were
**committed**, and the survivors asserted **by name**: renaming one survivor leaves both counts identical
and changes only identity, so a length assertion passes and the `toEqual` reds. And a direct overlay count
against `leg.args` turned `out.slice(1)` from **40/41** (where the single red was *incidental*) into **4
failed / 43**, three of them this assertion, one per leg, printing both counts and both lists. On whether
that count is a check or a mirror, **twelve shapes chosen to split a regex sweep from a token walk agree
12 of 12, by two genuinely different routes** — with the honest residue named: they share the flag
*vocabulary*, so a third upstream spelling would be missed in lockstep at 0 = 0.

### Vacuity answered structurally, not by running it

Emptying the scanner list makes the binding `undefined` so the length assertion **throws**; `BUILD_LEGS`
**cannot** be empty because the deriver throws at module load below its floors, so a near-empty derivation
fails **collection** rather than silently registering zero tests. And precisely: **3 of the 6 count
instances are `0 === 0`**, correctly, because `release.yml` passes no overlays — the three sidecar legs
carry the assertion that red-proofed at 2-vs-3. *Saying which instances are trivially true is the
difference between a vacuity check and a vacuity claim.*

### Sixteen adversarial shapes

Derived correctly: `-c`, `-c=`, `--config=`, folded `>-` and literal `|` block scalars, YAML-double-quoted
`args:`. Refused loudly: a surviving quote, a product-axis matrix, `${{ env.* }}`, `runs-on:` as a list,
`./src-tauri/…`, a dangling trailing `-c`. `--configuration` / `--config-dir` correctly ignored.
**"Refuses rather than guesses" is accurate.**

### The workflow change, accepted with its reasoning verified

`release-sidecar.yml`'s `verify-updater-pin` ran `npx vitest run <one file>`, **which would have skipped
`tauriConfigChain.test.ts` on the release path** — the file where the producer's refusals and
order-carrying live. *"The merged-config file would still red on a wrong chain, but not on a producer that
silently narrows."* Widened to `npm test`, mirroring what CPE-1903 did to the `cargo test` step three lines
above. Its wall-clock effect is **unmeasured and marked as such**.

**And a decline worth recording.** The Foreman noted that composing the two measured analogues (msrv 17.1 +
frontend 5.9, ×1.5) gives **35**, not the header's 30. The arithmetic was right and the worker **declined
anyway**: that header belongs to CPE-1967, already declares itself an unmeasured analogue, and resizing it
wants a real run — *"I'd rather not edit it on an argument."* **Changing a measured-looking number on the
strength of an argument is the same defect as writing an unmeasured claim, arriving from the opposite
direction.**

### Filed, not fixed

**CPE-1987** — `sidecarBundleResources.test.ts:469` carries *"Keep these two literals in lockstep with …
`pinned_pubkey.rs`"*: **a bare provenance claim on the root of trust**, in a file that already imports
`rustStrSliceAfter` and uses it to derive a different value from that same crate. Pre-existing; the Auditor
measured the four sites as byte-identical today, so it is a live-and-correct value with a dead check.

### Security — `SEC PASS`

The change **strictly increases** what the pins are computed over: 3 hand-listed sidecar OSes → 6 derived
legs across both channels, **plus the auto-merged half the pins previously could not see at all**. A
dropped overlay reds (9 or 6 tests depending which); an added overlay pulls itself into the merged config
and reds the pin, and **the pinned values are literals in the test, not derived from the workflow**; the
ENUMERATED listing fails closed on a directory, an unparseable file, or two files for one platform; and the
bounded YAML parser **throws rather than deriving an empty chain** when a workflow outgrows its subset.
The one gap in the stated blind spots was F1, now open-ended.

**Diff hygiene, confirmed by the Auditor:** no `tauri.conf.json`, no `capabilities/`, no key material
anywhere in the PR; the pinned pubkey and endpoint **byte-identical across all four sites**; every injected
value an obvious non-key; `git status --porcelain --untracked-files=all` empty after every sabotage.

### Gates at merge

`npm run check` **0 errors / 0 warnings** · full vitest **364 files / 5,528 passed / 62 skipped** ·
`cargo test --locked` in `crates/updater-verify` **147 / 0** · CI `completed success — total_count=26
pending=0 skipped=1 coverage=ok`.

**Family:** CPE-1873 (the pin this must not regress), CPE-1903 (the per-platform refusal this turns into
coverage, and the `npm test` precedent), CPE-1987 (the root-of-trust lockstep claim), CPE-1967 (measured
job timeouts — whose header was deliberately left alone), CPE-1932 (enumerate, don't recall), CPE-1933
(anchor on code, never on prose), CPE-1950 (two mechanisms are only two opinions if they can disagree).
