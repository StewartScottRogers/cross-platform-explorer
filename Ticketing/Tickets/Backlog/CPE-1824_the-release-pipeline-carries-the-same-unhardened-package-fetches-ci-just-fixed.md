---
id: CPE-1824
title: the release pipeline carries the same unhardened package fetches CPE-1787 just fixed in CI
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

CPE-1787 hardened the five `apt-get` sites in `.github/workflows/ci.yml` — retries, connect+data
timeouts, and a per-step `timeout-minutes` cap so a stalled mirror fails fast instead of riding to
the 360-minute default (which actually happened, for 1.5+ hours, on PR #935).

The identical unhardened `apt-get update && apt-get install -y ...` pattern is still live in the
**release** pipeline, which is arguably higher stakes — a hang there blocks shipping signed
installers, and the release build is the one nobody is watching:

- `.github/workflows/release.yml:49-54` and `:201`
- `.github/workflows/release-sidecar.yml:130-138`

The same hang class also reaches `ci.yml` through other tools, which CPE-1787 deliberately scoped
out:

- `ci.yml:496` — `brew install ffmpeg`
- `ci.yml:501` — `choco install ffmpeg`
- `ci.yml:499`, `:511`, `:529` — pdfium `curl` fetches that pass `--retry` but **no `--max-time`**,
  so a *stalled* transfer is unbounded even though a failed one retries
- `release-sidecar.yml:322` — **added 2026-08-20 by CPE-1764**, a sixth `curl` (fetching BtbN's
  `checksums.sha256`) with no `--max-time`, no `--connect-timeout`, and no `timeout-minutes` on its
  step. Confirmed by the CPE-1764 reviewer as a new site this ticket must pick up. Note it also has no
  `--fail`, so a 404 returns exit 0 with an error page in the variable — that half is CPE-1764's to fix,
  but check it landed before assuming this site only needs a timeout.

## Why it matters

A silent hang in the release workflow is worse than one in CI: CI hangs are noticed because someone
is waiting on a PR, whereas a release build hangs against nobody's attention and the first symptom is
a draft release with no installer assets — the exact state `/run` has to defend against by checking
for assets before publishing.

## Acceptance criteria

- [x] Every `apt-get` site in `release.yml` and `release-sidecar.yml` carries the same option set
      CPE-1787 established, and every step carries a `timeout-minutes` cap sized to its real duration.
- [x] `brew` and `choco` sites get an equivalent bound — a step-level `timeout-minutes` at minimum,
      plus any retry/timeout options those tools genuinely support (check, do not assume they mirror
      apt's).
- [x] Every `curl` fetch that can stall gets `--max-time` (and `--connect-timeout`), not just
      `--retry`. State the values chosen and why.
- [x] The `ciAptGetHardening` guard is extended to cover the release workflows, or a sibling guard is
      added — whichever keeps the assertions readable. It must parse the YAML structurally through
      `src/lib/preview/yaml.ts`, the way CPE-1787's guard does, never regex-over-raw-text.
- [x] Red-proof every assertion by deleting the single line it protects, observing red, and reverting;
      record the line for each.
- [x] Verify the `continue-on-error` interaction per site the way CPE-1787 did: a cap under
      `continue-on-error: true` still fails fast and then gets swallowed, so the job outcome is
      unchanged; a cap without it converts a silent hang into a hard failure. Say which each site is.

## Notes

Found by the CPE-1787 Reviewer, which credited that PR for declaring its `ci.yml`-only scope rather
than presenting a partial sweep as complete — this is the declared remainder, not a defect in it.

Separately, CPE-1787's own regression guard is being widened in that PR to catch bare `apt` as well
as `apt-get`; whatever spelling coverage lands there should be reused here rather than re-derived.

## Work Log

**2026-08-21** — Hardened every named site.

Sites hardened, each with `HARDENING_FLAGS` = `-o Acquire::ForceIPv4=true -o Acquire::Retries=3
-o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20` (verbatim reuse of CPE-1787's flags):

- `release.yml` — `release` job's "Install Linux system dependencies" (apt-get, `timeout-minutes: 8`,
  no `continue-on-error` → cap converts a silent hang into a hard failure) and `catalog` job's
  "Install libdbus…" (apt-get, `timeout-minutes: 5`, same — no `continue-on-error`).
- `release-sidecar.yml` — `release-sidecar` job's "Install Linux system dependencies" (apt-get,
  `timeout-minutes: 8`, no `continue-on-error`); "Stage native deps — ffmpeg + pdfium" step gained a
  `timeout-minutes` it never had at all (`${{ matrix.platform == 'macos-latest' && 35 || 12 }}` —
  Windows/Linux only download+extract, macOS additionally `git clone`s + compiles ffmpeg from
  source, the documented "slow outlier" — no `continue-on-error`, so this is also a silent-hang → hard-
  failure conversion); its `fetch()` helper's `curl` gained `--connect-timeout 15 --max-time 240`, and
  `verify_btbn_checksum()`'s `curl` (the CPE-1764 sixth site, `checksums.sha256`) gained
  `--connect-timeout 15 --max-time 60`. Confirmed CPE-1764's `--fail`-equivalent (manual HTTP-status
  check + `exit 1`) already landed on both — this ticket only added the stall bound.
- `ci.yml` (non-apt-get sites CPE-1787 scoped out) — `brew install ffmpeg` (macOS): no genuine
  per-invocation retry/timeout flag exists for `brew install` (checked via web search, not assumed —
  Homebrew's own internal curl calls have an undocumented-for-users default, not a `brew install`
  flag), so only `timeout-minutes: 10` was added. `choco install ffmpeg` (Windows): Chocolatey DOES
  expose `--execution-timeout` (alias `--timeout`), CommandExecutionTimeout in **seconds**, bounding
  the whole command including the download, default 2700s — added `--execution-timeout=480` PLUS an
  independent `timeout-minutes: 10` backstop (choco's own timeout enforcement has had real bugs
  around disabling/zero values, per chocolatey/choco#1224 and #1747, so it isn't trusted alone). The
  three pdfium `curl` sites (Linux/macOS/Windows) each gained `--connect-timeout 15 --max-time 180`
  plus `timeout-minutes: 6`; their pre-existing `--retry 5 --retry-all-errors --retry-delay 3` was
  left untouched (it covers a *different* failure mode — retryable terminal errors — and doesn't
  bound a stall). **Round 2 also added `--retry-max-time 150` to all three — see the 2026-08-21
  entry; without it the `--max-time` on these lines bounded one attempt, not the command.** All six
  `ci.yml` sites already carried `continue-on-error: true`, so each cap fails
  the step fast and then gets swallowed into a green job outcome, exactly like CPE-1787's site 3/5.

What each flag does and does NOT cover (per the ticket's instruction to verify, not assume):
- `apt-get -o Acquire::*` — same four options CPE-1787 verified for ci.yml; unchanged here.
- `curl --connect-timeout N` — bounds only the TCP/TLS handshake phase.
- `curl --max-time N` — bounds ONE attempt (connect + transfer). **Corrected in round 2 — the
  original wording here was wrong; see the 2026-08-21 entry below.**
- `curl --retry-max-time N` — bounds the retry SERIES; this is what makes a curl-level bound real
  once `--retry` is in play.
- `curl --retry N` — retries on a transient/retryable terminal error (refused connection, certain
  5xx); does NOT bound an open connection with zero bytes moving.
- `choco --execution-timeout=N` (seconds) — bounds the whole choco command (download + install) via
  choco's own internal timer; independently backstopped by GH Actions' `timeout-minutes` because
  choco's own enforcement has documented bugs.
- `brew install` — no equivalent flag exists; `timeout-minutes` is the only bound.
- GH Actions `timeout-minutes` — kills the whole step at the runner level regardless of what's
  hanging inside it (curl, choco, apt, git clone, compile); this is the backstop of last resort for
  every site above.

Guard: new sibling file `src/lib/releaseHangHardening.test.ts` (11 tests), parsing `release.yml`,
`release-sidecar.yml`, and `ci.yml` structurally via `parseYaml` (`src/lib/preview/yaml.ts`), reusing
`ciAptGetHardening.test.ts`'s `HARDENING_FLAGS` string and `APT_COMMAND_WORD` regex verbatim rather
than re-deriving them, plus a catch-all "no apt/apt-get invocation left unhardened" test per file
(mirrors CPE-1787's regression guard). Every one of the 11 tests' assertions was individually
red-proofed by deleting the exact line it protects (the `timeout-minutes` line, one `apt-get` site's
hardening flags, one `curl` site's `--connect-timeout`/`--max-time`, `--retry 5`, the choco
`--execution-timeout`, and the `Stage native deps` step's conditional `timeout-minutes` expression)
and observing the specific assertion go red before reverting — 13 individual red-proof runs in total
(two assertions each got their own separate red-proof: the pdfium `curl` "bounds connect+transfer"
test doubles as `--retry` and `--connect-timeout`/`--max-time`, and the choco test covers both
`--execution-timeout` and `timeout-minutes` as two separate deletions).

Gates: `npx vitest run` — 321 files / 4269 tests, all green. `npm run check` — 0 errors, 0 warnings.
Did not touch `Set-Content -Encoding utf8` (CPE-1842's signing-step encoding bug) or any
`PDFIUM_TAG`/`FFMPEG_BUILD_TAG`/hash-pinning lines (CPE-1839) — confirmed by a diff-content grep
after finishing; no line overlap with either ticket's scope. **The CPE-1839 half of that claim was
wrong — corrected in round 2 below. The CPE-1842 half was re-checked and is correct.**

**2026-08-21 (round 2)** — Reviewer returned CHANGES REQUESTED. Two corrections, one of them a
false statement that had been recorded as verified fact.

**1. `curl --max-time` does NOT bound a command that retries — the round-1 claim was backwards.**

Round 1 wrote, in `ci.yml`'s code comment and twice in this Work Log, that `--max-time` bounds "the
ENTIRE single curl invocation (connect + transfer + all `--retry` attempts within that one
process)" and said it was "confirmed against curl's own docs". It is the opposite. curl's
`docs/cmdline-opts/max-time.md` says:

> "If you enable retrying the transfer (--retry) then the maximum time counter is reset each time
> the transfer is retried. You can use --retry-max-time to limit the retry time."

Re-verified here two independent ways before acting, rather than taking the Reviewer's word: fetched
that file from curl's own repo, and read `curl --help all` on curl 8.21.0 locally, which describes
`-m, --max-time` as "Maximum time allowed for transfer" (singular, per-transfer) and lists a separate
`--retry-max-time` as "Retry only within this period". Both agree with the Reviewer, not with round 1.

Consequence at the three `ci.yml` pdfium sites (the only sites in these three workflows that combine
`--retry` with `--max-time`): with `--retry 5 --retry-delay 3 --max-time 180`, curl's own worst case
was roughly 5 x 180s plus delays — about 15 minutes — not the 180s the comment asserted.

**This was never a live hang risk.** All three steps also carry an independent `timeout-minutes: 6`,
so GitHub Actions kills them at 360s wall-clock regardless of what curl believes. The shipped
behaviour was always bounded; what was broken was the explanation. A wrong claim presented as a
verified fact is worse than an honest assumption, because the next person to touch these lines
reasons from it — and it was the headline methodology point of the whole PR.

Fix: added `--retry-max-time 150` to all three pdfium `curl` lines, so the curl-level bound is
genuinely true instead of merely softened in prose.

Why 150 specifically. Per curl's `--retry-max-time` docs, the timer starts before the first attempt
and is checked *before starting each new retry*; an attempt already in flight is allowed to run to
completion. So curl's real worst case is `retry-max-time + max-time`, not `retry-max-time`. Sizing it
against the existing backstop:

- step backstop is `timeout-minutes: 6` = 360s;
- an in-flight attempt can add up to `--max-time 180`;
- so require `N + 180 < 360`, i.e. `N < 180`;
- take `N = 150` → worst case 330s, landing strictly inside 360s with ~30s spare for the step's
  `mkdir`/`tar`/`echo`.

The point of leaving margin is that curl should lose on its *own* terms — a real exit code plus
`--fail`'s diagnostics in the log — rather than being killed opaquely by the runner, which is what a
value that raced the backstop would produce. 150 also costs the retries nothing in practice: their
actual job is retryable terminal errors (refused connection, 5xx), which fail in seconds each, so 5
of them plus 3s delays is ~20s — far inside 150.

`release-sidecar.yml`'s two `curl` sites pass no `--retry` at all, so their `--max-time` really is a
whole-invocation bound and their values (240s / 60s) stand unchanged. Their comment was reworded
anyway to say the property holds *because* there is no `--retry`, with an explicit caution that
adding one later requires `--retry-max-time` — otherwise that comment becomes the next false claim.

**2. Guard extended so this exact reasoning error cannot recur.**
`src/lib/releaseHangHardening.test.ts` 11 tests → 15. The new assertions are a *generic scan* of
every non-comment `curl` line in `ci.yml`/`release.yml`/`release-sidecar.yml` (via the same
`parseYaml` structural route as the rest of the file), failing if any line pairs `--retry` with
`--max-time` but no `--retry-max-time` — so a NEW curl added anywhere in these workflows is caught,
not just the three known sites. Flag matching uses `(?<![\w-])--retry(?![\w-])`, whose tail is what
stops `--retry-delay`/`--retry-all-errors`/`--retry-connrefused`/`--retry-max-time` counting as
`--retry`. `#`-comment lines are stripped first, because these `run` blocks contain long comments
naming these very flags — the comment-vs-key confusion CPE-1787's Reviewer round already found once.

Red-proofed twice, both reverted immediately after:
- Deleted ` --retry-max-time 150` from `.github/workflows/ci.yml:555` (the Linux pdfium `curl`).
  3 tests failed, 12 passed — including the new generic scan, reporting
  `expected [ Array(1) ] to deeply equal []`.
- Deleted `--retry 5 ` from that same line 555. 2 tests failed, 13 passed — the generic scan went
  *green* (the line no longer retries, so it is correctly not an offender) while the new
  non-vacuity test caught it with `expected 2 to be 3`. That is exactly the case the offenders scan
  structurally cannot see, which is why the non-vacuity test exists.

**3. The "zero line overlap" claim was half wrong (Reviewer's second correction).**
Round 1 and the PR body claimed zero line overlap with both CPE-1842 and CPE-1839. Re-checked:
- **CPE-1842 — correct, stands.** This PR touches no `Set-Content -Encoding utf8` / signing-step line.
- **CPE-1839 — wrong.** This PR *did* edit the `sums_code=$(curl -sSL --connect-timeout 15
  --max-time 60 ...)` line in `release-sidecar.yml`'s `verify_btbn_checksum()` (adding the timeout
  flags), and that is a line CPE-1839 will need to edit again. The round-1 check only grepped for
  `PDFIUM_TAG`/`FFMPEG_BUILD_TAG`/hash-pinning *tokens*, which that line does not contain — so the
  grep came back clean and was reported as "no overlap" when the real question was which lines
  CPE-1839's work will land on. Whoever takes CPE-1839 should expect to rebase over this PR's change
  to that line rather than finding it untouched.

Gates re-run in round 2 — numbers in the PR comment.
