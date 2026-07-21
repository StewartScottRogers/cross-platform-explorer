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

/// Location model + URI parser: classify a location as local vs. a remote scheme
/// (`sftp`/`smb`/`webdav`/`s3`) broken into `{scheme,user,host,port,path}` (CPE-680).
pub mod location;

/// Filesystem-provider abstraction: the trait every location backend implements — local disk
/// today, remote backends later — plus a `LocalProvider` and an in-memory `FakeProvider` (CPE-681).
pub mod provider;

/// Tag store: user tags + a colour label per path, persisted as `tags.json` (CPE-635). Pure model
/// helpers + `ServerCtx`-based command entry points (CPE-815 migration).
pub mod tags;

/// Settings store: the single on-disk `settings.json` document (CPE-226). Pure helpers +
/// `ServerCtx`-based entry points (CPE-815 migration).
pub mod settings;

/// Pure window-geometry resolver for the CLI launch options (CPE-598).
pub mod geometry;

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

/// Disk-usage scanning — recursive directory size + per-child breakdown (CPE-749/754).
pub mod disk_usage;

/// Duplicate-file finder — size-then-hash two-pass scan (CPE-420).
pub mod duplicates;

/// Filename search — substring/glob/brace-group matching + the shared streaming walker (CPE-603/697/666).
pub mod name_search;

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
