//! Generator for the gui-smoke `.cpevault` fixture (CPE-1249, epic CPE-738).
//!
//! Prints (base64, stdout) a REAL `.cpevault` blob — the exact envelope + `age` passphrase format the
//! production backend produces and reads (`vault_crypto`) — so `gui-smoke/wdio.conf.ts#seedVaultFixture`
//! can decode it into the smoke test's tmpDir and drive the real unlock→browse→lock flow against a
//! genuine vault (no mocked backend). Deliberately uses a **low scrypt work factor** so the smoke test's
//! unlock is fast on a CI runner; that only weakens a throwaway fixture whose passphrase is public here.
//!
//! Contents sealed (the tree that appears once the vault is unlocked):
//!   CPE-1249-inside.txt         "top secret vault contents\n"
//!   notes/                      (directory)
//!   notes/CPE-1249-hello.txt    "hello from inside the vault\n"
//!
//! Regenerate with:
//!   cargo run -p cpe-server --example gen_vault_fixture
//! then paste the printed base64 into `VAULT_FIXTURE_BASE64` in gui-smoke/wdio.conf.ts. The passphrase
//! is `VAULT_FIXTURE_PASSPHRASE` there — keep both in sync with the constants below.

use std::io::Write;

use age::secrecy::SecretString;
use base64::Engine;
use cpe_server::vault_crypto::{pack_entries, EntryKind, TreeEntry, MAGIC, SCHEMA_VERSION};

/// Must match `VAULT_FIXTURE_PASSPHRASE` in gui-smoke/wdio.conf.ts — and, since CPE-1950, that is a
/// *checked* fact rather than a note: `src/lib/guiSmokeFixtureLiterals.test.ts` reads this constant
/// out of this file (comments stripped) and `VAULT_FIXTURE_PASSPHRASE` out of `wdio.conf.ts`, and
/// compares them on every PR. Same for the first sealed entry's name below vs
/// `VAULT_FIXTURE_INNER_NAME`. A Rust example cannot import a TypeScript module, so a text derivation
/// is the honest form here; the alternative was leaving a "must match" claim nothing could falsify.
const PASSPHRASE: &str = "open-sesame-1249";
/// A deliberately-low scrypt log2(N): trivially cheap to decrypt (this is a throwaway fixture), well
/// under the backend's accepted `MAX_WORK_FACTOR` of 22.
const WORK_FACTOR: u8 = 4;

fn main() {
    let entries = vec![
        TreeEntry {
            path: "CPE-1249-inside.txt".to_owned(),
            kind: EntryKind::File,
            data: b"top secret vault contents\n".to_vec(),
        },
        TreeEntry {
            path: "notes".to_owned(),
            kind: EntryKind::Dir,
            data: Vec::new(),
        },
        TreeEntry {
            path: "notes/CPE-1249-hello.txt".to_owned(),
            kind: EntryKind::File,
            data: b"hello from inside the vault\n".to_vec(),
        },
    ];
    let plaintext = pack_entries(&entries);

    let passphrase = SecretString::from(PASSPHRASE.to_owned());
    let mut recipient = age::scrypt::Recipient::new(passphrase);
    recipient.set_work_factor(WORK_FACTOR);
    let encryptor = age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
        .expect("a single scrypt recipient is always valid");

    // Prepend the same outer envelope production uses (MAGIC || schema_version || age ciphertext).
    let mut blob = Vec::new();
    blob.extend_from_slice(MAGIC);
    blob.extend_from_slice(&SCHEMA_VERSION.to_le_bytes());
    let mut writer = encryptor.wrap_output(&mut blob).expect("wrap_output");
    writer.write_all(&plaintext).expect("write plaintext");
    writer.finish().expect("finish");

    println!("{}", base64::engine::general_purpose::STANDARD.encode(&blob));
}
