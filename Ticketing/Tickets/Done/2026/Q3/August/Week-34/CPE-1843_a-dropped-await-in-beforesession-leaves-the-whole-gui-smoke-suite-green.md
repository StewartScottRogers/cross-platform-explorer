---
id: CPE-1843
title: a dropped await in beforeSession would leave the whole gui-smoke suite green
type: task
priority: Low
status: Done
tags: ready
estimate: S
created: 2026-08-21
closed: 2026-08-22
---

## Problem

`gui-smoke/wdio.conf.ts`'s `beforeSession` waits for two ports before returning — tauri-driver's own
port and the native driver's. Both waits are correctly `await`ed today (`:1304` and `:1307-1313`,
verified by reading the merged code).

**Nothing pins that.** `npm run test:unit` exercises the extracted `waitForPort` function in isolation
(`gui-smoke/lib/waitForPort.test.ts`); nothing exercises `beforeSession` itself. A future edit that drops
one `await` — the classic form of exactly this bug — would leave the full 121-test suite green, and the
race would return silently on the platform where it is hard to reproduce.

That is the same shape this whole ticket family keeps closing: the fix is right, and the thing that keeps
it right is untested.

## A second, related exposure

`.github/workflows/gui-smoke.yml:202` and `:463` run `cargo install tauri-driver --locked` with **no
version pin**, so CI installs whatever is newest on crates.io.

CPE-1832's fix depends on `--port` and `--native-port` remaining real flags with stable defaults —
verified against tauri-driver 2.0.6's `cli.rs` (4444 and 4445, matching the harness constants exactly).
If upstream renames or re-defaults them, the harness silently stops waiting on the right thing.

Pre-existing, not introduced by CPE-1832 — but that fix increased how much depends on that CLI shape
staying still. Pinning the version makes an upstream break show up as a deliberate version bump rather
than a CI mystery.

## Acceptance criteria

- [x] A regression guard fails if either `waitForPort` call in `beforeSession` loses its `await`. A
      lint rule against un-awaited calls, a type-level guard, or a test that exercises `beforeSession`
      directly — whichever is cheapest to keep honest.
- [x] Red-proof it: drop one `await`, confirm the guard fires, restore, confirm green. If the guard does
      not fire, it has not closed the gap.
- [x] `tauri-driver` is version-pinned in both workflow sites, or the decision not to pin is recorded
      with its reason.
- [x] If pinned: a note next to the pin saying what depends on it (the `--port` / `--native-port` flags
      and their 4444/4445 defaults), so the next person bumping it knows what to re-check.

## Notes

Filed from the CPE-1832 review, which classified all of this as FOLLOW-UP rather than blocking — the
code is correct today and the fix was verified green on all four Linux CI shards, including shard 2 where
the original failure was observed.

A third observation from that review, recorded but not actioned: shard 2 took ~14 minutes against 6-7 for
the others before landing green. Nothing in the fix explains it (the two-port wait is bounded to about 30
seconds worst case), so it is almost certainly spec-assignment variance — worth a glance only if it
recurs. **It recurred.** CORRECTION to an earlier version of this line, which wrongly said it was not
observed again: this ticket's own PR run (32555323418) shows shard 2 at **14m21s** against 6m28s / 7m06s /
7m10s for shards 4/1/3, all green — the same shape, on a PR whose only functional change is a CI version
pin that cannot affect spec runtime. Two sightings on runs where nothing plausibly causal changed makes
"shard 2 owns the heaviest specs" the strong reading rather than noise. Not chased here (out of scope); a
balance ticket against `scripts/write-shard-manifest.ts` is being filed separately.

## Work Log

### 2026-08-21 — guard + version pin

**Guard mechanism chosen: a TypeScript-AST assertion over `wdio.conf.ts`'s source**, new file
`gui-smoke/lib/beforeSessionAwaits.test.ts` (4 cases, runs under the existing `npm run test:unit`).

Why this and not the alternatives:

- *A behavioural test calling `beforeSession` for real* was the first choice and was rejected on cost.
  Importing `wdio.conf.ts` is not free: its module top level `throw`s unless a real Tauri-CLI release
  binary already exists (the CPE-1044 guard), reads `src-tauri/tauri.conf.json`, and computes shard
  assignments from `process.env`. A unit test of the hook would therefore need a full app build to reach
  its first assertion — exactly the cost `waitForPort.ts` was extracted to avoid — or a refactor that
  puts more new, untested machinery between the harness and the guarded behaviour than the guard is
  worth.
- *An ESLint `no-floating-promises` rule* needs type-aware linting, and `gui-smoke/` has no ESLint setup
  at all (only stray `eslint-disable` comments inherited from the root). Standing up typed linting for
  one assertion is the most expensive option here, not the cheapest.
- *A type-level guard* cannot express it: `waitForPort` returns `Promise<void>` whether or not the
  caller awaits it.

The AST guard uses the `typescript` devDependency already present (it backs `npm run typecheck`), costs
milliseconds, needs no build, and asserts precisely the property at issue — every `waitForPort` call
inside `beforeSession` is the operand of an `await`. It is a *syntactic* guard and the file says so: it
proves the harness still waits, not that the ports are reachable (that stays `waitForPort.test.ts` plus
the real CI run).

**Red-proof (all four sabotages fired; line numbers are the final, shipped ones):**

| sabotage | result |
|---|---|
| dropped the `await` at `gui-smoke/wdio.conf.ts:1311` (the `TAURI_DRIVER_PORT` wait) | RED — `wdio.conf.ts:1311 — the waitForPort call for TAURI_DRIVER_PORT is NOT awaited (its parent expression is ExpressionStatement, not AwaitExpression)`; 3 pass / 1 fail |
| dropped the `await` at `gui-smoke/wdio.conf.ts:1314` (the `NATIVE_DRIVER_PORT` wait) | RED — same message naming `NATIVE_DRIVER_PORT` and line 1314; 3 pass / 1 fail |
| pointed the second wait at `TAURI_DRIVER_PORT` (waiting on the front door twice) | RED — 2 pass / 2 fail |
| removed `async` from `beforeSession` | RED — 3 pass / 1 fail |

Each was restored via `git checkout --` and the guard confirmed green again (4/4) after every one. The
two `await` drops were each proven twice: once at the pre-edit line numbers (1304 / 1307) and again at
the post-edit shipped line numbers above, after the comment additions shifted the file down by 7 lines.

**Version pin: PINNED, `cargo install tauri-driver --version 2.0.6 --locked` at BOTH sites**
(`.github/workflows/gui-smoke.yml`, the Windows leg's step and `gui-smoke-linux-build`'s step). Flags and
defaults re-verified independently for this ticket, not inherited from the brief, by reading
`~/.cargo/registry/src/*/tauri-driver-2.0.6/src/cli.rs`:

- `--port NUMBER` → `args.value_from_str("--port").unwrap_or(4444)`
- `--native-port NUMBER` → `.unwrap_or(4445)`
- `--native-host HOST` → `.unwrap_or(String::from("127.0.0.1"))` — the host the harness polls, so it
  matters too
- unknown args are rejected (`args.finish()` errors on anything extraneous), so a RENAMED flag fails
  loudly.

CORRECTED IN ROUND 2 (see below): an earlier version of this entry, and of the pin notes in the workflow,
also listed "the 4444/4445 defaults" as load-bearing and claimed a re-defaulted port would break the
harness silently. That is over-stated. `beforeSession` passes `--port 4444 --native-port 4445`
**explicitly**, so their upstream defaults are overridden and cannot silently break anything; for those
two flags only the NAMES matter, and a rename fails loudly. The silent-re-default risk is real only for
`--native-host`, which the harness does NOT pass and whose 127.0.0.1 default it polls. The notes now aim
the argument at the flag it actually applies to.

crates.io reports 2.0.6 as both `max_version` and `newest_version` (latest of 2.0.0-2.0.6 + 0.1.5), so
the pin changes nothing about what CI installs today — it only converts a future upstream change from a
CI mystery into a deliberate bump. A "what depends on this" note sits next to the pin at both sites, plus
matching notes at `gui-smoke/README.md`'s prereq and `wdio.conf.ts`'s port constants.

**Gates:** `npm run test:unit` (gui-smoke) 125 tests / 36 suites, 0 fail (was 121 / 35 — +4 from this
guard); `npm run typecheck` (gui-smoke) clean; root `npx vitest run` 4258 tests / 320 files, 0 fail;
root `npm run check` 0 errors / 0 warnings; `.github/workflows/gui-smoke.yml` re-parsed with PyYAML —
4 jobs, both `cargo install tauri-driver` steps carrying the pin.

**Not verified:** the real GUI smoke suite was not run locally (needs a display and a built app — CI
covers it), so this change is proven green only against the unit/type/parse gates and whatever the PR's
CI run reports. `cargo install tauri-driver --version 2.0.6 --locked` itself was not executed here; the
pin was verified against the already-vendored 2.0.6 source in the local registry cache and against
crates.io's version list.

### 2026-08-22 — round 2: the guard was hollow *in waiting*

Independent review approved PR #989 (9-mutation battery, 8 fired — including `void waitForPort(...)`,
assign-without-await, an unreturned `.then()`, and a decoy `// await waitForPort(...)` comment, which
confirms it is a real AST walk and not a source scan) but found one thing worth fixing, and it is the
interesting one.

**The finding: the guard could have deleted itself silently, under exactly the edit class that removes
the wait.** The first version parsed `wdio.conf.ts` and located `beforeSession` in the `describe` BODY,
not inside an `it()`. Those two calls are precisely the ones that prove the guard *reached its target*.
Measured here, not argued:

| shape | mutation | node 22.22.3 (this machine) | node 20.19.4 (what CI pins) |
|---|---|---|---|
| ORIGINAL | hook extracted to a named helper (`beforeSession: startTauriDriver,`) | prints `not ok 1`, reports **`# fail 0`**, total silently drops 125 → 121, **exits 0** | exits 1 |
| FIXED | same mutation | 5 failures, total stays 126, **exits 1** | **exits 1** |
| FIXED | `wdio.conf.ts` renamed/moved (ENOENT) | 5 failures, total stays 126, **exits 1** | **exits 1** |

So the guard was correct today **only because of the node 20 pin at `gui-smoke.yml:165,439,665,929`** —
an unrelated version pin that nobody bumping node would think to re-check, and one this repo would
plausibly bump for its own reasons. Worth stating plainly: this is the one hollow-guard finding in the
batch that was hollow *in waiting* rather than hollow now. A guard whose disappearance is invisible is
worse than no guard, and "it passes today" was never the property being bought.

**Fix:** every assertion now runs inside an `it()` — same discipline as `libLayout.test.ts`. Parse +
locate + collect run fresh per test via `analyzeBeforeSession()` (a few ms for a ~1350-line file), and a
new first case, `it("locates the beforeSession hook in wdio.conf.ts")`, makes "the guard could not find
its target" a FAILING TEST rather than a suite that quietly shrinks. The file's header comment records
the measurement above so the next person does not reintroduce describe-body work.

**Red-proof, round 2 — all four sabotages re-run at the final shipped line numbers, with exit codes**
(`npm run test:unit`, node 22.22.3; baseline is 126 tests / 126 pass / exit 0):

| sabotage | exit | counts |
|---|---|---|
| dropped `await` at `gui-smoke/wdio.conf.ts:1314` (`TAURI_DRIVER_PORT` wait) | **1** | 126 tests / 125 pass / 1 fail, message names line 1314 and the port |
| dropped `await` at `gui-smoke/wdio.conf.ts:1317` (`NATIVE_DRIVER_PORT` wait) | **1** | 126 tests / 125 pass / 1 fail, message names line 1317 and the port |
| `wdio.conf.ts` renamed (the vanish mutation) | **1** | 126 tests / 121 pass / 5 fail |
| hook extracted to a named helper (the vanish mutation) | **1** | 126 tests / 121 pass / 5 fail |

Both vanish mutations were also run under **node 20.19.4** (obtained via `npx -y node@20.19.4`, driving
`node_modules/tsx/dist/cli.mjs` directly): exit **1** on both, 5/5 failing. Every mutation was restored
and the suite re-confirmed green (126/126, exit 0) after each. Note the totals: under the fixed shape the
count never shrinks — the cases still exist and fail, which is the whole difference.

**Also corrected in round 2:** the pin note's over-claim about the 4444/4445 defaults (see the CORRECTED
block above — the asymmetry now points at `--native-host`, the flag it actually applies to), in
`gui-smoke.yml` at both sites, `gui-smoke/README.md`, and `wdio.conf.ts`'s port-constant comment. And the
shard-2 line at the top of this ticket, which wrongly said the outlier had not recurred; it did, on this
ticket's own PR run.

**Round-2 gates:** gui-smoke `npm run test:unit` **126 tests / 36 suites / 126 pass / 0 fail** (exit 0);
gui-smoke `npm run typecheck` clean; root `npx vitest run` 4258 tests / 320 files / 0 fail; root
`npm run check` 0 errors / 0 warnings; `gui-smoke.yml` re-parsed with PyYAML (4 jobs, both install steps
still carrying the pin).

**Still not verified after round 2:** the real GUI smoke suite locally (unchanged reason); the Windows
leg's pinned install, which no run on this PR executes because that job is schedule/dispatch-only; and
node versions other than 20.19.4 and 22.22.3.
