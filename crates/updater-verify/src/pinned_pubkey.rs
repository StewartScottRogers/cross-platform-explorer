//! CPE-1873 — a second (and third — see below), reviewed pin for the updater's root-of-trust
//! public key and update endpoints.
//!
//! # The problem this exists to close
//!
//! `verify-release-artifacts` (CPE-1058) reads its "expected" pubkey straight out of
//! `src-tauri/tauri.conf.json` — **the same commit** that produces the release artifact and defines
//! the workflow that checks it. That proves the manifest's signatures are internally *consistent*
//! with whatever pubkey ships in that build; it proves nothing about *authenticity* against the key
//! users already trust. A commit that swaps `tauri.conf.json`'s pubkey for an attacker-generated one
//! and signs the manifest with the matching private key sails through that check at `EXIT=0` — see
//! CPE-1873 for the auditor's reproduction (scenario 10).
//!
//! # What this file does, and — precisely — where each check actually runs
//!
//! [`EXPECTED_TAURI_UPDATER_PUBKEY`] and [`EXPECTED_TAURI_UPDATER_ENDPOINTS`] are **literal copies**
//! of the values that belong at `plugins.updater.pubkey` / `.endpoints` in the **base**
//! `src-tauri/tauri.conf.json`, committed in a location that is not that file — and, since CPE-1987,
//! they are also **the one source these values are pinned from**: the TypeScript guard reads them out
//! of this file at run time rather than carrying its own copy. Pinning `endpoints`
//! too matters on its own: a signature check alone doesn't stop a *downgrade* — repointing where the
//! app fetches `latest.json` from can serve an older, genuinely-signed, vulnerable build forever.
//!
//! Two independent things read these constants, and they cover different paths:
//!
//! - `tests/pinned_pubkey_guard.rs` (`#[test]`) — reads the **base** config and runs wherever
//!   `cargo test -p cpe-updater-verify` runs: `ci.yml`'s "updater-verify — clippy + test" step, on
//!   every push/PR to `main`. **It does NOT run on the tag-push path** — `ci.yml` has no `tags:`
//!   trigger, so a tag pointed at a commit that never reached `main` (or one CI never evaluated)
//!   never executes this test.
//! - `verify-release-artifacts` (the binary next to this crate) — now also checks the **base**
//!   config against these same constants, before it does anything else. This binary is what
//!   `release.yml`'s `verify-published-manifest` job actually runs on every `v*` tag push (see that
//!   file, and this crate's own doc comment on the binary), so the pin is enforced on the plain
//!   release's tag path too, independent of whether the tagged commit's history ever touched `main`.
//!
//! **Neither of the above ever reads an overlay file.** The build every install actually ships is
//! `release-sidecar.yml`'s: the base config with a chain of `--config` overlays applied on top
//! (that chain is DERIVED from the workflow's own `args:` line by `src/lib/tauriConfigChain.ts` and
//! merged by `src/lib/sidecarBundleResources.test.ts` — CPE-1900), and Tauri's own `--config` merge
//! (RFC 7386 recursive merge) lets an overlay override `plugins.updater.pubkey`/`.endpoints` the same
//! way it can override `bundle.resources` — the exact footgun CPE-1270/1271 already documented for
//! that key, now independently demonstrated for this one (CPE-1873 attempt 2, Security Auditor: one
//! added line in `tauri.sidecar.conf.json`, base file untouched, both checks above still green).
//! `src/lib/sidecarBundleResources.test.ts` closes that: it computes the full merged config per
//! shipped OS from the real `--config` overlay chain and asserts `plugins.updater.pubkey`/`.endpoints`
//! still equal the pin. Note the scope: that guard sees the `--config` chain, and *only* the
//! `--config` chain — the files Tauri merges on its own are the next paragraph's business.
//!
//! **That guard used to hold its own copy of these two values, under a comment asking the reader to
//! keep them in lockstep with the consts below. CPE-1987 deleted the copy**: it now reads
//! [`EXPECTED_TAURI_UPDATER_PUBKEY`] / [`EXPECTED_TAURI_UPDATER_ENDPOINTS`] out of THIS FILE at run
//! time (`src/lib/rustSource.ts`, comments stripped first, so a commented-out old value cannot be
//! read as the live one). Two consequences worth knowing before you edit either side:
//!
//! - **Renaming or restructuring these two consts breaks that test.** It does not go quietly green —
//!   the reader throws at collection with `anchor not found in Rust source: pub const
//!   EXPECTED_TAURI_UPDATER_PUBKEY` and the whole file reports zero tests (measured 2026-08-28).
//!   Rename the anchor there in the same commit.
//! - **DO NOT ADD A SECOND DECLARATION OF EITHER NAME — not even a `#[cfg]`-gated one, and not even a
//!   longer name that merely starts with it** (`…_PUBKEY_LEGACY`). A text scan takes the FIRST
//!   occurrence where rustc takes the one its name resolution and `cfg`s select, and that gap was a
//!   demonstrated silent bypass (CPE-1987 SEC-1: overlay + `release.yml`'s matrix `args:` + one decoy
//!   const here → 74/74 passed, clippy clean, `cargo test -p cpe-updater-verify` 8/8 ok, attacker root
//!   of trust on all six shipped legs). The reader now **refuses a non-unique anchor** rather than
//!   guessing, so such a declaration reds loudly at collection instead — but it reds, so do not add
//!   one casually. The rule and the three shapes live at `uniqueAnchorIndex` in `rustSource.ts`.
//! - **What the deleted copy was worth, and what it cost.** It could never drift *silently* — it was
//!   compared against the real merged config, so a stale copy simply went red. What it did do was let
//!   an attacker who wrote a key into an *overlay* **and** into that literal hide it from the only
//!   guard that could see it, because nothing here reads an overlay. **Sized honestly** (PR #1108
//!   review, CLAIM-1 — the first write-up of this said "two files, six legs, nothing red" and that
//!   does not reproduce): the two-file version is **3 failed / 44 passed**, because `release.yml`'s
//!   plain channel takes no overlay and its three legs still red. The all-green shape needs a THIRD
//!   file, adding the overlay to `release.yml`'s matrix `args:`. Deriving closes it either way. In
//!   exchange, a rotation that edits `tauri.conf.json` and these consts together no longer trips a
//!   third stale copy — the same self-consistency limit "What none of this proves" below declares.
//! - **The net trade, recorded rather than left implicit:** the TypeScript pin now trusts this file's
//!   *text*, where the Rust legs trust the compiler — strictly weaker than an independent literal was
//!   independent. Still net positive, because it closes a three-file attack needing no Rust edit and
//!   forces that third edit into this file, which is the most-reviewed one in the repo — **but that is
//!   only true with the uniqueness refusal in place.**
//!
//! **A THIRD path, found independently of `--config` entirely (CPE-1873 attempt 3, Security Auditor,
//! DEMONSTRATED; widened by CPE-1903):** Tauri merges a per-platform config file AUTOMATICALLY on
//! every build, with no `--config` flag involved at all — `tauri-utils::config::parse::read_from`
//! reads `tauri.conf.json` and then merges a per-platform file from the same directory via RFC 7396,
//! unconditionally. A `src-tauri/tauri.windows.conf.json` carrying only a `plugins.updater` override
//! left EVERY guard described above green (base pin, merged-overlay-chain pin, the
//! `verify-release-artifacts` pin check) while shipping an attacker's root of trust on every Windows
//! build, plain channel and sidecar both.
//!
//! Attempt 3 closed that by hardcoding the three `.json` filenames. **That was still an enumeration,
//! and it was bypassed again**: Tauri's real surface is fifteen names — three formats
//! (`tauri.<t>.conf.json`, `tauri.<t>.conf.json5`, `Tauri.<t>.toml`) across five `Target` variants —
//! and both non-`.json` formats were demonstrated ingesting an attacker config through this repo's own
//! installed `@tauri-apps/cli`. **CPE-1903 replaced the list with a derivation**: see
//! [`crate::platform_config_guard`], which scans `src-tauri/` and classifies what is actually there by
//! *shape*, closing the format, casing and next-filename-nobody-thought-of problems together. It runs
//! as a `#[test]` (`tests/platform_config_guard.rs`, PR/push + the sidecar channel's
//! `verify-updater-pin` gate), in `sidecarBundleResources.test.ts` (the TypeScript mirror), **and
//! inside the `verify-release-artifacts` binary**, so unlike attempt 3's version it reaches
//! `release.yml`'s tag path too.
//!
//! # What none of this proves
//!
//! Every check described above compares two (or more) files read from the **same commit/checkout**.
//! A rotation that updates `tauri.conf.json` (and any overlay) and every pin together in one commit
//! is perfectly self-consistent and passes all of them — nothing here consults a value that lives
//! **outside** the tagged commit (a repo secret, an org variable, a previously published release, a
//! signed history). This is defence-in-depth against a compromised token or a careless/mistaken
//! rotation (see CPE-1873's "Why High" section for that scope), turning a one-line, easy-to-miss
//! config edit into a multi-file diff a normal PR review actually surfaces — **not** a guarantee
//! against an attacker with unrestricted push access who edits the config and every pin together. The
//! ticket's own option 2 — source the expected pubkey from OUTSIDE the tree (a repo secret / org
//! variable the tagged commit cannot itself supply) — is the version that would actually deliver
//! authenticity rather than self-consistency, and is intentionally not built here; see the ticket's
//! Work Log for why and what it would take.
//!
//! # Key-rotation procedure (the deliberate path through these guards)
//!
//! A legitimate rotation is a **single PR** that changes ALL of the following together, with the
//! reason stated in the PR description and in a Work Log entry on the ticket that authorized it:
//!
//! 1. Generate the new keypair (see README.md "Auto-updates — one-time setup" for the
//!    non-interactive invocation — `tauri signer generate` prompts for a password by default, which
//!    hangs in a non-interactive shell). Never commit `updater.key` / `updater.key.pub` — gitignored.
//! 2. Update `src-tauri/tauri.conf.json` → `plugins.updater.pubkey` (and `.endpoints`, if it's also
//!    changing) to the new value. If any overlay under `src-tauri/*.conf.json` sets either key too,
//!    update it there as well — check with `grep -rn '"pubkey"\|"endpoints"' src-tauri/*.conf.json`.
//! 3. Update [`EXPECTED_TAURI_UPDATER_PUBKEY`] / [`EXPECTED_TAURI_UPDATER_ENDPOINTS`] below to match,
//!    in the same commit.
//! 4. Nothing to do for `src/lib/sidecarBundleResources.test.ts` — it READS step 3's consts (CPE-1987),
//!    so it follows automatically. It is still the check that fails first if step 2 and step 3
//!    disagree, or if any overlay sets a different value; it just no longer needs editing.
//! 5. Rotate the matching GitHub Actions secrets (`TAURI_SIGNING_PRIVATE_KEY`,
//!    `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) — done outside the repo, by whoever holds the private key.
//! 6. Open the PR normally. A reviewer sees a multi-file diff explaining *why* the root of trust is
//!    changing, rather than a silent one-line edit to a config blob.
//!
//! If any of the guards above go red and you did **not** just do the above on purpose: stop. Treat it
//! the same as any other unexplained change to signing material — do not "fix" it by copying the new
//! live value into the pin to make the check pass; find out why the config changed first.

/// The pubkey `src-tauri/tauri.conf.json` → `plugins.updater.pubkey` is expected to carry. See the
/// module doc above for what this proves, what it doesn't, where it's checked, and the rotation
/// procedure.
pub const EXPECTED_TAURI_UPDATER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDUyMUU1NzRGNjhFMjU2MUEKUldRYVZ1Sm9UMWNlVXYvc283NmRaeHVhYkQrNGpQKzZ5aitWL1ErWWRxUGFWRXlQdXJDTkNENG4K";

/// The `endpoints` array `src-tauri/tauri.conf.json` → `plugins.updater.endpoints` is expected to
/// carry (order-sensitive — compared as-is, not as a set). Pinned alongside the pubkey because
/// repointing this can silently downgrade users to an older, genuinely-signed, vulnerable build
/// forever, even with the pubkey pin fully intact. See the module doc above.
pub const EXPECTED_TAURI_UPDATER_ENDPOINTS: &[&str] = &[
    "https://github.com/StewartScottRogers/cross-platform-explorer/releases/latest/download/latest.json",
];
