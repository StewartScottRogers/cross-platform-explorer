//! # cpe-server
//!
//! The Cross-Platform Explorer **Server**: Tauri-free domain logic (epic CPE-810, ticket
//! CPE-815). It depends only on the runtime seam ([`ctx::ServerCtx`], CPE-814) and the wire
//! [`contract`] envelope (CPE-811) — never on Tauri — so the same Server drives the explorer
//! locally in-process today and, later, headless or remote behind a network transport.
//!
//! The Tauri app is a thin adapter: it provides the concrete `TauriCtx` implementation of
//! [`ctx::ServerCtx`] and dispatches to this crate's domain logic. This first extraction moves
//! the runtime seam and the filesystem-domain core (the location model + the
//! [`provider::FileSystemProvider`] abstraction) out of the app; the remaining command bodies
//! migrate here progressively (the app stays byte-for-byte behaviour-identical at each step).

/// The wire contract, re-exported so downstream consumers reach it through the Server (the
/// dependency direction the epic establishes: GUI/adapter → Server → contract).
pub use cpe_contract as contract;

/// The runtime seam abstracting host services (dir resolution, event emit, cancellation) off
/// the domain logic (CPE-814).
pub mod ctx;

/// Scriptable-action / user-macro model: a named sequence of rename/move/tag/convert steps, its
/// validation, and a filesystem-free expansion (`plan`) over a selection of inputs (CPE-938, epic CPE-739).
pub mod action_macro;

/// Location model + URI parser: classify a location as local vs. a remote scheme
/// (`sftp`/`smb`/`webdav`/`s3`) broken into `{scheme,user,host,port,path}` (CPE-680).
pub mod location;

/// Filesystem-provider abstraction: the trait every location backend implements — local disk
/// today, remote backends later — plus a `LocalProvider` and an in-memory `FakeProvider` (CPE-681).
pub mod provider;

/// SSH `known_hosts` parsing + host-key verification (TOFU / changed-key detection) — the pure security
/// core of the future SFTP provider, decoupled from any ssh crate so it's headless-testable (CPE-682).
pub mod known_hosts;

/// Saved remote-connection profiles (the persisted list behind the "Connections" sidebar, secrets
/// excluded — those live in the OS keychain). Pure data + JSON persistence (CPE-683).
pub mod connections;

/// Provider-agnostic recursive walk + bidirectional tree transfer over the `FileSystemProvider` trait,
/// so every backend (local/SFTP/WebDAV) shares one cancellable enumeration + copy (CPE-905).
pub mod transfer;

/// Lightweight, dependency-free source-symbol outline (functions/types/classes/headings per language) for
/// a jump-to-symbol code preview (CPE-910, epic CPE-724).
pub mod code_outline;

/// Tag store: user tags + a colour label per path, persisted as `tags.json` (CPE-635). Pure model
/// helpers + `ServerCtx`-based command entry points (CPE-815 migration).
pub mod tags;

/// Native metadata I/O: read/write/remove a named blob via NTFS ADS (Windows) or POSIX xattr (Unix),
/// with graceful `Unsupported` degradation — the storage primitive of the native-metadata bridge
/// (CPE-826, epic CPE-717).
pub mod native_meta;

/// Tag reconciliation + portable metadata codec: the pure push/pull policy bridging the internal tag
/// store and a path's native metadata, plus CPE's portable `{tags,label}` blob (CPE-827, epic CPE-717).
pub mod native_tags;

/// macOS Finder-tag codec: `_kMDItemUserTags` binary plist ⇄ `Vec<FinderTag>` (name + colour), for
/// Finder interop (CPE-829, epic CPE-717). Cross-platform, round-trip-testable everywhere.
pub mod finder_tags;

/// Native bridge orchestration: the per-OS `pull`/`push` glue wiring native_meta + the codecs +
/// reconciliation into a working bridge between native file metadata and the tag store (CPE-830).
pub mod native_bridge;

/// Settings store: the single on-disk `settings.json` document (CPE-226). Pure helpers +
/// `ServerCtx`-based entry points (CPE-815 migration).
pub mod settings;

/// Pure window-geometry resolver for the CLI launch options (CPE-598).
pub mod geometry;

/// Activity timeline bucketing (scrubbable replay view) over recorded audit events (CPE-916).
pub mod activity_timeline;
pub mod spotlight;
pub mod metadata_column;
pub mod restore_plan;

/// On-disk append-only session audit journal (CPE-800, epic CPE-733): durable per-session
/// JSON-lines of Agent Watch filesystem activity, bounded/rotated. Pure helpers over a base dir.
pub mod audit_journal;

/// Agent Board backend (CPE-520): read the repo's `Tickets/` folders as Kanban cards + move a card
/// between columns. Pure card/frontmatter logic (the Tauri commands do the file I/O).
pub mod ticket_board;

/// Small shared filesystem utilities: epoch-ms time conversion + streaming SHA-256 (CPE-815).
pub mod fsutil;

/// Text statistics — line/word/char/byte counts for a text file (CPE-414).
pub mod text_stats;

/// File + folder-tree SHA-256 checksums (CPE-412) and the integrity-baseline manifest (CPE-791).
pub mod checksum;

/// Folder statistics — recursive file/dir/byte totals (CPE-649).
pub mod folder_stats;

/// File comparison — byte-identical check (CPE-418).
pub mod compare;

/// Snapshot retention / thinning — grandfather-father-son (hourly/daily/weekly/monthly) keep-vs-prune
/// policy over a list of local snapshots (CPE-944, epic CPE-735). Pure; the engine takes/deletes them.
pub mod snapshot_retention;

/// Disk-usage scanning — recursive directory size + per-child breakdown (CPE-749/754).
pub mod disk_usage;

/// Duplicate-file finder — size-then-hash two-pass scan (CPE-420).
pub mod duplicates;

/// Filename search — substring/glob/brace-group matching + the shared streaming walker (CPE-603/697/666).
pub mod name_search;

/// Instant-search query core — parse `ext:`/`path:`/name-term queries, match candidates, rank by
/// relevance. Backend-agnostic; reused by the index engine (CPE-831, epic CPE-703).
pub mod index_query;

/// Folder templates — capture a folder structure as a reusable template and stamp it out with `{token}`
/// substitution, path-safe and non-destructive (CPE-835, epic CPE-740).
pub mod folder_template;

/// Content search — recursive line search in text files, bounded + binary-skipping (CPE-416).
pub mod content_search;

/// Shared filesystem model types (DirEntry / EntryInfo / Place / OpResult) + extension/hidden helpers.
pub mod model;

/// Directory listing — the shared walker behind `list_dir` + its streaming variant (CPE-663/662).
pub mod listing;

/// Link forge — create symbolic + hard links (CPE-802).
pub mod links;

/// Archive listing — browse into zip/tar/gzip/7z/iso without extracting (CPE-064/109/110/113).
pub mod archive;

/// Structured binary previews — hex / PE / MIDI / wasm / torrent text summaries (CPE-210/214/215/216/218).
pub mod binary_preview;

/// Document text extraction — RTF / DOCX / ODT / EPUB → plain text (CPE-070/071/072/077).
pub mod doc_text;

/// Structured-data previews — SQLite / spreadsheet / Parquet summaries (CPE-088/089/090/091).
pub mod data_preview;

/// Structured-data browser — schema + paged rows for SQLite / Parquet / Excel-ODS, the reader behind the
/// data-grid (CPE-847, epic CPE-721).
pub mod data_browser;

/// Image thumbnails — downscaled PNG generation + mtime-keyed disk cache (CPE-642/644).
pub mod thumbnail;

/// Thumbnail cache core — stable cache keys + LRU eviction (count + byte budgets) + request coalescing,
/// the pure cache-management model the universal thumbnail pipeline sits on (CPE-939, epic CPE-718).
pub mod thumb_cache;

/// Image preview — TIFF/PSD → PNG data-URL transcode + dimensions/EXIF metadata (CPE-099/101/659).
pub mod image_preview;

/// Backup copy engine — plan executor (copy/update/mirror-delete) with per-file results (CPE-797).
pub mod backup;

/// Server-side contract dispatch — route `Request` envelopes to domain functions (CPE-824, the Server
/// half of the remote RPC loop CPE-820).
pub mod dispatch;
