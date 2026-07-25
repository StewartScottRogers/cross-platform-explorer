//! Shared agent memory graph (CPE-524) — the [CPE-504] core. A per-project store of **markdown notes**
//! (frontmatter `id`/`tags` + `[[wikilinks]]` in the body) forming a **graph**. Agents add notes, walk
//! links to neighbours, and **recall** the most relevant notes for a query, so a swarm (or successive
//! sessions) share context instead of re-deriving it. This module is the pure core: no I/O — the
//! `.agentmemory/` disk persistence + MCP tools sit on top (CPE-525).
//!
//! It is **separate** from the app's own `memory/MEMORY.md` (activation decision) but reuses its shape.
//! Writes are **append-only** with content-hash **dedup**, so concurrent swarm writers never conflict.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// A single memory note.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Note {
    pub id: String,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub body: String,
}

fn parse_tags(raw: &str) -> Vec<String> {
    raw.trim()
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .map(|inner| {
            inner
                .split(',')
                .map(|t| t.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Extract `[[link]]` targets from a note body (trimmed, non-empty).
pub fn parse_links(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else { break };
        let link = after[..end].trim();
        if !link.is_empty() {
            out.push(link.to_string());
        }
        rest = &after[end + 2..];
    }
    out
}

/// Parse a note from markdown (frontmatter `id`/`tags` + body with `[[links]]`). `None` if it has no id.
pub fn parse_note(md: &str) -> Option<Note> {
    let mut id: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let t = md.trim_start();
    let body: &str = if let Some(rest) = t.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            for line in rest[..end].lines() {
                if let Some((k, v)) = line.split_once(':') {
                    match k.trim() {
                        "id" | "slug" => id = Some(v.trim().trim_matches('"').to_string()),
                        "tags" => tags = parse_tags(v.trim()),
                        _ => {}
                    }
                }
            }
            // body is everything after the closing `---` line.
            let after = &rest[end + 4..];
            after.strip_prefix('\n').unwrap_or(after)
        } else {
            t
        }
    } else {
        t
    };
    let id = id.filter(|s| !s.is_empty())?;
    Some(Note { id, tags, links: parse_links(body), body: body.trim().to_string() })
}

/// A content hash for dedup: identical tags (order-insensitive) + body ⇒ same hash.
pub fn content_hash(tags: &[String], body: &str) -> u64 {
    let mut t = tags.to_vec();
    t.sort();
    let mut h = DefaultHasher::new();
    t.hash(&mut h);
    body.trim().hash(&mut h);
    h.finish()
}

/// The per-project memory graph.
#[derive(Debug, Default)]
pub struct MemoryGraph {
    notes: HashMap<String, Note>,
    hashes: std::collections::HashSet<u64>,
}

impl MemoryGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a note. Append-only + **dedup**: a note whose (tags, body) content already exists is skipped
    /// (returns `false`); a duplicate `id` with new content overwrites. Returns whether it was stored.
    pub fn add(&mut self, note: Note) -> bool {
        let hash = content_hash(&note.tags, &note.body);
        if self.hashes.contains(&hash) {
            return false; // exact duplicate content — dedup
        }
        self.hashes.insert(hash);
        self.notes.insert(note.id.clone(), note);
        true
    }

    pub fn get(&self, id: &str) -> Option<&Note> {
        self.notes.get(id)
    }
    pub fn len(&self) -> usize {
        self.notes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// The notes a note links to that actually exist (dangling `[[links]]` are tolerated + omitted).
    pub fn neighbors(&self, id: &str) -> Vec<&Note> {
        let Some(note) = self.notes.get(id) else { return Vec::new() };
        note.links.iter().filter_map(|l| self.notes.get(l)).collect()
    }

    /// Notes that link **to** `id` (backlinks).
    pub fn backlinks(&self, id: &str) -> Vec<&Note> {
        self.notes.values().filter(|n| n.links.iter().any(|l| l == id)).collect()
    }

    fn base_score(note: &Note, terms: &[String], q_tags: &[String]) -> f64 {
        let tag_hits = note.tags.iter().filter(|t| q_tags.iter().any(|q| q.eq_ignore_ascii_case(t))).count();
        let hay = format!("{} {}", note.body.to_lowercase(), note.tags.join(" ").to_lowercase());
        let text_hits = terms.iter().filter(|t| hay.contains(t.as_str())).count();
        (tag_hits as f64) * 3.0 + (text_hits as f64)
    }

    /// Recall the top-`n` most relevant notes for a free-text `query` + optional `tags`. Score = tag
    /// overlap + text-term matches, plus **link proximity** (a fraction of the best directly-linked
    /// neighbour's base score). Only notes with a positive score are returned, best first.
    pub fn recall(&self, query: &str, tags: &[String], n: usize) -> Vec<&Note> {
        let terms: Vec<String> = query.to_lowercase().split_whitespace().map(|s| s.to_string()).collect();
        let base: HashMap<&str, f64> =
            self.notes.iter().map(|(id, note)| (id.as_str(), Self::base_score(note, &terms, tags))).collect();

        let mut scored: Vec<(&Note, f64)> = self
            .notes
            .values()
            .map(|note| {
                let proximity = note
                    .links
                    .iter()
                    .filter_map(|l| base.get(l.as_str()))
                    .cloned()
                    .fold(0.0_f64, f64::max);
                (note, base[note.id.as_str()] + 0.5 * proximity)
            })
            .filter(|(_, s)| *s > 0.0)
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.id.cmp(&b.0.id)));
        scored.into_iter().take(n).map(|(note, _)| note).collect()
    }
}

// --- Disk persistence + MCP tool surface (CPE-525) ----------------------------------------------

/// Serialize a note back to markdown (frontmatter `id`/`tags` + body). Round-trips with `parse_note`.
pub fn note_to_markdown(note: &Note) -> String {
    format!("---\nid: {}\ntags: [{}]\n---\n{}\n", note.id, note.tags.join(", "), note.body.trim())
}

fn sanitize(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect()
}

/// Write a note as a file in `dir` (created if needed): `<id>-<hash8>.md`. Append-only — the hash
/// suffix makes concurrent writers land in distinct files, never clobbering. Returns the path.
pub fn save_note(dir: &std::path::Path, note: &Note) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let hash = content_hash(&note.tags, &note.body);
    let path = dir.join(format!("{}-{:08x}.md", sanitize(&note.id), (hash & 0xffff_ffff) as u32));
    std::fs::write(&path, note_to_markdown(note))?;
    Ok(path)
}

/// Load every `*.md` in `dir` into a graph (dedup applies). A missing dir ⇒ empty graph, never an error.
pub fn load_dir(dir: &std::path::Path) -> MemoryGraph {
    let mut g = MemoryGraph::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return g };
    // Sort by path so content-hash dedup is deterministic across platforms. `read_dir` yields entries in
    // arbitrary OS order, and `add` keeps the FIRST note seen for a given content hash — so without a
    // stable order, which of two identical-content notes (e.g. `a-…md` vs `dup-…md`) survives (and thus
    // its id + links) would depend on the filesystem. This bit Linux CI, where `dup` was read before `a`.
    let mut paths: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();
    paths.sort();
    for p in paths {
        if let Ok(md) = std::fs::read_to_string(&p) {
            if let Some(n) = parse_note(&md) {
                g.add(n);
            }
        }
    }
    g
}

/// Dispatch an MCP memory tool against a graph (CPE-525) — the pure adapter the MCP server maps
/// `memory.write` / `memory.read` / `memory.recall` onto. `write` mutates the graph (the server also
/// persists via [`save_note`]); `read`/`recall` are read-only. Returns a JSON result.
pub fn memory_tool(graph: &mut MemoryGraph, tool: &str, args: &serde_json::Value) -> serde_json::Value {
    use serde_json::json;
    match tool {
        "memory.write" => {
            let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if body.trim().is_empty() {
                return json!({ "ok": false, "error": "empty body" });
            }
            let tags: Vec<String> = args
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| format!("note-{:08x}", (content_hash(&tags, &body) & 0xffff_ffff) as u32));
            let mut links = parse_links(&body);
            if let Some(extra) = args.get("links").and_then(|v| v.as_array()) {
                links.extend(extra.iter().filter_map(|l| l.as_str().map(String::from)));
            }
            let stored = graph.add(Note { id: id.clone(), tags, links, body });
            json!({ "ok": true, "stored": stored, "id": id })
        }
        "memory.read" => {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
            json!({ "ok": true, "note": graph.get(id) })
        }
        "memory.recall" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let tags: Vec<String> = args
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let n = args.get("n").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
            json!({ "ok": true, "notes": graph.recall(query, &tags, n) })
        }
        other => json!({ "ok": false, "error": format!("unknown memory tool '{other}'") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, tags: &[&str], body: &str) -> Note {
        Note { id: id.into(), tags: tags.iter().map(|s| s.to_string()).collect(), links: parse_links(body), body: body.into() }
    }

    #[test]
    fn parses_a_note_with_frontmatter_and_links() {
        let md = "---\nid: auth-flow\ntags: [auth, design]\n---\nUse OAuth. See [[token-store]] and [[rate-limit]].";
        let n = parse_note(md).unwrap();
        assert_eq!(n.id, "auth-flow");
        assert_eq!(n.tags, vec!["auth", "design"]);
        assert_eq!(n.links, vec!["token-store", "rate-limit"]);
        assert!(n.body.contains("OAuth"));
    }

    #[test]
    fn a_note_without_an_id_is_none() {
        assert!(parse_note("---\ntags: [x]\n---\nbody").is_none());
        assert!(parse_note("just text").is_none());
    }

    #[test]
    fn add_dedups_identical_content() {
        let mut g = MemoryGraph::new();
        assert!(g.add(note("a", &["x"], "hello")));
        assert!(!g.add(note("a2", &["x"], "hello"))); // same tags+body ⇒ dedup, not stored
        assert!(g.add(note("b", &["x"], "different")));
        assert_eq!(g.len(), 2);
    }

    #[test]
    fn neighbors_and_backlinks_resolve_and_tolerate_dangling() {
        let mut g = MemoryGraph::new();
        g.add(note("a", &[], "links to [[b]] and [[ghost]]"));
        g.add(note("b", &[], "plain"));
        let n: Vec<&str> = g.neighbors("a").iter().map(|x| x.id.as_str()).collect();
        assert_eq!(n, vec!["b"]); // ghost is dangling → omitted
        let bl: Vec<&str> = g.backlinks("b").iter().map(|x| x.id.as_str()).collect();
        assert_eq!(bl, vec!["a"]);
    }

    #[test]
    fn recall_ranks_by_tags_then_text_then_proximity() {
        let mut g = MemoryGraph::new();
        g.add(note("auth", &["auth"], "the authentication flow uses tokens [[tokens]]"));
        g.add(note("tokens", &[], "token storage details"));
        g.add(note("ui", &["ui"], "button colours"));
        // Query favouring the auth tag + text.
        let hits: Vec<&str> = g.recall("token flow", &["auth".into()], 3).iter().map(|n| n.id.as_str()).collect();
        assert_eq!(hits[0], "auth"); // tag + text wins
        assert!(hits.contains(&"tokens")); // matched by text ("token") + proximity from auth
        assert!(!hits.contains(&"ui")); // no overlap → excluded
    }

    #[test]
    fn recall_returns_nothing_for_an_unrelated_query() {
        let mut g = MemoryGraph::new();
        g.add(note("a", &["x"], "alpha"));
        assert!(g.recall("zzz nonexistent", &[], 5).is_empty());
    }

    // --- Persistence + MCP tool surface (CPE-525) ---------------------------------------
    #[test]
    fn markdown_round_trips_through_parse() {
        let n = note("auth", &["auth", "design"], "OAuth flow, see [[tokens]]");
        let back = parse_note(&note_to_markdown(&n)).unwrap();
        assert_eq!(back.id, "auth");
        assert_eq!(back.tags, vec!["auth", "design"]);
        assert_eq!(back.links, vec!["tokens"]);
    }

    #[test]
    fn save_then_load_dir_round_trips() {
        let dir = std::env::temp_dir().join(format!("cpe-mem-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        save_note(&dir, &note("a", &["x"], "alpha [[b]]")).unwrap();
        save_note(&dir, &note("b", &[], "beta")).unwrap();
        save_note(&dir, &note("dup", &["x"], "alpha [[b]]")).unwrap(); // same content as 'a' → dedups on load
        let g = load_dir(&dir);
        assert_eq!(g.len(), 2); // a/dup dedup to one; b
        assert_eq!(g.neighbors("a").len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_dir_of_a_missing_folder_is_empty_not_an_error() {
        assert!(load_dir(std::path::Path::new("/no/such/cpe/dir")).is_empty());
    }

    #[test]
    fn memory_tool_write_read_recall() {
        use serde_json::json;
        let mut g = MemoryGraph::new();
        let w = memory_tool(&mut g, "memory.write", &json!({ "id": "auth", "tags": ["auth"], "body": "token flow [[tokens]]" }));
        assert_eq!(w["ok"], true);
        assert_eq!(w["stored"], true);
        assert_eq!(g.len(), 1);
        // A duplicate write dedups.
        let w2 = memory_tool(&mut g, "memory.write", &json!({ "id": "auth2", "tags": ["auth"], "body": "token flow [[tokens]]" }));
        assert_eq!(w2["stored"], false);

        let r = memory_tool(&mut g, "memory.read", &json!({ "id": "auth" }));
        assert_eq!(r["note"]["id"], "auth");

        let rc = memory_tool(&mut g, "memory.recall", &json!({ "query": "token", "tags": ["auth"] }));
        assert_eq!(rc["notes"][0]["id"], "auth");

        assert_eq!(memory_tool(&mut g, "memory.bogus", &json!({}))["ok"], false);
        assert_eq!(memory_tool(&mut g, "memory.write", &json!({ "body": "  " }))["ok"], false); // empty body
    }
}
