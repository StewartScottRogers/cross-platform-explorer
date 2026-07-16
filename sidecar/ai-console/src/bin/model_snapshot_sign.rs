//! CLI: build + sign the model-catalog snapshot for a release (CPE-450).
//!
//! Usage: `model-snapshot-sign <resellers-response-dir> <out-dir> <version>`
//!
//! `<resellers-response-dir>` holds one `<reseller>.json` per reseller — each the raw body of that
//! reseller's `/models` endpoint, saved as `<reseller-id>.json` (the `model-snapshot.yml` workflow
//! curls them into a `responses/` dir). Every file is normalized to the common `Model` shape and
//! collected into one [`ModelSnapshot`], which is then ed25519-signed over its canonical bytes.
//!
//! The 32-byte ed25519 signing seed (hex) is read from `CPE_CATALOG_SIGNING_KEY` — the SAME secret
//! that signs the agent catalog (`catalog-sign`); no new key. `SNAPSHOT_GENERATED_AT` optionally
//! supplies the RFC 3339 "generated at" stamp (the workflow sets it via `date -u`); absent, a
//! `unix:<secs>` fallback is used.
//!
//! Emits `<out-dir>/models-index.json` (the canonical snapshot bytes) + `models-index.json.sig`
//! (the detached hex signature), ready to upload as a workflow artifact. The output verifies under
//! the seed's public key — the same key already embedded in `CATALOG_TRUSTED_KEYS` (src-tauri).
//!
//! The seed is NEVER printed or logged; only counts, the content hash, and paths go to stdout.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ai_console::model_snapshot::{content_hash, sign_snapshot, snapshot_from_reseller_dir};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!(
            "usage: {} <resellers-response-dir> <out-dir> <version>\n\
             \n\
             Reads one <reseller>.json per reseller from <resellers-response-dir>, normalizes and\n\
             collects them into a signed snapshot, and writes models-index.json + .sig into <out-dir>.\n\
             The signing seed (32-byte ed25519, hex) comes from the CPE_CATALOG_SIGNING_KEY env var.",
            args.first().map(String::as_str).unwrap_or("model-snapshot-sign")
        );
        std::process::exit(2);
    }
    let responses = Path::new(&args[1]);
    let out = Path::new(&args[2]);
    let version: u64 = args[3].parse().unwrap_or_else(|_| {
        eprintln!("version must be a non-negative integer (got {:?})", args[3]);
        std::process::exit(2);
    });

    let key = std::env::var("CPE_CATALOG_SIGNING_KEY").unwrap_or_else(|_| {
        eprintln!("CPE_CATALOG_SIGNING_KEY (32-byte ed25519 seed, hex) is not set");
        std::process::exit(2);
    });

    // RFC 3339 stamp from the workflow (`date -u +%Y-%m-%dT%H:%M:%SZ`); fall back to unix seconds so
    // the binary stays dependency-free and never fails just for a missing timestamp.
    let generated_at = std::env::var("SNAPSHOT_GENERATED_AT").unwrap_or_else(|_| {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        format!("unix:{secs}")
    });

    let snapshot = snapshot_from_reseller_dir(responses, version, generated_at.clone());
    if snapshot.models.is_empty() {
        // Not fatal — an all-failed fetch still produces a valid (empty) signed snapshot — but worth
        // shouting about, since it usually means every reseller fetch failed.
        eprintln!(
            "warning: no models normalized from {} — every reseller response was missing or unparseable",
            responses.display()
        );
    }

    let signature = sign_snapshot(&key, &snapshot).unwrap_or_else(|e| {
        eprintln!("signing failed: {e}");
        std::process::exit(1);
    });

    std::fs::create_dir_all(out).unwrap_or_else(|e| {
        eprintln!("create {}: {e}", out.display());
        std::process::exit(1);
    });

    // The signed bytes ARE the file — canonical (order-independent) so a client re-serializing to
    // verify gets byte-identical content. `sign_snapshot` signs over these same canonical bytes.
    let index_path = out.join("models-index.json");
    let sig_path = out.join("models-index.json.sig");
    let index_bytes = ai_console::model_snapshot::canonical_bytes(&snapshot);
    std::fs::write(&index_path, &index_bytes).unwrap_or_else(|e| {
        eprintln!("write {}: {e}", index_path.display());
        std::process::exit(1);
    });
    std::fs::write(&sig_path, &signature).unwrap_or_else(|e| {
        eprintln!("write {}: {e}", sig_path.display());
        std::process::exit(1);
    });

    println!(
        "signed model snapshot: {} model(s), version {version}, generated_at {generated_at}\n\
         content-hash {}\n\
         wrote {} + {}",
        snapshot.models.len(),
        content_hash(&snapshot),
        index_path.display(),
        sig_path.display()
    );
}
