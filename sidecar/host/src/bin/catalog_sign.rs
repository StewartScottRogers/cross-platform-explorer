//! CLI: build + sign the agent-catalog bundle for a release (CPE-377).
//!
//! Usage: `catalog-sign <agents-dir> <out-dir> <version>`
//! The ed25519 signing key (a 32-byte seed, hex) is read from `CPE_CATALOG_SIGNING_KEY`.
//! Emits `catalog-index.json` (+ `.sig`) and each `<id>.json` (+ `.sig`) into `<out-dir>`, ready to
//! upload as release assets next to the installer. Output verifies under the seed's public key —
//! embed that pubkey in `CATALOG_TRUSTED_KEYS` (src-tauri) to turn the fetch on.

use std::path::Path;

/// `catalog-sign keygen <file>` — generate an ed25519 signing key. The private 32-byte seed (hex)
/// is written to `<file>` (a `*.key`, gitignored); the **public** key is printed. Keeps the private
/// seed out of stdout/logs so it never lands in a transcript.
fn keygen(args: &[String]) {
    if args.len() != 3 {
        eprintln!("usage: {} keygen <key-file>", args[0]);
        std::process::exit(2);
    }
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).unwrap_or_else(|e| {
        eprintln!("rng: {e}");
        std::process::exit(1);
    });
    let key = ed25519_dalek::SigningKey::from_bytes(&seed);
    let pubkey = hex::encode(key.verifying_key().to_bytes());
    std::fs::write(&args[2], hex::encode(seed)).unwrap_or_else(|e| {
        eprintln!("write {}: {e}", args[2]);
        std::process::exit(1);
    });
    println!("public key (put in CATALOG_TRUSTED_KEYS): {pubkey}");
    eprintln!(
        "private seed written to {} — set it as the CPE_CATALOG_SIGNING_KEY repo secret, then \
         delete the file. NEVER commit it.",
        args[2]
    );
}

/// `catalog-sign verify <dir> <pubkey-hex>` — check a produced/published bundle: the index
/// signature, each entry's content hash, and each per-manifest signature, all under `pubkey`.
/// Exits non-zero on any failure. A diagnostic for confirming activation / a published catalog.
fn verify(args: &[String]) {
    if args.len() != 4 {
        eprintln!("usage: {} verify <dir> <pubkey-hex>", args[0]);
        std::process::exit(2);
    }
    let dir = Path::new(&args[2]);
    let keys = vec![args[3].clone()];
    let read = |name: &str| {
        std::fs::read(dir.join(name)).unwrap_or_else(|e| {
            eprintln!("read {name}: {e}");
            std::process::exit(1);
        })
    };
    let index_bytes = read("catalog-index.json");
    let index_sig = String::from_utf8(read("catalog-index.json.sig")).unwrap_or_default();
    if !sidecar_host::catalog::verify_index(&index_bytes, index_sig.trim(), &keys) {
        eprintln!("FAIL: index signature does not verify under the key");
        std::process::exit(1);
    }
    let index = sidecar_host::catalog::CatalogIndex::from_json(&String::from_utf8_lossy(&index_bytes))
        .unwrap_or_else(|e| {
            eprintln!("index parse: {e}");
            std::process::exit(1);
        });
    for entry in &index.entries {
        let m = read(&format!("{}.json", entry.id));
        if !entry.matches(&m) {
            eprintln!("FAIL: {} content does not match the index hash", entry.id);
            std::process::exit(1);
        }
        let sig = String::from_utf8(read(&format!("{}.json.sig", entry.id))).unwrap_or_default();
        if !keys.iter().any(|k| sidecar_host::trust::verify_signature(&m, sig.trim(), k)) {
            eprintln!("FAIL: {} signature does not verify", entry.id);
            std::process::exit(1);
        }
    }
    println!("OK: index + {} manifest(s) verify under the key", index.entries.len());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "keygen" {
        return keygen(&args);
    }
    if args.len() >= 2 && args[1] == "verify" {
        return verify(&args);
    }
    if args.len() != 4 {
        eprintln!("usage:\n  {0} keygen <key-file>\n  {0} <agents-dir> <out-dir> <version>", args[0]);
        std::process::exit(2);
    }
    let agents = Path::new(&args[1]);
    let out = Path::new(&args[2]);
    let version: u64 = args[3].parse().unwrap_or_else(|_| {
        eprintln!("version must be a non-negative integer");
        std::process::exit(2);
    });
    let key = std::env::var("CPE_CATALOG_SIGNING_KEY").unwrap_or_else(|_| {
        eprintln!("CPE_CATALOG_SIGNING_KEY (32-byte ed25519 seed, hex) is not set");
        std::process::exit(2);
    });

    // Gather (id, bytes) from each agent manifest in the dir.
    let read = std::fs::read_dir(agents).unwrap_or_else(|e| {
        eprintln!("read {}: {e}", agents.display());
        std::process::exit(1);
    });
    let mut paths: Vec<_> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .collect();
    paths.sort();

    let mut manifests = Vec::new();
    for p in &paths {
        let bytes = std::fs::read(p).unwrap_or_else(|e| {
            eprintln!("read {}: {e}", p.display());
            std::process::exit(1);
        });
        match serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|v| v.get("id").and_then(|s| s.as_str().map(String::from)))
        {
            Some(id) => manifests.push((id, bytes)),
            None => eprintln!("skip {}: no string `id`", p.display()),
        }
    }

    let files = sidecar_host::catalog::sign_bundle(&manifests, &key, version).unwrap_or_else(|e| {
        eprintln!("sign: {e}");
        std::process::exit(1);
    });
    std::fs::create_dir_all(out).unwrap_or_else(|e| {
        eprintln!("create {}: {e}", out.display());
        std::process::exit(1);
    });
    for (name, data) in &files {
        std::fs::write(out.join(name), data).unwrap_or_else(|e| {
            eprintln!("write {name}: {e}");
            std::process::exit(1);
        });
    }
    println!(
        "signed {} manifest(s) + index (version {version}) -> {}",
        manifests.len(),
        out.display()
    );
}
