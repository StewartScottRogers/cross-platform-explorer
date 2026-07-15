//! Remote-repo browsing (CPE-434) — provider-agnostic entry type + per-provider response parsers.
//!
//! Browsing "any repo" means turning each provider's API response into a common [`RemoteEntry`]
//! list the UI renders like a folder. The parsing is pure (unit-tested); the host performs the
//! actual allow-listed API call on the sidecar's behalf (CPE-433). GitHub's Contents API lands
//! first (browse any public repo); other providers add a parser against the same `RemoteEntry`.

use serde::Serialize;

/// One entry in a remote repo listing — a file or a folder, provider-agnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteEntry {
    pub name: String,
    /// Path within the repo (e.g. `src/lib.rs`).
    pub path: String,
    pub is_dir: bool,
    /// Byte size for files; 0 for folders.
    pub size: u64,
}

/// Parse a GitHub **Contents API** response (`GET /repos/{o}/{r}/contents/{path}`) into entries.
/// The endpoint returns a JSON array for a directory (or a single object for a file); we handle both.
/// Folders sort before files, then by name — the same ordering the explorer uses. Malformed input
/// yields an empty list rather than erroring.
pub fn parse_github_contents(json: &str) -> Vec<RemoteEntry> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let items: Vec<&serde_json::Value> = match &value {
        serde_json::Value::Array(a) => a.iter().collect(),
        serde_json::Value::Object(_) => vec![&value], // a single file
        _ => return Vec::new(),
    };
    let mut out: Vec<RemoteEntry> = items
        .into_iter()
        .filter_map(|it| {
            let name = it.get("name")?.as_str()?.to_string();
            let path = it.get("path").and_then(|p| p.as_str()).unwrap_or(&name).to_string();
            let is_dir = it.get("type").and_then(|t| t.as_str()) == Some("dir");
            let size = it.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
            Some(RemoteEntry { name, path, is_dir, size: if is_dir { 0 } else { size } })
        })
        .collect();
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_github_directory_listing_folders_first() {
        let json = r#"[
            {"name":"README.md","path":"README.md","type":"file","size":1024},
            {"name":"src","path":"src","type":"dir","size":0},
            {"name":"Cargo.toml","path":"Cargo.toml","type":"file","size":256},
            {"name":".github","path":".github","type":"dir","size":0}
        ]"#;
        let e = parse_github_contents(json);
        assert_eq!(e.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), [".github", "src", "Cargo.toml", "README.md"]);
        assert!(e[0].is_dir && e[1].is_dir && !e[2].is_dir);
        let readme = e.iter().find(|x| x.name == "README.md").unwrap();
        assert_eq!(readme.size, 1024);
    }

    #[test]
    fn parses_a_nested_path_and_a_single_file_object() {
        let dir = r#"[{"name":"lib.rs","path":"src/lib.rs","type":"file","size":42}]"#;
        assert_eq!(parse_github_contents(dir)[0].path, "src/lib.rs");
        // A file endpoint returns a single object, not an array.
        let file = r#"{"name":"x.txt","path":"docs/x.txt","type":"file","size":7}"#;
        let e = parse_github_contents(file);
        assert_eq!(e.len(), 1);
        assert_eq!((e[0].path.as_str(), e[0].size, e[0].is_dir), ("docs/x.txt", 7, false));
    }

    #[test]
    fn malformed_or_unexpected_json_yields_no_entries() {
        assert!(parse_github_contents("not json").is_empty());
        assert!(parse_github_contents("42").is_empty());
        assert!(parse_github_contents("[]").is_empty());
    }
}
