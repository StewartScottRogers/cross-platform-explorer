//! Slice B — the real-release guard (CPE-1058).
//!
//! Runs *once, after the whole release matrix completes* (CPE-1872 round 2), against the manifest as
//! actually published on the draft release, plus every asset it references, downloaded fresh into
//! `--search`. Reads the `pubkey` + `version` straight from `tauri.conf.json` and runs
//! [`cpe_updater_verify::verify_update_manifest`] over the real bytes — the same signature check the
//! runtime plugin performs. Exits non-zero on a manifest that wouldn't verify, so a botched signing /
//! version bump / manifest fails the release before it ships.
//!
//! Requires EVERY platform the manifest names to be fetched and verified (CPE-1872 round 2) — a
//! platform this run can't fetch is a hard failure, never a skip, because the manifest is the union of
//! the whole release matrix and no partial view of it is trustworthy. If `--expect-url-prefix` is given
//! (CPE-1872 round 3), every platform's `url` must additionally start with that prefix — the crypto
//! check alone only proves the artifact BYTES are genuine, not that the `url` a real updater will fetch
//! points at this repo's own release rather than a foreign host or the wrong tag serving an
//! identically-named asset.
//!
//! Usage:
//! ```text
//! verify-release-artifacts [--conf <tauri.conf.json>] [--search <dir>]... [--manifest <latest.json>] [--expect-url-prefix <prefix>]
//! ```
//! Defaults: `--conf src-tauri/tauri.conf.json`, `--search src-tauri/target`, and the newest `latest.json`
//! found under the search dirs; `--expect-url-prefix` is unset (no URL-binding check) by default so ad
//! hoc/test invocations without a real GitHub release context keep working. Skipping when signing
//! secrets are absent is handled in `release.yml` (the step only runs when `TAURI_SIGNING_PRIVATE_KEY`
//! is set), so this binary always expects real artifacts when it runs at all.

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
    let mut expect_url_prefix: Option<String> = None;

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
            "--expect-url-prefix" => match it.next() {
                Some(v) => expect_url_prefix = Some(v.clone()),
                None => return fail("--expect-url-prefix needs a value"),
            },
            "-h" | "--help" => {
                println!(
                    "verify-release-artifacts [--conf <tauri.conf.json>] [--search <dir>]... [--manifest <latest.json>] [--expect-url-prefix <prefix>]"
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
    //
    // CPE-1872 (security-audit finding 2): this used to be `index.entry(name).or_insert_with(...)` --
    // first-wins, in WHATEVER order the OS's directory walk happens to visit files in. `--search
    // src-tauri/target` is a build dir a cache restore can leave stale/duplicate-named files in, and a
    // basename collision anywhere in the search tree means the file actually verified is decided by
    // filesystem enumeration order, not by which one is the genuine build output -- demonstrated by the
    // auditor's `basename_decoy` fixture (a same-named file elsewhere in the tree, containing the bytes
    // a signature actually verifies against, shadowed the real bundle output and passed EXIT=0 while the
    // real file was never read). Every basename must now be UNIQUE across the entire search tree, or this
    // refuses to guess and fails loud instead of silently keeping whichever one it saw first.
    let mut index: HashMap<String, PathBuf> = HashMap::new();
    let mut duplicate_basenames: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
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
                if index.contains_key(name) {
                    duplicate_basenames.insert(name.to_string());
                } else {
                    index.insert(name.to_string(), p.to_path_buf());
                }
            }
        });
    }
    if !duplicate_basenames.is_empty() {
        return fail(&format!(
            "ambiguous artifact basename(s) found more than once under the search dirs -- refusing to guess which one is real: {}",
            duplicate_basenames.into_iter().collect::<Vec<_>>().join(", ")
        ));
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

    // CPE-1872 round 3 (security-audit finding B): the crypto check below only ever proves the ARTIFACT
    // BYTES behind a `url` are genuine -- it never looks at the url's host or path, because the loader
    // matches purely by basename. A manifest can carry a perfectly-signed artifact under a `url` that
    // points at a foreign host, or at the right repo but the wrong release tag, and pass clean while
    // shipping real updater clients infrastructure this release never checked (demonstrated:
    // `n1_foreign_host_same_basename`, `n2_wrong_tag_same_basename`, both EXIT=0 before this check).
    // Enforced HERE, at the same site as the crypto check, rather than as a separate shell-level grep
    // over the manifest in release.yml: this way the binding can never be silently dropped by a future
    // refactor of the download step, and it is covered by this crate's own test suite like every other
    // manifest invariant. Opt-in via `--expect-url-prefix` (unset by default) so ad hoc/test invocations
    // that don't have a real GitHub release context keep working unchanged.
    if let Some(prefix) = &expect_url_prefix {
        let offenders = cpe_updater_verify::platforms_with_url_outside_prefix(&manifest, prefix);
        if !offenders.is_empty() {
            let detail = offenders
                .iter()
                .map(|(name, url)| format!("{name} -> {url}"))
                .collect::<Vec<_>>()
                .join(", ");
            return fail(&format!(
                "manifest platform url(s) do not start with the expected prefix '{prefix}' -- refusing \
                 to trust a manifest that could point real updater clients at unexpected infrastructure \
                 even though the artifact bytes/signatures may check out: {detail}"
            ));
        }
    }

    println!("verify-release-artifacts (CPE-1058)");
    println!("  config     : {}", conf.display());
    println!("  version    : {version}");
    println!("  manifest   : {}", manifest_path.display());
    println!("  search dirs: {}", search_dirs.len());
    if let Some(prefix) = &expect_url_prefix {
        println!("  url prefix : {prefix} (enforced)");
    }

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
            // CPE-1872 (security-audit finding 1): as of the lib.rs fix, `Ok(())` is only possible when
            // EVERY platform the manifest names was fetched and cryptographically verified -- a platform
            // this runner couldn't fetch is now a hard `ArtifactUnavailable` failure, not a skip, so `n`
            // is always the full platform count on this path. `served == 0` is therefore unreachable in
            // practice (an empty `platforms` is already `NoPlatforms`, which is `Err`) -- kept as a
            // belt-and-suspenders guard in case that invariant is ever loosened again, so a future
            // regression fails loud here too rather than silently printing a misleading "OK".
            let n = served.get();
            let total = cpe_updater_verify::manifest_platform_count(&manifest).unwrap_or(n);
            if n == 0 || n != total {
                return fail(&format!(
                    "manifest claims {total} platform(s) but only {n} were actually, cryptographically verified -- refusing to report success on a partial check (this should be unreachable; verify_update_manifest is supposed to fail before returning Ok in this case)",
                ));
            }
            println!("OK: verified {n} of {total} platform signature(s) against the configured pubkey.");
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
