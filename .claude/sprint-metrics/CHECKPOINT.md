# Sprint checkpoint — run `batched-2026-08-17-1929`

**Written 2026-08-20 10:00 local.** Batches **34 of 40**. Sub-agents used this session: **~143** of the
~150 reset line — this is the quiesce-and-hand-off boundary, not a stop. A fresh session resumes under the
same count by reading `BATCH-COUNTER`.

## Standing user instruction (must survive the reset)

> "When you come towards the end of these batches please build, deploy and run the application before we
> start the next set of batches."

**Done once already** for v0.57.67-sidecar (built 3-OS green, published, installed, launched, verified).
**Do it again once CPE-1804 merges** — that ticket changes user-visible behaviour. Bump to **0.57.68**.

**Versioning is FIVE files, not three** (CLAUDE.md was wrong and is now corrected): `package.json`,
`src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `package-lock.json` (**two** version fields), and
`src-tauri/Cargo.lock` (the `cross-platform-explorer` entry). Neither build passes `--locked`, so a stale
lockfile never errors — it leaks out as a dirty working tree. `package-lock.json` had been three releases
behind when this was found.

Deploy sequence, in order, no step skipped:
1. Confirm the draft release **carries installer assets** before publishing — an empty draft means the
   build failed, and publishing it creates a broken public release.
2. Kill **every** `cpe` / `ai-console` process **including `--session-daemon`**. NSIS silently skips a
   file-locked sidecar and the registry version then lies.
3. Install silently, then verify **both** the registry version **and** the sidecar exe timestamps.
4. Launch, confirm responding.
5. WebView2 cache survives reinstall — a stale `index.html` can make a real frontend fix look broken.

## Open PRs

| PR | Ticket(s) | State |
|----|-----------|-------|
| **#961** | CPE-1806 | **APPROVED** (reviewer ×2 + UAT PASS). Head `ffd7dcd4`, mid-CI. **Merge on green as batch 35.** Approval carries a standing condition: `Backend (ubuntu-latest)` must be green. |
| **#962** | CPE-1804 **+** CPE-1805 | Reviewer APPROVE, UAT PASS, but **RED on ubuntu** and reworking. Banks as **two** batches (36 and 37) since it closes two tickets. |

### #962's failure — the important context
Two new tests panic inside `trash-5.2.6/src/freedesktop.rs:350` — the very dependency panic CPE-1791
exists for. Fabricated `TrashItem`s reach `trash::os_limited::metadata` via `trash_item_to_entry:2240`;
harmless on Windows, fatal on Linux. The reviewer had flagged "the new tests touch no OS trash" as a
**wording** correction; that false belief was the reason the author never looked.

**Do not accept a `#[cfg(not(target_os = "linux"))]` fix** — that trades a visible red for a silent hole on
the only platform where the underlying bug is real, which is precisely what CPE-1806 is fixing.

Also outstanding on #962: pin that both commands route through `listing_is_degraded`; remove the dependency
on the machine's ambient Recycle Bin contents; the UAT's F1 (hardcode `skipped: 0` in both commands and all
253 Rust tests still pass — the walker→command seam is unpinned); and two evidence corrections, noting red
-proof row 2's red count was understated **upward** (three, not one).

## Remaining work for batches 38–40

Backlog is **46**. Named candidates, best first: **CPE-1802** (ffmpeg override window — `.github/workflows`
is free now), **CPE-1813** (TAR still does not deliver ZIP's no-link-support refusal), **CPE-1814**
(dead `Skip|Abort` collapse + staging-failure `return` + dangling cfg-gated doc links + unqualified
taxonomy line), **CPE-1810** (`--warn` is not a theme token), **CPE-1811** (two falsified S3 doc comments),
**CPE-1817**, **CPE-1815**, **CPE-1816**.

## Process rules learned this session — carry them forward

- **Never `git add -A`.** Mine swept a sub-agent's stray `scratchpad_clippy_default.log` into an unrelated
  ticket commit. Worktree-isolated agents still occasionally write into the main working copy. Stage
  explicit paths.
- **"0 failing" is not green.** It has meant "zero checks registered" once tonight. Always verify the
  rollup is against the exact head SHA.
- **Green means the tests that exist passed**, not that review findings were addressed. One PR went fully
  green with its blocking finding unfixed.
- **PowerShell corrupts repo files** — it BOMs/re-encodes, and fabricates a BOM even when *reading* through
  `>`. Use Edit/Write or python; check `git diff --numstat`.
- The dominant defect class this sprint was **a claim reading stronger than its evidence** — nine candidate
  cannot-fail tests (eight real), a false premise that survived two tickets, four rounds of true code with
  false prose. The question that found nearly all of them: *what does this test fail for, specifically?*
