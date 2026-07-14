//! CLI: build + sign the agent-catalog bundle for a release (CPE-377).
//!
//! Usage: `catalog-sign <agents-dir> <out-dir> <version>`
//! The ed25519 signing key (a 32-byte seed, hex) is read from `CPE_CATALOG_SIGNING_KEY`.
//! Emits `catalog-index.json` (+ `.sig`) and each `<id>.json` (+ `.sig`) into `<out-dir>`, ready to
//! upload as release assets next to the installer. Output verifies under the seed's public key —
//! embed that pubkey in `CATALOG_TRUSTED_KEYS` (src-tauri) to turn the fetch on.

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: {} <agents-dir> <out-dir> <version>", args[0]);
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
