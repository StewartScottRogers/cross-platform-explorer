---
id: CPE-1903
title: the per-platform config guard covers 3 of the 9 filenames Tauri auto-merges — .json5 and Tauri.<os>.toml bypass it, and the check never runs on the tag path
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-26
---

## Summary

CPE-1873 closed three successive ways to override the updater's root of trust. This is the fourth, and
it is the same mechanism reached through variants nobody had enumerated.

Tauri's per-platform config is **not one filename per OS — it is three formats per OS.**
`tauri-utils::config::parse::ConfigFormat::into_platform_file_name` enumerates `.json`, `.json5` and
`Tauri.<os>.toml`; `does_supported_file_name_exist` returns true if **any enabled format's** platform
file exists, and `do_parse` falls through `path → json5 → toml`. CPE-1873's guard hardcodes only the
three `.json` names.

**Measured, each file alone, both guards:**

| attacker file | `cargo test --test pinned_pubkey_guard` | `vitest sidecarBundleResources` | |
|---|---|---|---|
| `tauri.windows.conf.json5` | exit 0 | 13/13 passed | **BYPASS** |
| `tauri.macos.conf.json5` | exit 0 | 13/13 passed | **BYPASS** |
| `tauri.linux.conf.json5` | exit 0 | 13/13 passed | **BYPASS** |
| `Tauri.windows.toml` | exit 0 | 13/13 passed | **BYPASS** |
| `Tauri.macos.toml` | exit 0 | 13/13 passed | **BYPASS** |
| `Tauri.linux.toml` | exit 0 | 13/13 passed | **BYPASS** |

**These formats are live in the CLI this repo actually builds with.** `ENABLED_FORMATS` is
cargo-feature-gated, so it was not assumed — it was measured against `@tauri-apps/cli 2.11.4` from this
repo's own `node_modules`:

    src-tauri/tauri.windows.conf.json5   →  npx tauri info  →  frontendDist: ../PWNED-BY-JSON5-PLATFORM-CONFIG
    src-tauri/Tauri.windows.toml         →  npx tauri info  →  frontendDist: ../PWNED-BY-TOML-PLATFORM-CONFIG

Both merged with **no `--config` flag**. Then composed: one `tauri.windows.conf.json5` carrying the
attacker `plugins.updater` block *and* an observable marker — the real CLI ingested the file, updater
block included, while `cargo test` exited 0 and vitest reported 13/13.

**Precondition:** land one new file. Arguably easier to slip past a reviewer than the `.json` variant,
because `Tauri.windows.toml` does not look like a sibling of `tauri.conf.json` at all.

**Consequence:** every Windows build, plain channel and sidecar both, ships an attacker-controlled
updater root of trust and update endpoint, with every guard green.

## Two more gaps in the same check

**1. The check is `#[test]`-only, so it never runs on the plain tag path.**
`no_automatic_per_platform_config_overrides_the_updater_pin` exists only in
`tests/pinned_pubkey_guard.rs` and `sidecarBundleResources.test.ts` — **not** in
`verify-release-artifacts.rs`. That is exactly the lesson of CPE-1873's round 1: a `#[test]` does not
reach `release.yml`'s tag path, which is why the base pin was moved into the binary. The move was not
carried over to this check. So on the plain channel's tag path, even the `.json` variant is unguarded —
and the live manifest's `windows-x86_64` entry points at the *plain* build's MSI (see CPE-1894), so that
channel is not hypothetical.

**2. Case sensitivity is host-dependent, and the gate runs on the wrong host.**
`Tauri.Windows.Conf.json` was caught on this machine — but only because NTFS lookups are
case-insensitive. `verify-updater-pin` is `runs-on: ubuntu-latest`, where Rust's `read_to_string` and
Node's `existsSync` are byte-exact: the file would be invisible to both guards there, while the Windows
build runner's NTFS would still merge it. **Flagged as reasoned from certain API semantics, not
demonstrated** — the auditor could not execute that leg locally, and said so.

## Acceptance criteria

- [ ] **Scan the directory rather than enumerate filenames.** `readdir(src-tauri)` and lowercase each
      entry, then match the per-platform pattern. This closes the format, case, and
      future-filename problems in one stroke and is less code than the current loop. Do not extend the
      hardcoded list from 3 names to 9 — that is the same mistake with a bigger number, and this ticket
      exists because enumeration has now failed three times on this exact surface.
- [ ] Move the check into `verify-release-artifacts` so the tag path gets it, keeping the `#[test]` as
      the fast PR-time signal. Mirror it on the frontend side.
- [ ] Red-proof every format and both cases: `.json`, `.json5`, `Tauri.<os>.toml`, for all three
      desktop OSes, plus at least one mixed-case spelling — and verify the case leg on a
      **case-sensitive** filesystem, not only on NTFS, since that is precisely where the current check
      would fail silently.
- [ ] Keep the semantics that were judged correct: refuse on the **presence of a `plugins.updater`
      key**, not on the file's existence and not on a value comparison. A per-platform file carrying
      only `plugins.cli` must stay allowed; `{"plugins":{"updater":null}}` — an RFC 7396 delete — must
      still be refused, as it currently is.
- [ ] Re-run CPE-1873's other closures afterwards to confirm no regression: the `--config` overlay
      injection must still red on all three OSes for both pubkey and endpoints.

## Notes

Filed 2026-08-26 by CPE-1873's independent Security Auditor, round 3, which recommended filing rather
than a fourth attempt: PR #1028 is strictly better than `main` and strictly better than its own attempt
2, closes the reproduction it was handed on both sides with the right semantics, and the remaining
surface needs a scoped pass rather than a rushed one against an attempt cap.

Worth stating for whoever picks this up, because it is the actual lesson of the whole CPE-1873 arc:
**three successive rounds each closed the reproduction and not the mechanism.** Round 1 pinned the base
config; round 2 found the `--config` overlays; round 3 found the automatic per-platform `.json`; this
round found the other six formats. Every round's fix was correct and every round's fix was an
enumeration. The durable answer is to derive what the shipped app's config actually is, rather than to
list the places it might come from.

Related: **CPE-1873** (the pin), **CPE-1900** (`CONFIG_CHAIN` is a hand-copied literal — same root
cause, a copy instead of a derivation), **CPE-1901** (`--skip-pin-check` and the secret-gated step),
**CPE-1894** (the live manifest mixing plain and sidecar assets, which is why the plain channel's tag
path matters here).


## Work Log

- **2026-08-26 (Worker)** — Picked up; `git mv` Backlog -> Doing as the first commit. Round 4 of the
  CPE-1873 arc, and the brief was explicit that the answer is a **derivation, not a longer list**.

  **What shipped.**
  - `crates/updater-verify/src/platform_config_guard.rs` (**new**) — the derivation, as a library
    module so the binary and the tests share one implementation.
    - `scan_for_platform_config_updater_overrides(dir)` calls `read_dir` and classifies what is
      actually there. **No name lookup anywhere**, which is what makes it behave identically on a
      case-insensitive NTFS build runner and on the byte-exact `ubuntu-latest` host where
      `verify-updater-pin` runs. Round 3's `read_to_string(dir.join("tauri.windows.conf.json"))` was a
      lookup, and therefore silently blind on exactly that host.
    - `is_auto_merged_platform_config_name(name)` ASCII-lowercases and matches the *shape* Tauri
      derives its names from — `tauri` `.` `<platform token>` `.` `<tail>`. That is all 15 of today's
      filenames (3 formats x 5 `Target` variants, verified against tauri-utils 2.9.3's
      `ConfigFormat::into_platform_file_name` in this machine's cargo registry), every casing, and any
      format Tauri adds later. `tauri.conf.json` / `Tauri.toml` and every `tauri.sidecar.*` overlay are
      deliberately NOT matched — their second segment is not a platform token, which is exactly the
      property Tauri itself keys on.
    - `platform_config_updater_refusal(text)` keeps the semantics CPE-1873 round 3 got right:
      refusal on the **presence of a `plugins.updater` key**, never on the file's existence and never
      by value comparison. `plugins.cli`-only stays allowed; `{"plugins":{"updater":null}}` (an RFC
      7396 delete) is still refused. Strict JSON is inspected structurally; formats this crate carries
      no parser for (JSON5, TOML — **no new dependencies** were added) get a conservative textual scan
      that fails closed on an `updater` token or on backslash escapes that could spell one.
    - 13 unit tests, including one that plants all 15 real filenames plus one innocent file in a temp
      dir and asserts 14 refusals, and one that plants `Tauri.Windows.Conf.JSON`. On `ubuntu-latest`
      those execute against a genuinely case-sensitive filesystem with real files on disk.
  - `crates/updater-verify/src/bin/verify-release-artifacts.rs` — runs the same library function on the
    directory holding its `--conf`. This is the binary `release.yml`'s tag-triggered
    `verify-published-manifest` job invokes, so **the tag path now gets the check** — CPE-1873 round
    1's lesson, which round 3 never carried over to this particular guard. Deliberately placed
    **outside** the `--skip-pin-check` block: that flag exists because this crate's fixtures scaffold
    throwaway keypairs unrelated to the pinned *values*, and this check compares no values and reads no
    constants, so the rationale does not reach it (this narrows CPE-1901's kill switch without
    stepping on that ticket). A `--conf` directory that cannot be listed is a hard failure, not "clean".
  - `crates/updater-verify/tests/platform_config_guard.rs` (**new**) — the fast PR-time signal against
    the real `src-tauri/`. Split out of `pinned_pubkey_guard.rs`, whose old three-filename
    `no_automatic_per_platform_config_overrides_the_updater_pin` is deleted; that file's module doc now
    points here.
  - `src/lib/sidecarBundleResources.test.ts` — the TypeScript mirror: `readdirSync` + the same
    ASCII-fold shape match (`asciiLower` is hand-rolled so it matches Rust's `to_ascii_lowercase`
    exactly rather than JS's Unicode `toLowerCase`), the same refusal semantics, plus four tests that
    exercise the derivation directly. `existsSync` is no longer imported — the lookup is gone.
  - `.github/workflows/release-sidecar.yml` — `verify-updater-pin` ran `cargo test --test
    pinned_pubkey_guard`. Naming one test target is the same enumeration mistake in another costume: it
    skipped the crate's `--lib` unit tests (where this round's derivation tests live) and would skip any
    guard added in a new `tests/*.rs`. Now `cargo test --locked` for the whole crate, so a guard added
    tomorrow gates the sidecar channel without anyone remembering to edit that line.
  - `crates/updater-verify/src/pinned_pubkey.rs` + `README.md` — the "third path" narrative rewritten
    from "three `.json` filenames" to the derivation, with the honest note that attempt 3's fix was
    itself an enumeration that got bypassed. README's rotation section now states that a rotation must
    not arrive as a per-platform config file.

  **Red-proof — all 9 filenames, both cases, three legs each, every file planted and deleted
  individually.** Legs: (1) `cargo test --test platform_config_guard`, (2) the real
  `verify-release-artifacts` binary invoked the way `release.yml` invokes it, (3) `npx vitest run
  src/lib/sidecarBundleResources.test.ts`.

  | planted file | leg 1 | leg 2 | leg 3 |
  |---|---|---|---|
  | `tauri.windows.conf.json` | EXIT=101 | EXIT=1 | 1 failed |
  | `tauri.macos.conf.json` | EXIT=101 | EXIT=1 | 1 failed |
  | `tauri.linux.conf.json` | EXIT=101 | EXIT=1 | 1 failed |
  | `tauri.windows.conf.json5` | EXIT=101 | EXIT=1 | 1 failed |
  | `tauri.macos.conf.json5` | EXIT=101 | EXIT=1 | 1 failed |
  | `tauri.linux.conf.json5` | EXIT=101 | EXIT=1 | 1 failed |
  | `Tauri.windows.toml` | EXIT=101 | EXIT=1 | 1 failed |
  | `Tauri.macos.toml` | EXIT=101 | EXIT=1 | 1 failed |
  | `Tauri.linux.toml` | EXIT=101 | EXIT=1 | 1 failed |
  | `Tauri.Windows.Conf.JSON` (mixed case) | EXIT=101 | EXIT=1 | 1 failed |
  | `tauri.windows.conf.json` = `{"plugins":{"updater":null}}` | EXIT=101 | EXIT=1 | 1 failed |

  Every message named the offending file by its on-disk spelling. Two GREEN controls confirm the
  semantics were preserved rather than replaced by a blanket ban: `tauri.windows.conf.json` carrying
  only `plugins.cli` + `bundle.targets`, and `Tauri.windows.toml` carrying only `[plugins.cli]`, both
  left leg 1 and leg 3 fully green (leg 2 then failed further downstream on `no latest.json found`,
  which is the expected result of running the binary without a manifest and proves the CPE-1903 check
  passed and let it proceed).

  **The case leg, precisely.** The ticket asked for it on a case-sensitive filesystem. Two local
  routes were tried and both need machine-global changes this run is barred from making: `fsutil file
  setCaseSensitiveInfo … enable` returns `error: 0x00000005 Access is denied.` without elevation, and
  WSL's `/mnt/z` is DrvFs `case=off` (demonstrated — writing `Alpha.txt` then `alpha.txt` produced one
  file). So it is covered by construction plus CI instead, which is stronger than a one-off local run:
  the guard performs **no name lookup at all**, so filesystem case behaviour cannot reach it; the
  matcher's mixed-case cases are pure-function tests; and
  `scan_catches_a_mixed_case_spelling_on_any_filesystem` writes a real `Tauri.Windows.Conf.JSON` to a
  real temp directory and scans it — which on `ubuntu-latest`, the host that runs `verify-updater-pin`,
  executes on a byte-exact filesystem. The NTFS run above is the third leg of the same proof.

  **CPE-1873's other closures re-run afterwards; all still red, no regression.**
  - `--config` overlay chain, attacker pubkey injected into `tauri.sidecar.conf.json` alone: vitest
    `3 failed | 12 passed`, one failure per shipped OS (windows/linux/macos).
  - Same overlay, attacker endpoints: vitest `3 failed | 12 passed`, again all three OSes.
  - Base config pubkey rotated: `cargo test --test pinned_pubkey_guard` EXIT=101 with `SECURITY
    (CPE-1873): the updater's root-of-trust public key changed.`; the binary EXIT=1 with the matching
    message.
  - Base config endpoints repointed: same shape, EXIT=101 / EXIT=1, `the updater's manifest
    endpoint(s) changed.`
  - Restored via `git checkout --` after each; `git status --short` clean and vitest back to 15/15.

  No private key material was generated, printed or committed at any point — the attacker "pubkey" is
  a plain placeholder string, since none of these checks parses it as a key.

  **Verification.** `crates/updater-verify`: `cargo clippy --locked --all-targets -- -D warnings`
  clean; `cargo test --locked` **52/52** (34 lib + 2 pinned-pubkey + 1 platform-config + 15
  release_guard). Frontend: `npm run check` 0 errors / 0 warnings; `npx vitest run` (full suite) 4504
  passed, **2 pre-existing failures in `src/lib/msrvSync.test.ts`** — that is CPE-1902 (the MSRV guard
  is not CRLF-safe and fails spuriously on Windows), open in Backlog, untouched by this work and
  unrelated to it. No feature-gated build modes exist in this crate, and nothing outside
  `crates/updater-verify` + `src/lib/sidecarBundleResources.test.ts` + docs was touched.

  **The ceiling, stated plainly.** Unchanged from CPE-1873: every check here compares files read from
  the **same commit** and consults nothing outside it. A key rotated in the same commit as the pin
  passes all of them. What this round adds is that the rotation can no longer arrive through a filename
  the guard was not told about.

  **Not in scope, untouched:** CPE-1900 (`CONFIG_CHAIN` is a hand-copied literal with no drift guard)
  and CPE-1901 (`--skip-pin-check`, and the secret-gated step).

- **2026-08-26 (Worker) — attempt 2, PR #1034 came back SEC FINDINGS.** The independent Security
  Auditor confirmed the derivation itself held — it attacked the 15-name count (verified independently
  in `tauri-utils-2.9.3/src/platform.rs`), the shape match, the case argument, the textual JSON5/TOML
  scan, JSON-vs-JSON5 parser divergence (checked against this repo's own `@tauri-apps/cli 2.11.4`),
  trailing dots/spaces, 8.3 names, ADS, junctions, and could not get past any of them; all CPE-1873
  regressions stayed red. Its verdict: *"this is the first version of this guard I could not beat on
  its own terms."* Two findings remained, both about what the scan does with an entry once it has one.

  **Finding 8 (BLOCKING, DEMONSTRATED against the real CLI) — a symlink named like a per-platform
  config was skipped by both guards and followed by Tauri.** Two committed files: an innocuous
  `src-tauri/innocent-looking-resource.json` carrying the attacker `plugins.updater` block, and a
  symlink `src-tauri/tauri.linux.conf.json → innocent-looking-resource.json`. Both guards filtered on
  an lstat-shaped predicate *before* reading — Rust's `DirEntry::file_type()` and Node's
  `Dirent.isFile()` neither of which traverses — so the entry was dropped before the content check
  ever saw it, while Tauri's `read_platform` (`path.exists()` + `read_to_string`, both of which
  follow) merged the payload: `npx tauri info` reported `frontendDist: ../PWNED-VIA-SYMLINK` with
  `cargo test` at exit 0 and vitest 15/15. Git stages such a link as mode `120000`, so it materialises
  as a real symlink on the ubuntu and macOS runners — and `verify-updater-pin`, the job gating
  `release-sidecar`'s build/sign/publish matrix, is `runs-on: ubuntu-latest`, exactly where it is real
  and therefore was invisible. `tauri.linux.conf.json` / `tauri.macos.conf.json` are the tokens that
  matter on those legs, and the live manifest's `linux-x86_64` / `darwin-aarch64` entries point at
  sidecar assets, so those are real users on the real auto-update channel.

  The detail worth keeping: the module doc said *"A file this guard cannot read as text is a refusal,
  not a skip: failing closed is the only safe default on this surface."* The read **was** fail-closed.
  The file-type probe two lines above it was fail-**open** — `unwrap_or(false)` skipped on error too.
  The right default was stated and then not applied to the predicate gating it.

  **Fix (it deletes code):** no file-type probe at all, on either side. `read_to_string` /
  `readFileSync` follow links exactly as Tauri does, and a directory, junction or otherwise unreadable
  entry now becomes a refusal through the already-correct fail-closed branch instead of a silent skip.
  The TypeScript side gained the matching `try`/`catch` so its read has the same semantics.

  **Finding 9 (DEMONSTRATED) — `{"plugins":null}` deletes the updater block and was allowed.** A
  17-byte `src-tauri/tauri.linux.conf.json` containing exactly `{"plugins":null}`: `cargo test` exit 0,
  vitest 15/15, no refusal. The content rule was `json.pointer("/plugins/updater")`, which on a null
  `plugins` cannot index `updater` and returns `None` — but under RFC 7396 a `null` at `plugins`
  deletes the **entire** plugins block from the merged config, updater included. That is this guard's
  own stated principle one level up: it refuses `{"plugins":{"updater":null}}` precisely because a null
  deletes the block, and deleting the parent deletes the child. Lower severity — update *suppression*
  (the app ships with no updater config and silently stops receiving security fixes) rather than
  root-of-trust replacement — but it is the freeze/downgrade harm class the endpoints pin exists for,
  and it landed with every guard green.

  **Fix:** the refusal now applies at every level a null can reach — a non-object **root** (an RFC 7396
  patch that replaces the whole config), a non-object or null **`plugins`**, and the `plugins.updater`
  key itself.

  **Red-proof, both findings, all three legs, each fixture planted alone and deleted after.**

  | fixture | leg 1 `#[test]` | leg 2 tag-path binary | leg 3 vitest |
  |---|---|---|---|
  | symlink `tauri.linux.conf.json` → `innocent-looking-resource.json` | EXIT=101 | EXIT=1 | 1 failed of 16 |
  | `tauri.linux.conf.json` = `{"plugins":null}` | EXIT=101 | EXIT=1 | 1 failed of 16 |

  The symlink leg's message reads `` `tauri.linux.conf.json` exists and sets a `plugins.updater` key ``
  — i.e. the guard read *through* the link and parsed the target's JSON, which is the whole point. The
  null leg reads `` sets `plugins` to something other than an object … update suppression, which
  freezes every install on the build it already has ``.

  **Before/after on this host, to prove the fix is what changed the outcome.** With the same symlink
  fixture in place, the attempt-1 code (`git checkout 68898320 -- <the two files>`) gives `cargo-test
  EXIT=0 / vitest 15 passed` — the bypass reproduced — and the attempt-2 code gives `cargo-test
  EXIT=101 / vitest 1 failed | 15 passed`. Same shape for `{"plugins":null}`: attempt 1 EXIT=0 /
  15 passed, attempt 2 EXIT=101 / 1 failed. Also confirmed the fixture is what the auditor described:
  `git ls-files -s` reports `120000 c43befab… src-tauri/tauri.linux.conf.json`, so it does survive a
  commit as a real symlink.

  **Benign paths re-confirmed, unbroken.**
  - A **directory** named `tauri.linux.conf.json` is now a refusal through the fail-closed branch, as
    intended: `exists but could not be read as text (Access is denied. (os error 5))` on Windows,
    `(EISDIR: illegal operation on a directory, read)` on the Node side — a refusal, never a silent
    skip.
  - `tauri.windows.conf.json` with only `plugins.cli` + `bundle.targets`: leg 1 EXIT=0, leg 3 16/16.
  - `Tauri.windows.toml` with only `[plugins.cli]`: leg 1 EXIT=0, leg 3 16/16.

  **CPE-1873 regressions re-run after the fix; all still red.** Overlay chain with an attacker pubkey
  in `tauri.sidecar.conf.json`: vitest `3 failed | 13 passed`, one per shipped OS. Overlay endpoints:
  same, `3 failed | 13 passed`. Base-config pubkey rotated: `cargo test` EXIT=101 with `SECURITY
  (CPE-1873): the updater's root-of-trust public key changed.`, binary EXIT=1. Base-config endpoints
  repointed: EXIT=101 / EXIT=1, `the updater's manifest endpoint(s) changed.` Restored via `git
  checkout --` after each; clean tree, vitest back to 16/16.

  **A Linux-side execution was attempted and is honestly reported as NOT run.** WSL Ubuntu has cargo
  but no `cc` (no build-essential), so every `cargo test` there failed at `error: linker \`cc\` not
  found` — those exit-101s are build failures, not guard verdicts, and are not counted as evidence.
  Installing a toolchain is a machine-global change this run is barred from making. What stands
  instead: the fix is filesystem-agnostic *by construction* — there is no file-type probe left to
  behave differently, and `read_to_string`/`readFileSync` follow symlinks on every platform — and the
  new `scan_follows_a_symlink_named_like_a_platform_config` unit test builds a real symlink
  (`std::os::unix::fs::symlink` on Unix, `symlink_file` on Windows) and runs on the Linux and macOS CI
  legs, reporting rather than passing vacuously if the fixture cannot be created.

  **Verification, attempt 2.** `crates/updater-verify`: `cargo clippy --locked --all-targets -- -D
  warnings` clean; `cargo test --locked` **56/56** (38 lib + 2 pinned-pubkey + 1 platform-config + 15
  release_guard). Frontend: `npm run check` 0 errors / 0 warnings; `npx vitest run` (full suite) 4583
  passed, with **3 pre-existing Windows-only failures in two files that this work never touched** —
  2 in `src/lib/msrvSync.test.ts` (CPE-1902, open in Backlog: the MSRV guard is not CRLF-safe) and 1 in
  `src/lib/sprintStallControls.test.ts` (CPE-1880's `scripts/**/*.mjs` LF checkout guard, same Windows
  CRLF class). Both pass on the Linux runners. No private key material generated, printed or
  committed; every attacker file and the symlink deleted, `git status --short` clean.

  Branch had been rebased by the Foreman (merge commit `68898320`, keeping this PR's whole-crate
  `cargo test --locked` over `main`'s minimal `--locked` fix to the same line); pulled before starting.
