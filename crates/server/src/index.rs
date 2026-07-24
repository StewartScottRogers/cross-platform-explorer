//! Instant-search **index engine** (CPE-832/833, epic CPE-703): the compact per-volume filename index that
//! CPE-831's query core ([`crate::index_query`]) runs against, kept live incrementally as files change.
//!
//! [`crate::index_query`] owns the query grammar, `matches`, and `rank`; this module owns the *candidate
//! feed*: an initial crawl ([`Index::build`]), a persistent on-disk store ([`Index::save`] /
//! [`Index::load`]), a fast [`Index::search`] that prunes candidates with a trigram map before handing
//! survivors to `index_query::matches`/`score`, and — new in CPE-833 — incremental mutation primitives
//! ([`Index::apply_create`] / [`Index::apply_remove`] / [`Index::apply_rename`]) so a change source can keep
//! the index current in O(depth) instead of re-crawling.
//!
//! ## Design (Option A — roll-our-own, chosen in the CPE-832 big-design writeup)
//! - **Feature-gated OFF** (`index` cargo feature): the plain build compiles zero indexer — the delete-test.
//! - **Zero new dependencies.** The on-disk format is a hand-rolled versioned binary layout (no
//!   bincode/rkyv/mmap crate), keeping the `cpe-server` lean-core rule. Load reads the file into memory and
//!   **rebuilds** the auxiliary maps (trigram postings, name interner, child adjacency) rather than
//!   persisting them — smaller disk (filenames only), and load is a cold one-time cost, not the <100ms
//!   *warm query* path (`index_query::rank` owns that).
//! - **Absolute-path roots (CPE-833).** The root entry stores the absolute crawl-root path, so reconstructed
//!   hit paths are absolute + openable, and change events (which carry absolute paths) resolve against the
//!   same strings.
//! - **Interned names + parent pointers.** Each entry stores a name id and its parent's entry index, so a
//!   full path is reconstructed on demand and unique names are stored once.
//! - **Tombstoning, not physical delete (CPE-833).** Entries are addressed by index (parents + trigram
//!   postings point at ids), so a remove **marks the entry + its subtree dead** rather than shifting the
//!   vec; `search`, `path_of`, and `resolve` skip dead entries. [`Index::to_bytes`] **compacts** (drops dead
//!   entries + remaps ids), so tombstones never persist and the on-disk format stays v1.
//! - **Trigram pruning is an optimisation, never a correctness gate.** Trigrams only narrow the candidate
//!   set for plain substring terms; the final say is always `index_query::matches`, so globs, short terms,
//!   and `ext:`/`path:`-only queries fall back to a full scan and still return correct results.
//!
//! The crawl reuses the `list_dir` skip-on-error discipline (unreadable dirs skipped, dot-dirs + symlinks
//! not descended) shared with [`crate::name_search`], so the plain explorer and the index agree on what a
//! volume contains.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::fsutil::entry_is_symlink;
use crate::index_query::{self, Candidate, Query};

/// On-disk magic + format version. A mismatch on either → transparent rebuild (never a hard error): the
/// caller treats [`IndexError::Stale`] as "crawl again", so bumping this constant silently re-indexes.
const MAGIC: &[u8; 8] = b"CPEIDX\x00\x00";
const FORMAT_VERSION: u32 = 1;

/// Safety cap on directories walked in one [`Index::build`], mirroring `name_search`'s bound so a runaway
/// tree (or a symlink loop that slipped through) can't crawl forever. Reported via [`BuildStats::truncated`].
const MAX_DIRS: u64 = 5_000_000;

/// A sentinel parent id meaning "this entry is a crawl root" (no parent within the index).
const NO_PARENT: u32 = u32::MAX;

/// One indexed filesystem entry. `name` and `parent` are indices (into [`Index::names`] and
/// [`Index::entries`] respectively); `is_dir` distinguishes folders; `dead` tombstones a removed entry
/// (never persisted — [`Index::to_bytes`] compacts dead entries away). Deliberately tiny — the whole point
/// of Option A is that a filename index costs only interned names + ids.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    /// Index into [`Index::names`] — the entry's bare file name (the absolute path for a crawl root).
    name: u32,
    /// Index into [`Index::entries`] of the containing directory, or [`NO_PARENT`] for a crawl root.
    parent: u32,
    is_dir: bool,
    /// In-memory tombstone: a removed entry is marked dead (subtree included) rather than physically
    /// deleted, so parent + trigram indices stay valid. Compacted away on [`Index::to_bytes`].
    dead: bool,
}

/// A single instant-search hit: the reconstructed absolute path, the bare name, whether it's a folder, and
/// its `index_query::score` relevance (higher = better). Owned so callers can stream/serialise freely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexHit {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub score: i32,
}

/// Why an [`Index::load`] didn't return a usable index.
#[derive(Debug)]
pub enum IndexError {
    /// The file couldn't be read (missing, permissions, truncated header). Carries the OS message.
    Io(String),
    /// The file is a different magic/format version than this build understands — the caller should
    /// **rebuild** (crawl again), not surface an error. This is the transparent-rebuild path.
    Stale,
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexError::Io(m) => write!(f, "index io error: {m}"),
            IndexError::Stale => write!(f, "index is stale (format mismatch); rebuild"),
        }
    }
}
impl std::error::Error for IndexError {}

/// What a [`Index::build`] crawl covered — how many directories it scanned and whether a cap truncated it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuildStats {
    pub dirs_scanned: u64,
    /// True if the crawl stopped early — a [`MAX_DIRS`] cap or a cancellation. The index is still valid,
    /// just partial.
    pub truncated: bool,
}

/// A compact per-volume filename index. Build it from a root with [`Index::build`], persist with
/// [`Index::save`], reload with [`Index::load`], query with [`Index::search`], and keep it live with the
/// `apply_*` mutation primitives.
#[derive(Debug, Clone, Default)]
pub struct Index {
    /// An opaque id for the volume/root this index describes (e.g. a hash of the root path). Stored in the
    /// header so a caller can sanity-check a loaded file belongs to the volume it expected.
    volume_id: u64,
    /// Interned unique names — each [`Entry::name`] indexes here. Deduped so a name shared by many files
    /// costs one string.
    names: Vec<String>,
    /// The flat entry table. An entry's `parent` indexes back into this vec.
    entries: Vec<Entry>,
    /// Case-folded trigram → entry-id posting list. Aux (rebuilt on load); used only to prune candidates
    /// for plain substring terms. Mutations only ever *add* postings — correctness comes from `matches`.
    trigrams: HashMap<u32, Vec<u32>>,
    /// name → id interner. Aux (rebuilt on load); lets [`Index::resolve`] turn a path component into a
    /// name id in O(1) and keeps interning cheap during mutation.
    name_ids: HashMap<String, u32>,
    /// parent entry id → its child entry ids. Aux (rebuilt on load); the adjacency backing path resolution
    /// and subtree removal. May list dead ids — every read checks [`Entry::dead`].
    children: HashMap<u32, Vec<u32>>,
    /// Whether the crawl that produced this index was truncated (cap or cancel).
    truncated: bool,
}

impl Index {
    /// The volume id this index was built for.
    pub fn volume_id(&self) -> u64 {
        self.volume_id
    }

    /// How many **live** entries the index holds (tombstones excluded).
    pub fn len(&self) -> usize {
        self.entries.iter().filter(|e| !e.dead).count()
    }

    /// Whether the index has no live entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether the crawl that produced this index stopped early (cap or cancel).
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Crawl `root` into a fresh index. `root` should be an **absolute** path; it is stored (trailing
    /// separators trimmed) as the root entry so reconstructed hit paths are absolute. `volume_id` is stored
    /// verbatim in the header. `cancel` is polled between directories so a caller can abort a long crawl; a
    /// cancelled crawl returns the partial index built so far, with [`BuildStats::truncated`] set.
    ///
    /// Mirrors [`crate::name_search::walk_name_matches`]: an explicit stack (bounded memory), skip
    /// unreadable dirs, don't descend dot-dirs or symlinks. A non-folder root is an `Err`.
    pub fn build(root: &str, volume_id: u64, cancel: &AtomicBool) -> Result<(Index, BuildStats), String> {
        let root_path = Path::new(root);
        if !root_path.is_dir() {
            return Err(format!("{root}: not a folder"));
        }

        let mut idx = Index { volume_id, ..Index::default() };
        let mut stats = BuildStats::default();

        // The root itself is entry 0 (its own parent), storing the *absolute* root path so descendants
        // reconstruct to absolute, openable paths.
        let root_name = root.trim_end_matches(std::path::MAIN_SEPARATOR);
        let root_id = idx.push_entry(root_name, NO_PARENT, true);

        // Stack of (directory path, its entry id) to expand.
        let mut stack = vec![(root_path.to_path_buf(), root_id)];
        while let Some((dir, dir_id)) = stack.pop() {
            if cancel.load(Ordering::Relaxed) {
                stats.truncated = true;
                break;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            stats.dirs_scanned += 1;
            if stats.dirs_scanned > MAX_DIRS {
                stats.truncated = true;
                break;
            }
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let Ok(meta) = entry.metadata() else { continue };
                let is_dir = meta.is_dir();
                let id = idx.push_entry(&name, dir_id, is_dir);
                if is_dir && !name.starts_with('.') && !entry_is_symlink(&entry) {
                    stack.push((entry.path(), id));
                }
            }
        }

        idx.truncated = stats.truncated;
        idx.rebuild_trigrams();
        Ok((idx, stats))
    }

    /// Intern `name`, returning its id (deduped so repeats are free).
    fn intern(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.name_ids.get(name) {
            return id;
        }
        let id = self.names.len() as u32;
        self.names.push(name.to_string());
        self.name_ids.insert(name.to_string(), id);
        id
    }

    /// Append a fresh [`Entry`] under `parent`, maintaining the interner + child adjacency. Returns its id.
    /// Trigrams are added in bulk by [`Index::rebuild_trigrams`] after a crawl, or per-entry by
    /// [`Index::add_trigrams`] during mutation.
    fn push_entry(&mut self, name: &str, parent: u32, is_dir: bool) -> u32 {
        let name_id = self.intern(name);
        let id = self.entries.len() as u32;
        self.entries.push(Entry { name: name_id, parent, is_dir, dead: false });
        self.children.entry(parent).or_default().push(id);
        id
    }

    /// Reconstruct an entry's absolute path by walking parent pointers up to a root. Uses the platform path
    /// separator so the string matches what the explorer shows. Guards against a corrupt parent cycle by
    /// bounding the climb to the entry count.
    fn path_of(&self, mut id: u32) -> String {
        let mut parts: Vec<&str> = Vec::new();
        let mut guard = 0usize;
        while let Some(entry) = self.entries.get(id as usize) {
            parts.push(&self.names[entry.name as usize]);
            if entry.parent == NO_PARENT {
                break;
            }
            id = entry.parent;
            guard += 1;
            if guard > self.entries.len() {
                break; // corrupt cycle — stop rather than loop forever
            }
        }
        parts.reverse();
        parts.join(std::path::MAIN_SEPARATOR_STR)
    }

    /// The id of the **live** child of `parent` named `name_id`, or `None`. O(fan-out) over the adjacency.
    fn child_by_name(&self, parent: u32, name_id: u32) -> Option<u32> {
        self.children.get(&parent)?.iter().copied().find(|&c| {
            let e = &self.entries[c as usize];
            !e.dead && e.name == name_id
        })
    }

    /// Resolve an absolute path to its live entry id, or `None` if it isn't indexed. Finds the crawl root
    /// whose stored path prefixes `path`, then descends component by component via the child adjacency.
    fn resolve(&self, path: &str) -> Option<u32> {
        let sep = std::path::MAIN_SEPARATOR;
        let target = path.trim_end_matches(sep);
        // Roots are entries with no parent; usually one per index.
        for (id, entry) in self.entries.iter().enumerate() {
            if entry.parent != NO_PARENT || entry.dead {
                continue;
            }
            let root_name = &self.names[entry.name as usize];
            if target == root_name {
                return Some(id as u32);
            }
            let prefix = format!("{root_name}{sep}");
            let Some(rest) = target.strip_prefix(&prefix) else { continue };
            let mut cur = id as u32;
            let mut ok = true;
            for comp in rest.split(sep).filter(|c| !c.is_empty()) {
                let Some(&name_id) = self.name_ids.get(comp) else {
                    ok = false;
                    break;
                };
                match self.child_by_name(cur, name_id) {
                    Some(child) => cur = child,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                return Some(cur);
            }
        }
        None
    }

    /// Split an absolute path into `(parent_dir, leaf_name)`, or `None` if it has no separator (a bare
    /// root). Trailing separators are ignored.
    fn split_leaf(path: &str) -> Option<(&str, &str)> {
        let sep = std::path::MAIN_SEPARATOR;
        let trimmed = path.trim_end_matches(sep);
        let pos = trimmed.rfind(sep)?;
        Some((&trimmed[..pos], &trimmed[pos + 1..]))
    }

    /// Record that `path` (a file or folder) now exists. Its **parent directory must already be indexed**;
    /// if not, the event is ignored (`None`) — the change source can trigger a subtree rebuild. Reviving a
    /// previously-removed entry of the same name is handled. Returns the entry id on success.
    pub fn apply_create(&mut self, path: &str, is_dir: bool) -> Option<u32> {
        let (parent_path, leaf) = Self::split_leaf(path)?;
        let parent_id = self.resolve(parent_path)?;
        let leaf_id = self.intern(leaf);
        // An existing *live* child → just refresh its kind. A tombstoned one → revive in place.
        if let Some(&existing) = self
            .children
            .get(&parent_id)
            .and_then(|v| v.iter().find(|&&c| self.entries[c as usize].name == leaf_id))
        {
            let e = &mut self.entries[existing as usize];
            e.is_dir = is_dir;
            e.dead = false;
            self.add_trigrams(existing);
            return Some(existing);
        }
        let id = self.push_entry(leaf, parent_id, is_dir);
        self.add_trigrams(id);
        Some(id)
    }

    /// Record that `path` no longer exists: tombstone the entry and its whole subtree. A path that isn't
    /// indexed is a no-op (returns `false`); returns `true` when something was removed.
    pub fn apply_remove(&mut self, path: &str) -> bool {
        let Some(root) = self.resolve(path) else { return false };
        // DFS the subtree via the child adjacency, marking every node dead.
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if self.entries[id as usize].dead {
                continue;
            }
            self.entries[id as usize].dead = true;
            if let Some(kids) = self.children.get(&id) {
                stack.extend(kids.iter().copied());
            }
        }
        true
    }

    /// Record that `from` was renamed/moved to `to`. Reparents the entry (its subtree follows automatically
    /// via parent pointers) and updates its leaf name. Both endpoints' directories must be indexed; an
    /// unresolvable `from` or `to`-parent is a no-op (`false`). Returns `true` on success.
    pub fn apply_rename(&mut self, from: &str, to: &str) -> bool {
        let Some(id) = self.resolve(from) else { return false };
        let Some((to_parent_path, to_leaf)) = Self::split_leaf(to) else { return false };
        let Some(new_parent) = self.resolve(to_parent_path) else { return false };
        let old_parent = self.entries[id as usize].parent;
        // Reparent (roots keep their special NO_PARENT — a renamed crawl root just changes its stored path).
        if old_parent != NO_PARENT && new_parent != old_parent {
            if let Some(v) = self.children.get_mut(&old_parent) {
                v.retain(|&c| c != id);
            }
            self.children.entry(new_parent).or_default().push(id);
            self.entries[id as usize].parent = new_parent;
        }
        let new_name_id = self.intern(to_leaf);
        self.entries[id as usize].name = new_name_id;
        self.add_trigrams(id);
        true
    }

    /// Add `id`'s current name's trigrams to the posting map (keeping each posting list sorted + deduped).
    /// Only ever *adds* — stale postings from an old name are harmless (every candidate is re-checked by
    /// `index_query::matches`) and are cleaned up by compaction on save.
    fn add_trigrams(&mut self, id: u32) {
        let lower = self.names[self.entries[id as usize].name as usize].to_lowercase();
        for tri in trigrams_of(&lower) {
            let postings = self.trigrams.entry(tri).or_default();
            if let Err(pos) = postings.binary_search(&id) {
                postings.insert(pos, id);
            }
        }
    }

    /// Rebuild the trigram posting map from the current (live) names + entries. Called after a build and
    /// after a load. Each live entry contributes the trigrams of its lowercased name.
    fn rebuild_trigrams(&mut self) {
        let mut map: HashMap<u32, Vec<u32>> = HashMap::new();
        for (id, entry) in self.entries.iter().enumerate() {
            if entry.dead {
                continue;
            }
            let lower = self.names[entry.name as usize].to_lowercase();
            for tri in trigrams_of(&lower) {
                map.entry(tri).or_default().push(id as u32);
            }
        }
        // Posting lists come out already ascending (entries pushed in id order); no sort needed.
        self.trigrams = map;
    }

    /// Rebuild every auxiliary map (interner, child adjacency, trigrams) from `names` + `entries`. Used by
    /// [`Index::from_bytes`], which materialises entries directly rather than through [`Index::push_entry`].
    fn rebuild_aux(&mut self) {
        self.name_ids = self
            .names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i as u32))
            .collect();
        self.children.clear();
        for (id, entry) in self.entries.iter().enumerate() {
            self.children.entry(entry.parent).or_default().push(id as u32);
        }
        self.rebuild_trigrams();
    }

    /// Search the index with a parsed [`Query`], returning up to `limit` hits best-first. Trigram pruning
    /// narrows the candidate set for plain substring name terms; every survivor is still confirmed by
    /// `index_query::matches`, so correctness never depends on the trigram map. Dead entries are skipped.
    pub fn search(&self, query: &Query, limit: usize) -> Vec<IndexHit> {
        if query.is_empty() {
            return Vec::new();
        }

        let mut hits: Vec<IndexHit> = Vec::new();
        let push_if_match = |id: u32, hits: &mut Vec<IndexHit>| {
            let entry = &self.entries[id as usize];
            if entry.dead {
                return;
            }
            let name = &self.names[entry.name as usize];
            let path = self.path_of(id);
            let ext = ext_of(name);
            let cand = Candidate { name, path: &path, ext };
            if index_query::matches(query, &cand) {
                let score = index_query::score(query, &cand);
                hits.push(IndexHit { path, name: name.clone(), is_dir: entry.is_dir, score });
            }
        };

        match self.prune_candidates(query) {
            Some(ids) => {
                for id in ids {
                    push_if_match(id, &mut hits);
                }
            }
            None => {
                for id in 0..self.entries.len() as u32 {
                    push_if_match(id, &mut hits);
                }
            }
        }

        // Best-first with a deterministic total order, mirroring index_query::rank.
        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.path.cmp(&b.path))
        });
        hits.truncate(limit);
        hits
    }

    /// The candidate entry ids to check, or `None` meaning "scan everything". We can prune only when the
    /// query has at least one plain (non-glob) name term of length ≥ 3: intersect the trigram postings of
    /// its trigrams. Globs, short terms, and filter-only queries can't be pruned safely, so they scan all.
    fn prune_candidates(&self, query: &Query) -> Option<Vec<u32>> {
        // The most selective usable term drives pruning; intersecting several plain terms would be even
        // tighter, but one solid term already collapses the scan and keeps the logic simple + obviously
        // correct (matches() re-checks every survivor anyway).
        let term = query.name_terms.iter().find(|t| t.len() >= 3 && !is_glob(t))?;

        let tris: Vec<u32> = trigrams_of(term);
        if tris.is_empty() {
            return None;
        }
        // Intersect the sorted posting lists of every trigram in the term. A missing trigram ⇒ no entry
        // can contain the term ⇒ empty candidate set.
        let mut acc: Option<Vec<u32>> = None;
        for tri in tris {
            let postings = self.trigrams.get(&tri).map(|v| v.as_slice()).unwrap_or(&[]);
            acc = Some(match acc {
                None => postings.to_vec(),
                Some(prev) => intersect_sorted(&prev, postings),
            });
            if acc.as_ref().is_some_and(|v| v.is_empty()) {
                return Some(Vec::new());
            }
        }
        acc
    }

    /// Serialise to the hand-rolled binary format, **compacting** dead entries away (so tombstones never
    /// persist and the format stays v1): header, interned names, live entry table with parent ids remapped.
    /// Trigrams + aux maps are not persisted (rebuilt on load). Little-endian throughout.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Compact: assign each live entry a new dense id; a live entry's parent is always live (removal
        // tombstones whole subtrees), so the remap is closed.
        let mut remap = vec![NO_PARENT; self.entries.len()];
        let mut live: Vec<Entry> = Vec::new();
        for (old, e) in self.entries.iter().enumerate() {
            if e.dead {
                continue;
            }
            remap[old] = live.len() as u32;
            live.push(e.clone());
        }
        for e in live.iter_mut() {
            if e.parent != NO_PARENT {
                e.parent = remap[e.parent as usize];
            }
        }

        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&self.volume_id.to_le_bytes());
        out.push(self.truncated as u8);
        // Names: count, then each as (u32 len + utf8 bytes). Names are kept as-is; a few now-unreferenced
        // strings from removed entries are harmless and cheap (filenames only).
        out.extend_from_slice(&(self.names.len() as u32).to_le_bytes());
        for name in &self.names {
            out.extend_from_slice(&(name.len() as u32).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
        }
        // Entries (live only): count, then each as (u32 name, u32 parent, u8 is_dir).
        out.extend_from_slice(&(live.len() as u32).to_le_bytes());
        for e in &live {
            out.extend_from_slice(&e.name.to_le_bytes());
            out.extend_from_slice(&e.parent.to_le_bytes());
            out.push(e.is_dir as u8);
        }
        out
    }

    /// Parse the binary format produced by [`Index::to_bytes`], rebuilding the aux maps. A wrong magic or
    /// format version → [`IndexError::Stale`] (rebuild); any short/garbled body → [`IndexError::Io`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Index, IndexError> {
        let mut r = Reader::new(bytes);
        let magic = r.take(8).ok_or(IndexError::Stale)?;
        if magic != MAGIC {
            return Err(IndexError::Stale);
        }
        let version = r.u32().ok_or(IndexError::Stale)?;
        if version != FORMAT_VERSION {
            return Err(IndexError::Stale);
        }
        let volume_id = r.u64().ok_or_else(|| IndexError::Io("truncated header".into()))?;
        let truncated = r.u8().ok_or_else(|| IndexError::Io("truncated header".into()))? != 0;

        let name_count = r.u32().ok_or_else(|| IndexError::Io("truncated names".into()))?;
        let mut names = Vec::with_capacity(name_count as usize);
        for _ in 0..name_count {
            let len = r.u32().ok_or_else(|| IndexError::Io("truncated name len".into()))?;
            let raw = r.take(len as usize).ok_or_else(|| IndexError::Io("truncated name".into()))?;
            let s = std::str::from_utf8(raw).map_err(|_| IndexError::Io("non-utf8 name".into()))?;
            names.push(s.to_string());
        }

        let entry_count = r.u32().ok_or_else(|| IndexError::Io("truncated entries".into()))?;
        let mut entries = Vec::with_capacity(entry_count as usize);
        for _ in 0..entry_count {
            let name = r.u32().ok_or_else(|| IndexError::Io("truncated entry".into()))?;
            let parent = r.u32().ok_or_else(|| IndexError::Io("truncated entry".into()))?;
            let is_dir = r.u8().ok_or_else(|| IndexError::Io("truncated entry".into()))? != 0;
            if name as usize >= names.len() {
                return Err(IndexError::Io("entry name id out of range".into()));
            }
            entries.push(Entry { name, parent, is_dir, dead: false });
        }

        let mut idx = Index { volume_id, names, entries, truncated, ..Index::default() };
        idx.rebuild_aux();
        Ok(idx)
    }

    /// Persist to `path` (via [`Index::to_bytes`]). Writes to a temp sibling then renames, so a crash never
    /// leaves a half-written index that would look valid.
    pub fn save(&self, path: &Path) -> Result<(), IndexError> {
        let bytes = self.to_bytes();
        let tmp = path.with_extension("cpeidx.tmp");
        std::fs::write(&tmp, &bytes).map_err(|e| IndexError::Io(e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| IndexError::Io(e.to_string()))?;
        Ok(())
    }

    /// Load from `path` (via [`Index::from_bytes`]). A missing file or read error → [`IndexError::Io`]; a
    /// format/magic mismatch → [`IndexError::Stale`] so the caller rebuilds transparently.
    pub fn load(path: &Path) -> Result<Index, IndexError> {
        let bytes = std::fs::read(path).map_err(|e| IndexError::Io(e.to_string()))?;
        Index::from_bytes(&bytes)
    }
}

/// The lowercased extension of `name` without the dot, or `""` for none — matching how CPE-831's
/// [`Candidate::ext`] is populated elsewhere. A leading-dot name (`.gitignore`) has no extension.
fn ext_of(name: &str) -> &str {
    match name.rfind('.') {
        Some(0) | None => "",
        Some(pos) => &name[pos + 1..],
    }
}

/// True if `term` uses glob metacharacters (so trigram pruning can't be applied to it).
fn is_glob(term: &str) -> bool {
    term.contains('*') || term.contains('?') || term.contains('{')
}

/// Pack the case-folded trigrams of `s` (already-lowercased for query terms; callers lowercase names
/// first) into u32 keys. Each key holds three bytes of the (lossy) byte stream; non-ASCII bytes still pack
/// deterministically, so multibyte names index + match consistently. Fewer than 3 bytes yields no trigram
/// (callers fall back to a full scan).
fn trigrams_of(s: &str) -> Vec<u32> {
    let bytes = s.as_bytes();
    if bytes.len() < 3 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(bytes.len() - 2);
    for w in bytes.windows(3) {
        out.push((w[0] as u32) << 16 | (w[1] as u32) << 8 | w[2] as u32);
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Intersect two ascending, deduped id lists. Both posting lists are kept in id order, so this is a linear
/// merge.
fn intersect_sorted(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

/// A tiny cursor over the index byte buffer — each `take`/`uN` advances and returns `None` past the end, so
/// a truncated file degrades to an `IndexError` instead of panicking.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&mut self) -> Option<u64> {
        self.take(8)
            .map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::AtomicU64;
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-index-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Build a small tree, index it, and assert queries return the right names.
    fn sample_tree() -> std::path::PathBuf {
        let d = scratch("tree");
        fs::create_dir_all(d.join("src")).unwrap();
        fs::create_dir_all(d.join("docs")).unwrap();
        fs::create_dir_all(d.join(".git")).unwrap(); // dot-dir — not descended
        fs::write(d.join("README.md"), b"x").unwrap();
        fs::write(d.join("src/main.rs"), b"x").unwrap();
        fs::write(d.join("src/report.rs"), b"x").unwrap();
        fs::write(d.join("docs/report.md"), b"x").unwrap();
        fs::write(d.join(".git/report_hidden.rs"), b"x").unwrap();
        d
    }

    fn names_of(hits: &[IndexHit]) -> Vec<&str> {
        hits.iter().map(|h| h.name.as_str()).collect()
    }

    fn sorted_names(hits: &[IndexHit]) -> Vec<&str> {
        let mut v = names_of(hits);
        v.sort();
        v
    }

    fn build(root: &std::path::Path) -> Index {
        Index::build(&root.to_string_lossy(), 1, &AtomicBool::new(false)).unwrap().0
    }

    #[test]
    fn build_indexes_recursively_and_skips_dot_dirs() {
        let d = sample_tree();
        let (idx, stats) = Index::build(&d.to_string_lossy(), 42, &AtomicBool::new(false)).unwrap();
        assert_eq!(idx.volume_id(), 42);
        assert!(!stats.truncated);
        let hits = idx.search(&index_query::parse("report"), 100);
        assert_eq!(sorted_names(&hits), vec!["report.md", "report.rs"]);
        assert!(!hits.iter().any(|h| h.name == "report_hidden.rs")); // dot-dir never indexed
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn search_respects_ext_and_path_filters_and_globs() {
        let d = sample_tree();
        let idx = build(&d);
        let rs = idx.search(&index_query::parse("report ext:rs"), 100);
        assert_eq!(names_of(&rs), vec!["report.rs"]);
        let docs = idx.search(&index_query::parse("report path:docs"), 100);
        assert_eq!(names_of(&docs), vec!["report.md"]);
        let globbed = idx.search(&index_query::parse("*.rs"), 100);
        assert_eq!(sorted_names(&globbed), vec!["main.rs", "report.rs"]);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn search_reconstructs_absolute_paths() {
        let d = sample_tree();
        let idx = build(&d);
        let hits = idx.search(&index_query::parse("main"), 10);
        assert_eq!(hits.len(), 1);
        // The path is absolute: it starts with the crawl root and ends with the on-disk relative path.
        let root = d.to_string_lossy();
        assert!(hits[0].path.starts_with(root.as_ref()), "path was {}", hits[0].path);
        let want_tail: String = ["src", "main.rs"].join(std::path::MAIN_SEPARATOR_STR);
        assert!(hits[0].path.ends_with(&want_tail), "path was {}", hits[0].path);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn empty_query_returns_nothing() {
        let d = sample_tree();
        let idx = build(&d);
        assert!(idx.search(&index_query::parse("   "), 10).is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn save_reload_roundtrips_and_still_queries() {
        let d = sample_tree();
        let idx = Index::build(&d.to_string_lossy(), 7, &AtomicBool::new(false)).unwrap().0;
        let file = d.join("volume.cpeidx");
        idx.save(&file).unwrap();
        let reloaded = Index::load(&file).unwrap();
        assert_eq!(reloaded.volume_id(), 7);
        assert_eq!(reloaded.len(), idx.len());
        assert_eq!(sorted_names(&reloaded.search(&index_query::parse("report"), 100)), vec!["report.md", "report.rs"]);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn load_rejects_bad_magic_and_wrong_version_as_stale() {
        assert!(matches!(Index::from_bytes(b"not-an-index"), Err(IndexError::Stale)));
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        assert!(matches!(Index::from_bytes(&bytes), Err(IndexError::Stale)));
    }

    #[test]
    fn from_bytes_reports_io_on_truncated_body() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes()); // volume_id
        bytes.push(0); // truncated flag
        bytes.extend_from_slice(&5u32.to_le_bytes()); // claims 5 names, but none follow
        assert!(matches!(Index::from_bytes(&bytes), Err(IndexError::Io(_))));
    }

    #[test]
    fn build_cancellation_yields_partial_truncated_index() {
        let d = scratch("cancel");
        fs::create_dir_all(d.join("a")).unwrap();
        fs::write(d.join("a/x.txt"), b"x").unwrap();
        let cancel = AtomicBool::new(true); // already cancelled → stops before scanning children
        let (idx, stats) = Index::build(&d.to_string_lossy(), 1, &cancel).unwrap();
        assert!(stats.truncated);
        assert!(idx.truncated());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn build_errors_on_non_folder_root() {
        let d = scratch("file");
        let f = d.join("plain.txt");
        fs::write(&f, b"x").unwrap();
        assert!(Index::build(&f.to_string_lossy(), 1, &AtomicBool::new(false)).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn ext_of_handles_dotfiles_and_no_extension() {
        assert_eq!(ext_of("report.md"), "md");
        assert_eq!(ext_of("archive.tar.gz"), "gz");
        assert_eq!(ext_of("Makefile"), "");
        assert_eq!(ext_of(".gitignore"), "");
    }

    #[test]
    fn trigram_pruning_matches_full_scan_results() {
        let d = sample_tree();
        let idx = build(&d);
        let pruned = idx.search(&index_query::parse("report"), 100);
        let scanned = idx.search(&index_query::parse("re"), 100);
        for h in &pruned {
            assert!(scanned.iter().any(|s| s.path == h.path), "pruning dropped {}", h.path);
        }
        let _ = fs::remove_dir_all(&d);
    }

    // ---- CPE-833: incremental mutation ----

    /// Path under the crawl root, using the platform separator, for feeding the apply_* primitives.
    fn abs(root: &std::path::Path, rel: &[&str]) -> String {
        let mut p = root.to_path_buf();
        for c in rel {
            p.push(c);
        }
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn apply_create_adds_a_findable_entry_under_an_indexed_dir() {
        let d = sample_tree();
        let mut idx = build(&d);
        assert!(idx.search(&index_query::parse("newfile"), 10).is_empty());
        let id = idx.apply_create(&abs(&d, &["src", "newfile.rs"]), false);
        assert!(id.is_some());
        let hits = idx.search(&index_query::parse("newfile"), 10);
        assert_eq!(names_of(&hits), vec!["newfile.rs"]);
        assert!(hits[0].path.ends_with(&["src", "newfile.rs"].join(std::path::MAIN_SEPARATOR_STR)));
        // A create whose parent isn't indexed is ignored.
        assert!(idx.apply_create(&abs(&d, &["nope", "ghost.rs"]), false).is_none());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn apply_remove_tombstones_the_whole_subtree() {
        let d = sample_tree();
        let mut idx = build(&d);
        // Removing the src/ dir must drop both files under it.
        assert!(idx.apply_remove(&abs(&d, &["src"])));
        let report = idx.search(&index_query::parse("report"), 10);
        assert_eq!(names_of(&report), vec!["report.md"]); // only the docs one survives
        assert!(idx.search(&index_query::parse("main"), 10).is_empty()); // src/main.rs gone
        // Removing something not indexed is a no-op.
        assert!(!idx.apply_remove(&abs(&d, &["does", "not", "exist"])));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn apply_rename_in_place_and_across_dirs() {
        let d = sample_tree();
        let mut idx = build(&d);
        // In-place rename README.md -> CHANGES.md.
        assert!(idx.apply_rename(&abs(&d, &["README.md"]), &abs(&d, &["CHANGES.md"])));
        assert!(idx.search(&index_query::parse("readme"), 10).is_empty());
        assert_eq!(names_of(&idx.search(&index_query::parse("changes"), 10)), vec!["CHANGES.md"]);
        // Move src/main.rs -> docs/main.rs; its reconstructed path must now be under docs/.
        assert!(idx.apply_rename(&abs(&d, &["src", "main.rs"]), &abs(&d, &["docs", "main.rs"])));
        let hits = idx.search(&index_query::parse("main"), 10);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with(&["docs", "main.rs"].join(std::path::MAIN_SEPARATOR_STR)), "{}", hits[0].path);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn moving_a_directory_carries_its_subtree() {
        let d = sample_tree();
        let mut idx = build(&d);
        // Move the whole src/ dir to docs/src — report.rs under it must follow to the new path.
        assert!(idx.apply_rename(&abs(&d, &["src"]), &abs(&d, &["docs", "src"])));
        let hits = idx.search(&index_query::parse("report.rs"), 10);
        assert_eq!(hits.len(), 1);
        let tail = ["docs", "src", "report.rs"].join(std::path::MAIN_SEPARATOR_STR);
        assert!(hits[0].path.ends_with(&tail), "subtree didn't follow: {}", hits[0].path);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn incremental_mutations_match_a_fresh_rebuild() {
        // Build A, mutate it; build B directly on the equivalent on-disk shape; assert same search results.
        let d = sample_tree();
        let mut a = build(&d);
        // Apply: add src/extra.rs, remove docs/report.md, rename README.md -> TOP.md — mirror on disk for B.
        a.apply_create(&abs(&d, &["src", "extra.rs"]), false);
        fs::write(d.join("src/extra.rs"), b"x").unwrap();
        a.apply_remove(&abs(&d, &["docs", "report.md"]));
        fs::remove_file(d.join("docs/report.md")).unwrap();
        a.apply_rename(&abs(&d, &["README.md"]), &abs(&d, &["TOP.md"]));
        fs::rename(d.join("README.md"), d.join("TOP.md")).unwrap();
        let b = build(&d);

        for q in ["report", "extra", "top", "*.rs", "ext:md", "main"] {
            let query = index_query::parse(q);
            let ap: Vec<String> = a.search(&query, 999).into_iter().map(|h| h.path).collect();
            let bp: Vec<String> = b.search(&query, 999).into_iter().map(|h| h.path).collect();
            assert_eq!(ap, bp, "query {q:?}: incremental != rebuild");
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn to_bytes_compacts_tombstones_away() {
        let d = sample_tree();
        let mut idx = build(&d);
        let before = idx.len();
        idx.apply_remove(&abs(&d, &["src"])); // tombstones src, main.rs, report.rs
        let after_remove = idx.len();
        assert!(after_remove < before);
        // Reload from serialised bytes: dead entries are gone, and the survivors still query correctly.
        let reloaded = Index::from_bytes(&idx.to_bytes()).unwrap();
        assert_eq!(reloaded.len(), after_remove);
        assert_eq!(names_of(&reloaded.search(&index_query::parse("report"), 10)), vec!["report.md"]);
        // And a fresh rebuild after mirroring the delete on disk agrees.
        fs::remove_dir_all(d.join("src")).unwrap();
        let fresh = build(&d);
        assert_eq!(reloaded.len(), fresh.len());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn create_revives_a_removed_entry_in_place() {
        let d = sample_tree();
        let mut idx = build(&d);
        idx.apply_remove(&abs(&d, &["docs", "report.md"]));
        assert_eq!(names_of(&idx.search(&index_query::parse("report"), 10)), vec!["report.rs"]);
        // Re-create the same path → it's findable again.
        idx.apply_create(&abs(&d, &["docs", "report.md"]), false);
        assert_eq!(sorted_names(&idx.search(&index_query::parse("report"), 10)), vec!["report.md", "report.rs"]);
        let _ = fs::remove_dir_all(&d);
    }
}
