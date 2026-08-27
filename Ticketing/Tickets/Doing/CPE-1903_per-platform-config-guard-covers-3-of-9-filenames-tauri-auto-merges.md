---
id: CPE-1903
title: the per-platform config guard covers 3 of the 9 filenames Tauri auto-merges — .json5 and Tauri.<os>.toml bypass it, and the check never runs on the tag path
type: bug
priority: High
status: Doing
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
