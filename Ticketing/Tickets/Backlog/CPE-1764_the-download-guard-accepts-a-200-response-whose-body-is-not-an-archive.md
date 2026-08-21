---
id: CPE-1764
title: The release download guard accepts a 200 response whose body is not an archive, so a wrong body still reaches tar
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-17
closed:
---

## Problem

Found by **CPE-1762's UAT**, which was explicitly trying to break the new guard rather than confirm it.
Recorded here because it is a real residual risk and outside CPE-1762's acceptance criteria, so folding it
into that PR would have been scope creep.

`fetch()` in `.github/workflows/release-sidecar.yml` (~line 248) checks **only the HTTP status code**:

```bash
code=$(curl -sSL --write-out '%{http_code}' -o "$out" "$url")
if [ "$code" != "200" ]; then ... exit 1; fi
```

Measured: fetching `https://httpbin.org/html` — an HTML page served with **HTTP 200** — was **accepted**,
exit 0, and the HTML document was written to the output path as if it were an archive. It would then be
handed to `tar`/`unzip`, reproducing exactly the misleading `xz: File format not recognized` error that
CPE-1762 exists to eliminate.

## Why it is Low, and why it is still worth a ticket

BtbN's actual rot is **404-shaped** — they delete the release, so the pinned asset genuinely 404s, and the
status check catches that. This is the failure mode that has actually bitten us, twice, and it is closed.

The gap is the *other* shapes: a CDN or proxy serving an error page with a 200, a repository that replaces
a pruned asset with a placeholder, an interception page on a restricted runner network, or a partial body
truncated by a dropped connection but still reported 200. None has happened here. But the guard's whole
justification is "the first error line should tell the truth", and in these shapes it still lies in exactly
the old way.

Note the shape of this defect: **it fails by succeeding.** That is the class this repo has been bitten by
repeatedly, and the reason to close it rather than tolerate it.

## What to do

Add a cheap body sanity check after the status check, for each downloaded artifact:

- Verify the file's **magic bytes** match what the caller expects (zip `PK\x03\x04`, xz `\xFD7zXZ`, gzip
  `\x1f\x8b`). `file -b` is available on all three runners, or check the bytes directly — decide which and
  record why.
- Consider a **minimum plausible size** as a second cheap signal — a 197-byte "archive" is not one. Size
  alone is weaker than magic bytes; use it as a supplement, not a substitute.
- A **published checksum** would be strictly better than either, if the upstream publishes one per asset.
  Check whether BtbN and bblanchon do; if they do, prefer that and say so.

Whatever is chosen, the failure message must keep CPE-1762's standard: name the URL, say what was expected
versus what arrived, and never let the bad bytes reach `tar`/`unzip`.

## Acceptance criteria

- [ ] A 200 response carrying a non-archive body fails the guard, with a message naming the URL and saying
      the body is not the expected archive type. Demonstrate with real output against a live 200-serving
      HTML URL, as the UAT did.
- [ ] Every currently-guarded download (pdfium on all three OSes, ffmpeg on Windows and Linux) gets the
      check, with the expected type per call site — not a single hardcoded type.
- [ ] A genuine, live download still succeeds and is not slowed meaningfully.
- [ ] A truncated-but-200 body is considered: state whether the chosen check catches it, and if not, say so
      explicitly rather than leaving it implied.
- [ ] The macOS ffmpeg leg (build-from-source via `git clone`) is confirmed out of scope, in writing.

## Notes

Related: **CPE-1762** (the status-code guard this extends; PR #922), **CPE-1763** (the scheduled freshness
check), CPE-1258 (introduced the native-deps staging step).

Filed by the Foreman during the batched sprint of 2026-08-17, from CPE-1762's UAT break-attempt findings.

## Work Log

**2026-08-20 — Worker (branch `cpe-1764-download-body-guard`)**

Extended `fetch()` in `.github/workflows/release-sidecar.yml` (the CPE-1762 status-code guard) with a
body-sanity layer after the existing `HTTP 200` check, plus a new checksum layer for the one download
family that publishes one.

**What was chosen and why (per "What to do"):**
- **Magic bytes**, checked directly via `head -c N | od -An -tx1` (no `file -b` dependency — one fewer
  tool to assume is present cross-runner, and the comparison is 3 lines of POSIX shell). Verified `od`,
  `head`, `wc`, `sha256sum` are all present in the Git Bash environment that stands in for
  `windows-latest`'s `shell: bash` here; ubuntu-latest/macos-latest ship all four natively.
- **Minimum plausible size**, `MIN_ARCHIVE_BYTES=65536` (64KiB) — checked live Content-Length for every
  pinned asset: pdfium ≈3.4–3.7MB (all 3 OSes), ffmpeg-Windows 145,349,121 bytes, ffmpeg-Linux
  111,679,252 bytes (measured; corrected here from an earlier vague "≈106–138MB" — the Reviewer's own
  full download of the pinned Linux archive confirms the 111,679,252-byte figure). The floor sits ~53×
  below the smallest real asset, so it can't false-positive on a genuine download, while still catching
  anything HTML-error/placeholder/truncated-connection sized.
- **Published checksum, where it exists** — checked both upstreams directly (`gh release view --json
  assets`): **BtbN DOES publish** a `checksums.sha256` covering every asset in a release (confirmed
  against the actual pinned tag `autobuild-2026-07-31-14-10`, real sha256 line format, real hash
  extracted and matched). **bblanchon does NOT** — pdfium releases carry only a
  `pdfium-attestation.json`, which is a Sigstore bundle (needs `gh attestation verify`/cosign, not
  curl+sha256sum); verifying that is a heavier follow-up, not built here, and the workflow now says so
  in a comment next to the pdfium calls. So: magic-bytes + size for all 5 call sites; ADDITIONALLY a
  full sha256 checksum comparison against BtbN's published file for the 2 ffmpeg (BtbN) call sites only.

**What the check catches, and what it explicitly does NOT (AC: truncated-but-200 body):**
- Magic bytes alone catches: wrong content type entirely (HTML page, empty body) — does NOT catch a
  truncated transfer that stopped after a valid header (the first bytes are still correct).
- Min-size alone catches: any body short enough to fall under 64KiB, including a truncated-early
  transfer — does NOT catch a transfer truncated LATE (past 64KiB) with an otherwise-valid header, nor
  any content substitution that happens to be the right size.
- **Neither catches a body that is the right type AND past the size floor but is still wrong** (a late
  truncation, or corruption in transit) — proven with a red-proof fixture: a valid, correctly-sized zip
  with one byte flipped deep inside passed magic-bytes+size cleanly (`RESULT=PASS`).
- The **checksum layer closes that gap for ffmpeg (BtbN) only, for corruption/truncation — NOT for
  tampering.** `checksums.sha256` is fetched from the SAME release, SAME origin, SAME TLS channel, SAME
  `curl` as the artifact itself: there is no independent trust anchor (no hash pinned in this repo, no
  signature, no second origin). Anyone who can serve a malicious ffmpeg archive can serve a matching
  checksum in the same breath. **A mismatch proves the body differs from what BtbN currently publishes —
  corruption, truncation, or a substitution that didn't also rewrite the checksum — not "verified
  authentic."** (Corrected here: an earlier draft of this Work Log said "or tampered" and "closes that
  gap" without that qualifier, overclaiming tamper resistance the layer doesn't provide — flagged by the
  PR's Reviewer.) The same tampered-byte fixture WAS rejected by `verify_btbn_checksum` with a
  checksum-mismatch message — that's real integrity-over-the-wire, just not authenticity.
- **Ordering correction (also flagged by the Reviewer, and important — it inverts which side of this is
  actually weaker):** pdfium (bblanchon) does NOT get this third layer, but that is NOT because pdfium is
  the weaker case. bblanchon publishes something **stronger** than BtbN's same-origin checksum: a
  Sigstore attestation bundle (`pdfium-attestation.json`, Fulcio-issued cert, confirmed live — 18,215
  bytes) with a genuine **independent trust anchor**. pdfium has the **stronger** authenticity mechanism
  available and currently **unused**; ffmpeg (BtbN) has the **weaker** mechanism (same-origin sha256) and
  it IS wired up. Read it as: ffmpeg gets integrity-over-the-wire today; pdfium's path to real
  authenticity (Sigstore verification, via `gh attestation verify`/cosign) is the actual higher-value
  follow-up, not a footnote. Don't let "ffmpeg has a checksum, pdfium doesn't" be misread as "ffmpeg is
  more verified" — for authenticity it is the other way around.
- **Also:** `PDFIUM_TAG`/`FFMPEG_BUILD_TAG` are literal pins and nothing floats, but GitHub release
  **assets are mutable within a tag** — the tag pin alone does not pin content. This applies whether or
  not a checksum layer exists for that download.
- Stated explicitly here per the AC, not left implied.

**Failure messages** (rule: must beat `tar`'s own confusing error) — every branch prints `::error::` lines
that name the URL, state HTTP 200 was received, and say what type/size/checksum was expected vs. what
arrived (see the workflow diff for exact text).

**Live verification (rule 3/AC1):**
- Ran the exact edited `fetch()` function (extracted verbatim from the committed file, not a
  reconstruction) against `https://httpbin.org/html` — HTTP 200, HTML body — and it was **rejected**:
  `download body is not a gzip (.tgz) archive: https://httpbin.org/html … expected magic bytes 1f8b, got
  3c21 (HTTP 200, 3741 bytes)`. This is the ticket's own repro case, reproduced against the real edited
  code.
- Ran the same function against the real pinned `pdfium-linux-x64.tgz`
  (`chromium/7961`) — a genuine live download — and it **passed**, byte-identical to the real
  Content-Length (3,650,783 bytes). Confirms AC "a genuine, live download still succeeds."
- Ran `verify_btbn_checksum`'s real-network path (fetch + awk-extract) against the actual pinned BtbN
  tag's `checksums.sha256`: confirmed a real pinned filename's hash extracts correctly
  (`ffmpeg-n8.1.2-34-g9b6c8969e0-win64-lgpl-8.1.zip` → `089e4169e9…`, 64 hex chars, matches `gh release
  view`'s own listing), and confirmed the "no entry for this filename" failure path fires correctly
  against the real remote file for a filename that doesn't exist in it.
- Full downloads of the real ffmpeg archives (145,349,121 bytes Windows, 111,679,252 bytes Linux) were
  NOT run by me (bandwidth/time in this sandbox); the checksum-comparison logic itself was validated
  against local fixtures (match + a tampered-byte mismatch), and the network fetch+parse of the real
  `checksums.sha256` was validated live as above. **Update (Reviewer round, same day):** the PR's
  Reviewer independently downloaded the real pinned 111,679,252-byte ffmpeg-Linux archive and ran both
  `fetch()` and `verify_btbn_checksum()` against it live — rc=0. That closes this gap: the full positive
  path on a real large asset is now confirmed, not just inferred from the smaller pdfium case.

**Local fixture wrong-body cases (rule 4)** — all against a local `python -m http.server` stub (chosen
over `file://` because curl doesn't populate `%{http_code}` for `file://`, which would make the
HTTP-200 gate untestable that way):
- HTML error page (200, 3.3KB): rejected — magic mismatch.
- HTML error page padded to 86KB (200, above the size floor): rejected — magic mismatch (isolates the
  magic check from the size check).
- Truncated archive (200, 500 bytes, valid gzip header): rejected — size check.
- Empty body (200, 0 bytes): rejected — magic mismatch (empty).
- Genuine gzip/zip/xz fixtures (all >64KB, correct magic): all passed.
- Tampered zip (correct magic, correct size, 1 byte flipped mid-file): passed magic+size, **rejected by
  checksum** — the AC's "truncated-but-valid-prefix is a different failure" case, demonstrated directly.

**Red-proof (rule 4, on the actual committed file, then reverted)** — each removed, its fixture stopped
being caught, confirmed, reverted:
- Removed the magic-byte block (`if [ "$actual" != "$magic" ]…`, line 296) → the 86KB padded HTML fixture
  fell through as a false PASS. Reverted.
- Removed the size block (`if [ "$size" -lt "$MIN_ARCHIVE_BYTES" ]…`, line 303) → the 500-byte truncated
  fixture fell through as a false PASS. Reverted.
- Removed one `verify_btbn_checksum` call (Linux ffmpeg site) → the tampered-content case for that site
  would no longer be caught (checksum test asserting call count drops from 2 to a missing call was also
  exercised on the guard test itself, see below). Reverted.

**Guard test** — added `src/lib/releaseSidecarDownloadBodyGuard.test.ts` (10 tests), parsing
`release-sidecar.yml` structurally through `src/lib/preview/yaml.ts` (`parseYaml`) per CPE-1787's
precedent, not regex-over-raw-text — reads `step.run` off the parsed object so a `# CPE-1764: …` prose
comment can never be mistaken for the executable line it's next to. Asserts: the status gate is
preserved; the magic-byte and size checks exist with their message content; exactly 5 `fetch` call sites
each carry all 4 args; each call site's expected type (pdfium×3 gzip, ffmpeg-win zip, ffmpeg-linux xz —
three different types, not one hardcoded); exactly 2 `verify_btbn_checksum` calls; the pdfium-attestation
gap is documented in-workflow; the macOS ffmpeg leg's out-of-scope note is present. Red-proofed each of
the 5 substantive assertions by editing the workflow file (removing the magic block, removing the size
block, removing one checksum call, reverting one call site to the old 2-arg form) and confirming the
exact corresponding test(s) failed and no others — then reverted every edit.

**Gates run** (superseded by a larger re-run after the Reviewer round — see the dated addendum below for
the current 15/15 and 4228 figures; kept here as the original snapshot):
- `bash -n` on the extracted `run:` script (both the full block and a functions-only extraction taken
  verbatim from the committed file): clean, both before and after the final edit.
- `npx vitest run src/lib/releaseSidecarDownloadBodyGuard.test.ts`: 10/10 passed.
- `npx vitest run` (full suite): 320 files / 4223 tests passed.
- `npm run check`: 0 errors, 0 warnings.
- `release-sidecar.yml` parses successfully under the repo's own bounded-subset YAML parser (the guard
  test's `parseWorkflow()` call parses the whole file, not just the edited step).

**AC status:**
- [x] 200-with-non-archive-body fails the guard, message names URL + expected-vs-got. Demonstrated live
      against `https://httpbin.org/html`.
- [x] All 5 currently-guarded downloads (pdfium×3, ffmpeg×2) get the check, with a per-call-site type
      (three distinct expected types, not one hardcoded).
- [x] A genuine live download still succeeds — demonstrated against the real pinned pdfium-linux-x64.tgz
      (by me) and the real pinned 111,679,252-byte ffmpeg-Linux archive (independently, by the PR's
      Reviewer); not slowed meaningfully (one `head -c`/`wc -c`/`od` pass over an already-downloaded
      local file, plus one small extra `checksums.sha256` fetch for the two ffmpeg sites).
- [x] Truncated-but-200 considered and stated explicitly: magic+size catches an early truncation; neither
      catches a late truncation/corruption with a valid-sized header. The checksum layer closes that gap
      for ffmpeg (BtbN) — but ONLY for corruption/truncation, NOT for tampering, because it's a
      same-origin/same-channel checksum with no independent trust anchor (corrected framing, below).
      pdfium gets no such third layer, but NOT because it's the weaker case: bblanchon publishes a
      Sigstore attestation (`pdfium-attestation.json`) with a genuine independent trust anchor — the
      STRONGER mechanism, currently UNUSED here (needs `gh attestation verify`/cosign — a heavier
      follow-up). ffmpeg's same-origin sha256 is the WEAKER of the two available mechanisms, and it's the
      one that's wired up. Also noted: GitHub release assets are mutable within a pinned tag — the tag
      pin alone doesn't pin content, for either upstream.
- [x] macOS ffmpeg leg (git-clone-and-build) confirmed out of scope, in writing, both in the workflow
      comment and here.

PR: (see branch `cpe-1764-download-body-guard`). Scope held to body validation only — did not touch
`brew`/`choco`/`curl --max-time` (CPE-1824's territory) or any `apt-get` site (CPE-1787, merged).

---

**2026-08-20 (same day) — Worker, addressing PR #977's Reviewer round**

The Foreman relayed the Reviewer's findings. What it independently verified and cleared: every magic
constant against the live pinned assets (including the two zip end-of-central-directory variants,
`50 4b 05 06`/`50 4b 07 08`, correctly NOT what BtbN produces for a 145MB build); the 64KiB floor pinned
by the `calls.length === 5` assertion and applied via the shared `fetch()` to all five sites; fail-closed
across the whole matrix (wrong content, unknown asset, and a bogus tag so `checksums.sha256` 404s — all
`exit 1`); and a full positive-path run against the real pinned 111,679,252-byte ffmpeg-Linux archive
(rc=0). One blocker and two elevated follow-ups came back:

**Blocker — overclaimed tamper resistance, and backwards framing.** Fixed both:
- The shipped error text (checksum-mismatch message) no longer says "tampered in transit"; it now says
  the body differs from what BtbN currently publishes — corruption, truncation, or a substitution that
  didn't also rewrite the checksum — and adds "(this is NOT tamper resistance - see the comment above)".
- The `verify_btbn_checksum` comment block (`.github/workflows/release-sidecar.yml`) now states
  explicitly: the checksum is fetched from the SAME release, SAME origin, SAME TLS channel, SAME `curl`
  as the artifact — no independent trust anchor — and the real authenticity upgrade is pinning hashes IN
  THIS REPO, not fetching them from upstream. Also states GitHub release ASSETS are mutable within a
  pinned tag, so the tag pin alone doesn't pin content.
- **Framing corrected, both in-workflow and in this Work Log (above):** pdfium's Sigstore attestation
  (`pdfium-attestation.json`, Fulcio-issued cert, confirmed live by the Reviewer — 18,215 bytes) has a
  genuine independent trust anchor and is the STRONGER mechanism — currently unused. BtbN's same-origin
  sha256 is the WEAKER of the two available mechanisms, and it's the one that's wired up. The original
  Work Log had this backwards (framed pdfium as the weaker case); corrected in place above rather than
  left standing next to a contradicting addendum.

**Elevated from follow-up, fixed both:**
- **A — the guard test greenlit removing every `exit 1`.** Added a `lineImmediatelyAfter(marker)` helper
  to `src/lib/releaseSidecarDownloadBodyGuard.test.ts` and one test pinning all six substantive guard
  blocks (the original CPE-1762 status check plus the five new CPE-1764/checksum blocks) to actually
  `exit 1` immediately after their error output, not just print it. Red-proofed by doing exactly what the
  Reviewer did: replaced all six `exit 1` lines with `echo "::warning::continuing anyway"` — the new test
  failed (others stayed green); reverted, green again.
  Also fixed the second hole in the same file: the Windows/Linux ffmpeg call-site lookups keyed off the
  local `out` filename (`c.out === "ffmpeg.zip"` / `"ffmpeg.tar.xz"`), not the URL. Added `resolveArg()`,
  which chases this script's own `VAR="..."` assignment chain (`$FFMPEG_LINUX_URL` →
  `${FFMPEG_LINUX_ASSET}` → the literal `...-lgpl-8.1.tar.xz` suffix) so the lookup is keyed on the
  actual asset being downloaded. Red-proofed by editing `FFMPEG_LINUX_ASSET`'s literal suffix from
  `.tar.xz` to `.zip` (output filename left unchanged) — the Linux-ffmpeg-expects-xz test failed
  correctly; reverted, green again.
- **B — a missing `checksums.sha256` misdiagnosed as a missing entry.** `verify_btbn_checksum` now fetches
  `checksums.sha256` with its own `--write-out '%{http_code}'` status check (`$sums_code`), separate from
  the asset-lookup step. A non-200 (e.g. a bogus tag, confirmed live against
  `this-tag-does-not-exist-cpe1764-redproof` → real HTTP 404) now reports "couldn't fetch BtbN's
  checksums.sha256 (HTTP $sums_code)" and exits, rather than falling through to awk finding nothing and
  reporting "no checksum entry for `<asset>`". Verified live: the bogus-tag case now correctly reports
  the fetch failure; a real-tag-but-wrong-asset-name case still correctly reports "no checksum entry".
  Both paths still fail closed (rule 5 holds), the first line just now points at the right cause.

**Stale figure fixed:** the vague "ffmpeg ≈106–138MB" was replaced with the measured figures throughout
this Work Log — Windows zip 145,349,121 bytes, Linux tar.xz 111,679,252 bytes (the latter independently
confirmed by the Reviewer's own full download).

**Not mine:** `verify_btbn_checksum`'s `curl` (no `--max-time`, no `timeout-minutes` on the step) is
CPE-1824's territory per the Foreman — untouched here.

**Gates re-run after all of the above:**
- `bash -n` on the extracted `run:` script (fresh extraction, post-edit): clean.
- Live re-verification against the actual committed code: `https://httpbin.org/html` (200, HTML) still
  rejected with the same magic-byte message; the real pinned `pdfium-linux-x64.tgz` still accepted,
  byte-identical (3,650,783 bytes); `verify_btbn_checksum`'s real-network fetch-status path exercised
  live against both a bogus tag (404 → "couldn't fetch") and the real tag with a bogus asset name (200 →
  "no checksum entry"); a real-tag/real-published-asset-name-but-local-fixture-content case confirmed a
  genuine checksum mismatch against the real, live `checksums.sha256` (expected hash `089e4169e9…`
  matches the earlier-confirmed real value).
- `npx vitest run src/lib/releaseSidecarDownloadBodyGuard.test.ts`: **15/15 passed** (was 10; +5 new:
  the exit-1-pinning test, the checksum-fetch-status test, the no-overclaim test, the
  mutable-assets/no-trust-anchor test, and the corrected-framing test).
- `npx vitest run` (full suite): **320 files / 4228 tests passed** (was 4223; +5, matching the new tests
  above — no regressions elsewhere).
- `npm run check`: 0 errors, 0 warnings.
- Red-proofed both elevated fixes on the actual committed file (see A and B above), plus re-ran the
  original five red-proofs (magic, size, checksum-call-count, call-site-arg-count) — all still correctly
  isolate their one test, all reverted clean.

PR #977 updated on the same branch (`cpe-1764-download-body-guard`), pushed after this round.
