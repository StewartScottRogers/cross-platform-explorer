//! Slice B — the real-release guard (CPE-1058).
//!
//! Runs on a release runner *after* `tauri-action` has built the installers + the updater `latest.json`.
//! Reads the `pubkey` + `version` straight from `tauri.conf.json`, locates the just-built `latest.json`
//! and updater artifacts on disk, and runs [`cpe_updater_verify::verify_update_manifest`] over the real
//! bytes — the same signature check the runtime plugin performs. Exits non-zero on a manifest that
//! wouldn't verify, so a botched signing / version bump / manifest fails the release before it ships.
//!
//! It verifies only the platforms whose artifacts are present on *this* runner (the release job is a
//! per-OS matrix; each leg builds a slice of the platforms and merges into a shared `latest.json`). Other
//! platforms are skipped here and verified on their own runner. If the manifest names platforms but *none*
//! matched a local artifact, that is treated as a failure — a run that verified nothing is not a pass.
//!
//! Usage:
//! ```text
//! verify-release-artifacts [--conf <tauri.conf.json>] [--search <dir>]... [--manifest <latest.json>]
//! ```
//! Defaults: `--conf src-tauri/tauri.conf.json`, `--search src-tauri/target`, and the newest `latest.json`
//! found under the search dirs. Skipping when signing secrets are absent is handled in `release.yml`
//! (the step only runs when `TAURI_SIGNING_PRIVATE_KEY` is set), so this binary always expects real artifacts.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cpe_updater_verify::verify_update_manifest;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut conf = PathBuf::from("src-tauri/tauri.conf.json");
    let mut manifest_path: Option<PathBuf> = None;
    let mut search_dirs: Vec<PathBuf> = Vec::new();

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--conf" => match it.next() {
                Some(v) => conf = PathBuf::from(v),
                None => return fail("--conf needs a path"),
            },
            "--manifest" => match it.next() {
                Some(v) => manifest_path = Some(PathBuf::from(v)),
                None => return fail("--manifest needs a path"),
            },
            "--search" => match it.next() {
                Some(v) => search_dirs.push(PathBuf::from(v)),
                None => return fail("--search needs a path"),
            },
            "-h" | "--help" => {
                println!(
                    "verify-release-artifacts [--conf <tauri.conf.json>] [--search <dir>]... [--manifest <latest.json>]"
                );
                return ExitCode::SUCCESS;
            }
            other => return fail(&format!("unknown argument: {other}")),
        }
    }
    if search_dirs.is_empty() {
        search_dirs.push(PathBuf::from("src-tauri/target"));
    }

    // --- Read pubkey + version from tauri.conf.json ---
    let conf_text = match std::fs::read_to_string(&conf) {
        Ok(t) => t,
        Err(e) => return fail(&format!("cannot read {}: {e}", conf.display())),
    };
    let conf_json: serde_json::Value = match serde_json::from_str(&conf_text) {
        Ok(v) => v,
        Err(e) => return fail(&format!("{} is not valid JSON: {e}", conf.display())),
    };
    let pubkey = match conf_json
        .pointer("/plugins/updater/pubkey")
        .and_then(|v| v.as_str())
    {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => return fail("tauri.conf.json has no plugins.updater.pubkey"),
    };
    let version = match conf_json.get("version").and_then(|v| v.as_str()) {
        Some(v) => v.to_string(),
        None => return fail("tauri.conf.json has no top-level version"),
    };

    // --- Build a basename -> path index of every file under the search dirs, and find latest.json ---
    let mut index: HashMap<String, PathBuf> = HashMap::new();
    let mut discovered_manifest: Option<PathBuf> = None;
    for dir in &search_dirs {
        walk(dir, &mut |p| {
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                if name == "latest.json" {
                    // Prefer the most recently modified latest.json if several exist.
                    if is_newer(p, discovered_manifest.as_deref()) {
                        discovered_manifest = Some(p.to_path_buf());
                    }
                }
                index.entry(name.to_string()).or_insert_with(|| p.to_path_buf());
            }
        });
    }

    let manifest_path = match manifest_path.or(discovered_manifest) {
        Some(p) => p,
        None => {
            return fail(&format!(
                "no latest.json found under {} (searched: {})",
                search_dirs
                    .iter()
                    .map(|d| d.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                search_dirs.len()
            ));
        }
    };
    let manifest = match std::fs::read_to_string(&manifest_path) {
        Ok(t) => t,
        Err(e) => return fail(&format!("cannot read {}: {e}", manifest_path.display())),
    };

    println!("verify-release-artifacts (CPE-1058)");
    println!("  config     : {}", conf.display());
    println!("  version    : {version}");
    println!("  manifest   : {}", manifest_path.display());
    println!("  search dirs: {}", search_dirs.len());

    // --- Verify. The loader resolves a platform `url` to a local file by basename; it counts how many
    //     artifacts it actually served so we can reject a run that verified nothing. ---
    let served = Cell::new(0usize);
    let result = verify_update_manifest(&manifest, &pubkey, &version, |url| {
        let name = basename(url)?;
        let path = index.get(name)?;
        match std::fs::read(path) {
            Ok(bytes) => {
                served.set(served.get() + 1);
                println!("  verifying  : {name}");
                Some(bytes)
            }
            Err(e) => {
                eprintln!("  warn: cannot read {}: {e}", path.display());
                None
            }
        }
    });

    match result {
        Ok(()) => {
            let n = served.get();
            if n == 0 {
                return fail(
                    "manifest shape is valid but NO platform artifact matched a local file — nothing was \
                     cryptographically verified (wrong --search dir, or artifacts not built?)",
                );
            }
            println!("OK: manifest + {n} platform signature(s) verified against the configured pubkey.");
            ExitCode::SUCCESS
        }
        Err(problems) => {
            eprintln!("FAILED: the updater manifest would not verify:");
            for p in &problems {
                eprintln!("  - {p}");
            }
            ExitCode::FAILURE
        }
    }
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("verify-release-artifacts: {msg}");
    ExitCode::FAILURE
}

/// The last path segment of an updater `url` (a GitHub release download URL), used to match it to a
/// locally-built artifact of the same filename.
fn basename(url: &str) -> Option<&str> {
    url.rsplit(['/', '\\']).next().filter(|s| !s.is_empty())
}

fn is_newer(candidate: &Path, current: Option<&Path>) -> bool {
    let Some(current) = current else { return true };
    let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    match (mtime(candidate), mtime(current)) {
        (Some(a), Some(b)) => a >= b,
        _ => true,
    }
}

/// Recursively visit every file under `dir`, calling `f` on each. Silently ignores unreadable dirs
/// (matches the app's "skip what you can't read" filesystem convention) so a permission hiccup on some
/// unrelated subtree never fails the guard.
fn walk(dir: &Path, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => walk(&path, f),
            Ok(ft) if ft.is_file() => f(&path),
            _ => {}
        }
    }
}
