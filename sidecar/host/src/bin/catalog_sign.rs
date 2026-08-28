//! CLI: build + sign the agent-catalog bundle for a release (CPE-377).
//!
//! Usage: `catalog-sign <agents-dir> <out-dir> <version>`
//!
//! `<version>` is the anti-rollback counter stamped on every entry (CPE-372). It is **not** a
//! publish timestamp and must never be computed from a clock here or by the caller: it used to be
//! `date +%s` in release.yml, which meant re-running the workflow on an old tag signed that tag's
//! stale manifests with a number newer than anything installed, and the trust engine accepted the
//! downgrade (CPE-1941). Releases derive it from the tagged commit via
//! `.github/workflows/scripts/catalog-version.sh`, which also owns the installed-base floor; this
//! binary deliberately keeps no second copy of that rule and just signs what it is given.
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
/// signature, its schema, its entry ids, each entry's content hash, and each per-manifest
/// signature, all under `pubkey`. Exits non-zero on any failure. A diagnostic for confirming
/// activation / a published catalog.
///
/// # The index goes through `VerifiedIndex`, like every other path-forming reader (CPE-1954)
///
/// This used to parse with `CatalogIndex::from_json` and then `dir.join(format!("{}.json",
/// entry.id))`. It was the last path-forming read of a catalog index outside the verifying
/// constructor, and it was the one that mattered most: `sign_bundle` refuses a hostile id at
/// **publish** time, so nothing this repo ships can carry one — but this subcommand exists to
/// appraise a bundle someone **else** built, under a key the operator names on the command line.
/// That input never passes `sign_bundle` at all.
///
/// Measured on the pre-fix binary (`tests/catalog_sign_verify_gate.rs`): a third-party bundle whose
/// single entry id was `../outside/evil` printed `OK: index + 1 manifest(s) verify under the key`
/// and exited 0, while the directory named on the command line contained no manifest whatsoever —
/// every byte it appraised came from one level up. The operator asked about a directory and was
/// answered about a different one.
///
/// "The operator supplied the bundle" does not license that. They chose the *directory*; the
/// filenames come out of a document they are asking this tool to judge, and reading one is a
/// path-formation step, not a request.
///
/// No second guard is added at the `join` below, deliberately. `VerifiedIndex::open_reported`
/// already refuses the whole index on a bad id, so a check here could never be reached and would
/// read as coverage while being unreachable — the CPE-1929 shape. One door, checked once. The
/// door's own reachability was measured rather than argued: see the CPE-1929 pair recorded at the
/// `is_valid_entry_id` refusal in `catalog.rs` (5 red disabled, 7 red with the predicate lying).
///
/// The `pub(crate)` on `CatalogIndex::from_json` means rustc now refuses the old spelling from this
/// binary outright — reintroducing it here is `error[E0624]: associated function `from_json` is
/// private`, measured. The back door (`serde_json::from_str::<CatalogIndex>`) stays open to the
/// compiler and is closed by `src/lib/catalogIndexOneDoor.test.ts`, which sweeps every tracked
/// `.rs` file.
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
    // A `.sig` that is not UTF-8 becomes the empty string and therefore verifies against nothing —
    // fail-closed, never "could not read it, carry on".
    let index_sig = String::from_utf8(read("catalog-index.json.sig")).unwrap_or_default();
    // Signature, encoding, parse, schema, and entry-id charset — one gate, in that order, and the
    // reason comes back with the refusal so the operator of a genuinely-newer catalog is told their
    // tool is old rather than that their bundle is broken. That distinction is the only reason this
    // fix was deferred out of PR #1063; `IndexRefusal::UnsupportedSchema` is it.
    let verified = sidecar_host::catalog::VerifiedIndex::open_reported(
        &index_bytes,
        index_sig.trim(),
        &keys,
    )
    .unwrap_or_else(|refusal| {
        eprintln!("FAIL: {refusal}");
        std::process::exit(1);
    });
    // Every id below is now a single safe path component, so the `join`s are joins of a filename.
    for entry in verified.entries() {
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
    println!("OK: index + {} manifest(s) verify under the key", verified.entries().len());
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
