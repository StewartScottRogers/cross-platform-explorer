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
