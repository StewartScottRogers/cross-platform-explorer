//! CPE-1873 — the second, reviewed pin for the updater's root-of-trust public key.
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
//! # What this file does
//!
//! [`EXPECTED_TAURI_UPDATER_PUBKEY`] is a **literal copy** of the value that belongs at
//! `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`, committed in a location that is not
//! `tauri.conf.json` itself. `tests/pinned_pubkey_guard.rs` reads the live config on every CI run
//! (`ci.yml`'s "updater-verify — clippy + test" step, which runs on every push/PR to `main`, not just
//! on a tag) and fails loudly the instant the two disagree.
//!
//! This does **not** stop someone with push access from editing both this file and
//! `tauri.conf.json` in the same commit — nothing living inside the repository can, since the
//! tagged commit still supplies both the workflow and its own input either way. What it *does* do is
//! turn a one-line, easy-to-miss `tauri.conf.json` diff into a **two-file diff that is impossible to
//! satisfy by accident or by touching only one file** — a normal PR review, or a glance at
//! `git show`, now surfaces a pubkey change instead of burying it in a JSON blob nobody reads
//! character-by-character. That is the defence-in-depth this ticket scoped: protection against a
//! compromised token or a careless/mistaken rotation, not a fully malicious insider with unrestricted
//! push access (see CPE-1873's "Why High" section for that boundary spelled out).
//!
//! # Key-rotation procedure (the deliberate path through this guard)
//!
//! A legitimate rotation is a **single PR** that changes BOTH of these together, with the reason
//! stated in the PR description and in a Work Log entry on the ticket that authorized it:
//!
//! 1. Generate the new keypair: `npm run tauri signer generate -- -w ./updater.key` (see README.md
//!    "Auto-updates — one-time setup"). Never commit `updater.key` — it's gitignored.
//! 2. Update `src-tauri/tauri.conf.json` → `plugins.updater.pubkey` to the new **public** key.
//! 3. Update [`EXPECTED_TAURI_UPDATER_PUBKEY`] below to the identical string, in the same commit.
//! 4. Rotate the matching GitHub Actions secrets (`TAURI_SIGNING_PRIVATE_KEY`,
//!    `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) — done outside the repo, by whoever holds the private key.
//! 5. Open the PR normally. A reviewer sees a two-file diff explaining *why* the root of trust is
//!    changing, rather than a silent one-line edit to a config blob.
//!
//! If `tests/pinned_pubkey_guard.rs` goes red and you did **not** just do the above on purpose: stop.
//! Treat it the same as any other unexplained change to signing material — do not "fix" it by copying
//! the new live value into this file to make the test pass; find out why `tauri.conf.json` changed
//! first.

/// The pubkey `src-tauri/tauri.conf.json` → `plugins.updater.pubkey` is expected to carry. See the
/// module doc above for what this proves, what it doesn't, and the rotation procedure.
pub const EXPECTED_TAURI_UPDATER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDUyMUU1NzRGNjhFMjU2MUEKUldRYVZ1Sm9UMWNlVXYvc283NmRaeHVhYkQrNGpQKzZ5aitWL1ErWWRxUGFWRXlQdXJDTkNENG4K";
