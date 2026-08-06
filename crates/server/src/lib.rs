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

/// Scheme-based routing seam: classify a location by scheme and pick its `FileSystemProvider` — local
/// today, remote schemes cleanly rejected until wired in. The single dispatch point the FS commands guard
/// through so remote URIs fail consistently and local behaviour is unchanged (CPE-685, epic CPE-616).
pub mod fs_route;

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

/// Lightweight, dependency-free foldable block-range heuristic (brace nesting / indentation suites /
/// markdown sections per language) for a code preview's fold gutter (CPE-1050, epic CPE-724).
pub mod code_folds;

/// Enclosing-symbol breadcrumb: combines `code_outline` + `code_folds` to return the outermost→innermost
/// symbol chain containing a given line, for a jump-to / breadcrumb UI (CPE-1053, epic CPE-724).
pub mod code_breadcrumb;

/// Aggregate code-intelligence bundle: composes `code_outline` + `code_folds` + `indent_guides` +
/// `minimap` into one `CodeIntel { outline, folds, indent, minimap }` payload for a single-round-trip
/// code preview (CPE-1089, epic CPE-724).
pub mod code_intel;

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

/// Pure resolver for the `--open <dir>` launch argument — open the explorer at a folder on startup
/// (CPE-1043). Filesystem-free (existence check injected), so it's unit-testable.
pub mod launch;

/// Activity timeline bucketing (scrubbable replay view) over recorded audit events (CPE-916).
pub mod activity_timeline;

/// Replay projection core (CPE-1063, epic CPE-728): fold the recorded audit-event stream into the live
/// file-set state at any moment `T`. Pure event-sourcing fold over [`audit_journal::AuditEvent`].
pub mod replay;

/// Replay folder-view projection + transition diff (CPE-1065, epic CPE-728): direct children of a
/// folder at moment `T` (`children_at`) and the added/removed/modified diff between two scrub
/// cursors (`diff_states`), to animate a scrub. Pure projection over [`replay::FsState`].
pub mod replay_view;

/// Baseline snapshot for activity replay (CPE-1109, epic CPE-728): capture the pre-existing entries
/// under a watched root once at watch-start (a bounded recursive walk reusing [`listing`]), persist it
/// as a separate `<session>.baseline.json` (representation B — never synthetic events in the journal),
/// and seed [`replay::state_at_from`] from it so pre-existing untouched files appear in a reconstruction.
pub mod replay_baseline;
pub mod batch_media;

/// Batch media **transform engine** (CPE-1083, epic CPE-723): applies an ordered list of
/// [`batch_media::MediaOp`]s to an image's bytes (decode once → fold ops → encode) — the executor for
/// the plan [`batch_media::plan`] computes. Pure bytes→bytes; no filesystem I/O.
pub mod batch_transform;

/// Batch execute runner (CPE-1084, epic CPE-723): ties [`batch_media::plan`]'s planned outputs to the
/// [`batch_transform::apply_ops`] engine — reads each planned item's input bytes, transforms them, and
/// writes the result to its planned output. Skip-on-error, non-destructive, real filesystem I/O.
pub mod batch_execute;
pub mod spotlight;
pub mod type_class;
pub mod metadata_column;
pub mod restore_plan;
pub mod revert_safety;

/// On-disk append-only session audit journal (CPE-800, epic CPE-733): durable per-session
/// JSON-lines of Agent Watch filesystem activity, bounded/rotated. Pure helpers over a base dir.
pub mod audit_journal;

/// On-disk append-only per-session **cost/activity metrics** journal (CPE-1113, epic CPE-731 slice b):
/// a sibling of [`audit_journal`] at a coarser grain — one [`metrics_journal::SessionMetricsRecord`] row
/// per ended Agent Watch session, in a single bounded/rotated `agent-metrics/history.jsonl`, so the cost
/// dashboard can show cross-session history. Pure helpers over a base dir; advisory/best-effort figures.
pub mod metrics_journal;
pub mod revert_attribution;

/// Scrubber transport model for activity replay: step to next/previous distinct event ts, slice a
/// half-open playback window, and advance the playhead by a speed-scaled delta (CPE-1064, epic
/// CPE-728). Pure `ts`-only math, independent of the projection core.
pub mod replay_transport;

/// Journal-backed replay assembly (CPE-1066, epic CPE-728): read a session's durable audit journal
/// back from disk and package it for replay (events + time bounds + summary). Reuses
/// `audit_journal::read_session`, `replay::bounds`, and `activity_timeline::summarize`.
pub mod replay_session;

/// Agent Board backend (CPE-520): read the repo's `Ticketing/` folders as Kanban cards + move a card
/// between columns. Pure card/frontmatter logic (the Tauri commands do the file I/O).
pub mod ticket_board;

/// Ticket-directives MCP surface (CPE-962): expose the board's `## Agent Directives` over the Model
/// Context Protocol so any MCP agent can list + reply to directives. Pure dispatch; the `ticket-mcp` bin
/// serves it over stdio.
pub mod ticket_mcp;

/// Small shared filesystem utilities: epoch-ms time conversion + streaming SHA-256 (CPE-815).
pub mod fsutil;

/// Text statistics — line/word/char/byte counts for a text file (CPE-414).
pub mod text_stats;

/// File + folder-tree SHA-256 checksums (CPE-412) and the integrity-baseline manifest (CPE-791).
pub mod checksum;

/// Compress-selection planner (CPE-1056, epic CPE-705): selection → inner archive-entry names,
/// with collision detection.
pub mod compress_plan;

/// Folder statistics — recursive file/dir/byte totals (CPE-649).
pub mod folder_stats;

/// Thumbnail request queue — priority scheduler (Visible > Prefetch > Background) with dedupe + promotion,
/// the pure scheduling core of the thumbnail pipeline (CPE-950, epic CPE-718). Pairs with thumb_cache.
pub mod thumb_queue;

/// Thumbnail pipeline glue — drives `thumb_queue` + `thumb_cache` from a batch of requests, the seam a
/// Tauri command dispatches into (CPE-1237, epic CPE-718).
pub mod thumb_pipeline;

/// File comparison — byte-identical check (CPE-418).
pub mod compare;

/// Snapshot retention / thinning — grandfather-father-son (hourly/daily/weekly/monthly) keep-vs-prune
/// policy over a list of local snapshots (CPE-944, epic CPE-735). Pure; the engine takes/deletes them.
pub mod snapshot_retention;

/// Content-addressed snapshot store — a refcounted, deduplicated, bounded blob store + capture/release
/// lifecycle behind checkpoint & rollback (CPE-969, epic CPE-732; also CPE-735). Pure; feeds
/// [`restore_plan`]'s revert diff. The bytes behind each hash are the caller's to persist.
pub mod snapshot;

/// Disk-backed snapshot capture & restore engine (CPE-1011, epic CPE-735) — the persistence half
/// [`snapshot`] leaves to the caller: walk a folder into a [`restore_plan::Snapshot`], write new blobs
/// content-addressed under a store directory (deduped), and restore a captured manifest back to a
/// directory byte-for-byte. Std + serde only; not feature-gated.
pub mod snapshot_capture;

/// Retention-prune command layer (CPE-1196, epic CPE-735): wires the pure grandfather-father-son policy
/// ([`snapshot_retention::thin`]) to the real on-disk store — enumerate a root's manifests, preview a
/// keep/prune decision non-destructively, and apply it via [`snapshot_capture::prune`] (+ an optional
/// total-byte cap). Store-dir-based, like `snapshot_capture` itself.
pub mod snapshot_prune;

/// Snapshot schedule rules store + pure `due()` policy (CPE-1198, epic CPE-735): a persisted per-folder
/// rule (path, interval, retention policy, enabled), mirroring the `column_config` store pattern, and a
/// deterministic decision over which roots are due for a fresh capture at an injected `now`. No timer, no
/// UI here — [`snapshot_schedule::snapshot_run_due`] is a single pass a caller (timer or "run now" button)
/// invokes.
pub mod snapshot_schedule;

/// Revert-plan execution engine (CPE-1081, epic CPE-732) — applies a minimal [`restore_plan::RestoreAction`]
/// plan (Create/Overwrite/Delete) to a REAL directory: the surgical, current-state-aware counterpart to
/// [`snapshot_capture::restore`]'s whole-manifest replay. Std-only; reuses `restore_plan`/`snapshot_capture`
/// without modifying them.
pub mod revert_engine;

/// Per-watched-root checkpoint store (CPE-1123, epic CPE-732) — the command-layer glue that wires the
/// checkpoint/rollback engine (`snapshot_capture` + `restore_plan` + `revert_safety` + `revert_engine`)
/// into a live store: capture a checkpoint, keep a tolerant JSON-lines index per root (mirroring
/// `audit_journal`'s on-disk pattern), preview a revert (plan + drift), and revert whole-tree or a single
/// path. Reaches the app-data dir through the `ServerCtx` seam; the Tauri commands are one-liners into it.
pub mod checkpoint_store;

/// Disk-usage scanning — recursive directory size + per-child breakdown (CPE-749/754).
pub mod disk_usage;

/// Tray quick-access list — bounded pinned + recent entries (add/pin/unpin/remove) for the system-tray
/// menu, the pure model behind the tray-resident quick-access (CPE-946, epic CPE-713).
pub mod tray_quick;

/// Duplicate-file finder — size-then-hash two-pass scan (CPE-420).
pub mod duplicates;

/// Organization / declutter rules engine — pure, deterministic, filesystem-free planner that maps each
/// file to a proposed destination subfolder under a chosen rule (by kind / extension / modified-year /
/// size). No AI, no I/O (CPE-987, epic CPE-979); a later move engine executes the proposals.
pub mod organize;

/// Rules-based auto-organize command layer (CPE-1142, epic CPE-979): lists a directory, maps it to
/// `organize::OrganizeEntry`s, and — on apply — checkpoints the tree first, then executes the plan on
/// disk. The I/O + `ServerCtx` glue `organize` itself deliberately stays free of.
pub mod organize_apply;

/// Spotlight frecency — rank recently+frequently used items (count × recency-decay) for the overlay's
/// default empty-query view (CPE-952, epic CPE-704). Pure + deterministic.
pub mod spotlight_frecency;

/// Orphaned-sidecar-file detector — pure, deterministic scan of a caller-supplied directory listing that
/// flags companion files (subtitles, metadata sidecars, thumbnails) whose primary media file is gone
/// (CPE-1006, epic CPE-1002). No AI, no I/O, no new deps.
pub mod orphan_sidecars;

/// Orphaned-sidecar **scan** pipeline (CPE-1283, epic CPE-1002): walks a folder (optionally
/// recursive, skip-unreadable), groups its files per directory, and calls [`orphan_sidecars::
/// find_orphans`] on each group — the adapter [`orphan_sidecars`]'s own docs describe as the
/// caller's job. Command/UI wiring is a separate ticket (CPE-1287).
pub mod orphan_sidecars_scan;

/// Filename search — substring/glob/brace-group matching + the shared streaming walker (CPE-603/697/666).
pub mod name_search;

/// Search date/age filter — parse relative (`<7d`, `today`, `yesterday`) and absolute (`2024`,
/// `2024-07`, `2024-07-25`) date expressions into a [`date_filter::DateFilter`] and test a modified-time
/// against it. Pure, UTC-epoch-seconds only, `now` injected for deterministic tests (CPE-1060, epic
/// CPE-703). Standalone — does not touch [`index_query`].
pub mod date_filter;

/// Embedded terminal dock tab model — open/close/activate/rename terminal tabs with active-tab fixup, the
/// pure bookkeeping behind the terminal panel (CPE-947, epic CPE-714). No PTY here.
pub mod terminal_tabs;

/// Instant-search query core — parse `ext:`/`path:`/name-term queries, match candidates, rank by
/// relevance. Backend-agnostic; reused by the index engine (CPE-831, epic CPE-703).
pub mod index_query;

/// Search size filter — parse human size expressions (`>1mb`, `<=500k`, `1mb..1gb`, `2.5g`) into a
/// predicate over a candidate's byte count (CPE-1059, epic CPE-703). Standalone; does not touch
/// [`index_query`] — wiring a `size:` token into that grammar is a separate later ticket.
pub mod size_filter;

/// Instant-search **index engine** — the compact per-volume filename index (crawl + on-disk store + trigram
/// pruning) that feeds [`index_query`]. Feature-gated OFF by default (`index`) so the plain build compiles
/// zero indexer code (CPE-832, epic CPE-703).
#[cfg(feature = "index")]
pub mod index;

/// Instant-search index **service** — holds the resident per-volume [`index::Index`]es in memory, persists
/// them, and merges a query across every resident volume; the thin state layer behind the `index_*` Tauri
/// commands (CPE-1137, epic CPE-703). Same `index` feature gate as the engine, so the plain build compiles
/// zero index code.
#[cfg(feature = "index")]
pub mod index_service;

/// Live index watch — pure event→mutation mapping between a normalized filesystem-watch event and the
/// [`index::Index`] incremental primitives (CPE-1138, epic CPE-703). No `notify` dependency here; the OS
/// watcher subscription is the thin `src-tauri` adapter. Same `index` feature gate as the engine/service.
#[cfg(feature = "index")]
pub mod index_watch;

/// Spotlight result aggregation — merge fuzzy-ranked hits from several sources (actions/folders/files/
/// recents) into grouped, per-kind-capped, best-first results for the overlay (CPE-948, epic CPE-704).
pub mod spotlight_results;

/// Folder templates — capture a folder structure as a reusable template and stamp it out with `{token}`
/// substitution, path-safe and non-destructive (CPE-835, epic CPE-740).
pub mod folder_template;

/// Media playlist / queue model — ordered tracks + a cursor with repeat (off/one/all) + seeded shuffle
/// navigation, the pure core behind the audio/video player pane (CPE-943, epic CPE-720).
pub mod playlist;

/// Content search — recursive line search in text files, bounded + binary-skipping (CPE-416).
pub mod content_search;

/// User macro library — a persisted, ordered store of named action-macros with CRUD + validation +
/// reordering, built on action_macro (CPE-951, epic CPE-739). Pure model, JSON round-trips.
pub mod macro_library;

/// Action-macro persistence store — a `macros.json` catalog (name → `ActionMacro`) in the app
/// config dir, mirroring the `folder_template` store pattern (CPE-1033, epic CPE-739).
pub mod macro_store;

/// Macro executor resolution + undo model — resolves `(ActionMacro, inputs)` into a collision-safe,
/// scope-checked, reversible op list; the actual disk writes happen in the app's `macro_run`
/// command, driven by this module's plan (CPE-1187, epic CPE-739).
pub mod macro_run;

/// Shared filesystem model types (DirEntry / EntryInfo / Place / OpResult) + extension/hidden helpers.
pub mod model;

/// Directory listing — the shared walker behind `list_dir` + its streaming variant (CPE-663/662).
pub mod listing;

/// Link forge — create symbolic + hard links (CPE-802).
pub mod links;

/// Same-volume detection (CPE-1026, epic CPE-661): decide whether two paths live on the same
/// volume — the foundation for copy-vs-move on drag-drop. Pure `std` (device-id compare on Unix,
/// drive/UNC-prefix compare on Windows); no new dependencies.
pub mod volume;

/// Network-share address parser (pure model): normalizes a user-typed `smb://`/`nfs://`/`ftp://`/
/// `sftp://` URL or Windows UNC path into a `{protocol,host,share,path}` struct, so the future
/// "connect to server" dialog and the mount glue share one tested understanding of an address
/// (CPE-1024, epic CPE-716). No network or mount I/O here.
pub mod net_share;

/// Shared anchored-wildcard (glob) matcher — one `*`/`?` matcher behind [`selection`], [`name_search`],
/// and the tree-scan exclude-glob support (CPE-1302). Pure: no filesystem access.
pub mod glob;

/// Select-by-pattern engine — glob/extension/kind selection queries over a listing, plus `Invert`
/// (CPE-1025, epic CPE-711). Pure: no filesystem access.
pub mod selection;

/// Shell context-menu model — registered verbs + which apply to a given selection, the pure core behind
/// "CPE as a shell citizen" (CPE-945, epic CPE-712). Per-OS shell registration is separate glue.
pub mod shell_menu;

/// Archive listing — browse into zip/tar/gzip/7z/iso without extracting (CPE-064/109/110/113).
pub mod archive;

/// Archive container format detector — pure magic-byte + extension sniff, no archive crate (CPE-1054,
/// epic CPE-705).
pub mod archive_format;

/// Per-line indent-guide depth for a code preview (CPE-1051, epic CPE-724).
pub mod indent_guides;

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

/// EXIF-orientation correction for thumbnails (CPE-1085, epic CPE-718): read a JPEG's EXIF
/// `Orientation` tag and bake it into the decoded pixels via the standard 8-value rotate/flip
/// table, so phone photos thumbnail upright. A documented local copy of the proven logic in
/// [`batch_transform`] (kept private there), exposed here as public reusable free functions —
/// same pattern as CPE-1079's rename-marker copy.
pub mod thumb_orient;

/// Thumbnail cache core — stable cache keys + LRU eviction (count + byte budgets) + request coalescing,
/// the pure cache-management model the universal thumbnail pipeline sits on (CPE-939, epic CPE-718).
pub mod thumb_cache;

/// Thumbnail source decode — PSD (via vendored `psd`) + bomb-guarded raster decode, returning the raw
/// bytes alongside the image so the caller can apply EXIF orientation (CPE-1086, epic CPE-718).
pub mod thumb_source;

/// SVG thumbnail rasterization via `resvg`/`usvg`/`tiny-skia`, bomb-guarded against an implausible
/// declared canvas size (CPE-1236, epic CPE-718).
pub mod thumb_svg;

/// Font glyph-sheet thumbnails (`.ttf`/`.otf`/`.woff`) via `ab_glyph`, including a WOFF1->SFNT
/// unwrapper reusing the already-vendored `flate2` (CPE-1236, epic CPE-718).
pub mod thumb_font;

/// PDF first-page thumbnail rendering via `pdfium-render` driving a dynamically-loaded pdfium
/// prebuilt (CPE-1256, epic CPE-718). Feature-gated OFF by default (`pdf-thumb`) so the plain build
/// compiles zero PDF code and pulls in no pdfium dependency.
#[cfg(feature = "pdf-thumb")]
pub mod thumb_pdf;

/// Video representative-frame thumbnail extraction by shelling out to a bundled `ffmpeg` executable
/// (CPE-1257, epic CPE-718). Feature-gated OFF by default (`video-thumb`) so the plain build compiles
/// zero video code; adds no Cargo dependency even when on (subprocess, never linked).
#[cfg(feature = "video-thumb")]
pub mod thumb_video;

/// Image preview — TIFF/PSD → PNG data-URL transcode + dimensions/EXIF metadata (CPE-099/101/659).
pub mod image_preview;

/// DICOM medical-image reading: curated tag list + pixel-data → PNG data-URL transcode via `dicom-rs`
/// (CPE-1345, gated-format-reader lane). Feature-gated OFF by default (`dicom-thumb`) so the plain build
/// compiles zero DICOM code and pulls in no `dicom-object`/`dicom-pixeldata` dependency.
#[cfg(feature = "dicom-thumb")]
pub mod dicom;

/// Backup copy engine — plan executor (copy/update/mirror-delete) with per-file results (CPE-797).
pub mod backup;

/// Editable media-metadata model — apply set/clear edits to EXIF/IPTC/ID3 fields, refusing read-only ones
/// (CPE-942, epic CPE-725). Pure edit policy; the codec layer reads/writes the real files.
pub mod media_meta_edit;

/// Metadata-studio dispatch — read *all* of a file's metadata across the per-format read codecs, and write
/// edits back for the writable formats (mp3/flac), routed by extension (CPE-1041, epic CPE-725). The one
/// entry point the app's metadata commands call. Pure.
pub mod media_meta;

/// Media-metadata read codecs — parse a media file's bytes into [`media_meta_edit::MetaField`]s. First
/// codec: ID3v2 audio tags (CPE-970, epic CPE-725; also feeds CPE-707 columns). Pure, std-only.
pub mod media_meta_read;

/// Media-metadata **write** codec — the counterpart to [`media_meta_read::read_id3v2`]. Builds a valid
/// ID3v2.4 tag from the `"id3"`-group [`media_meta_edit::MetaField`]s and prepends it to a file's audio
/// payload, replacing any pre-existing ID3v2 tag rather than stacking on top of it (CPE-1035, epic
/// CPE-725). Pure, std-only.
pub mod media_meta_write;

/// Audio-metadata column extractor — map read ID3 [`media_meta_edit::MetaField`]s to typed
/// [`metadata_column::CellValue`]s so Track/Year columns sort numerically (CPE-971, epic CPE-707). Pure.
pub mod media_column;

/// Image-metadata column extractor — read an image header (no full decode) into a
/// [`metadata_column::CellValue::Dimensions`] cell for a Dimensions column (CPE-974, epic CPE-707).
pub mod image_column;

/// Shared ISO-BMFF (`MP4`/`MOV`) box-walking primitives — the one `BoxHeader`/`read_box_header`/
/// `find_child_box` implementation used by [`video_column`], [`video_meta_read`], and [`video_meta_write`]
/// (CPE-1309, epic CPE-725). Bounds-checked, never panics.
pub mod iso_bmff;

/// Video-metadata column extractor — walk the ISO-BMFF (`MP4`/`MOV`) box tree to `moov/mvhd` for a
/// duration-in-seconds [`metadata_column::CellValue::Float`] cell (CPE-1028, epic CPE-707). Pure.
pub mod video_column;

/// MP4/MOV video-metadata read codec — walk the ISO-BMFF `moov/udta/meta/ilst` box tree for iTunes-style
/// descriptive tags (Title/Artist/Album/…) into [`media_meta_edit::MetaField`]s in group `"video"`
/// (CPE-1037, epic CPE-725). Pure, std-only, never panics on malformed/truncated input.
pub mod video_meta_read;

/// MP4/MOV video-metadata **write** codec — the counterpart to [`video_meta_read::read_mp4`]. Writes the
/// nine iTunes-style tags into `moov/udta/meta/ilst` via a never-move-`mdat` append-and-shadow rewrite, so
/// the absolute `stco`/`co64` sample offsets stay valid (CPE-1309, epic CPE-725). Pure, std-only.
pub mod video_meta_write;

/// Document-info column extractor — map a PDF's read `/Info` [`media_meta_edit::MetaField`]s to typed
/// [`metadata_column::CellValue`]s so Title/Author/… columns surface for PDFs (CPE-1039, epic CPE-707).
/// Pure.
pub mod doc_info_column;

/// Video-tag column extractor — map an MP4/MOV's read iTunes-style [`media_meta_edit::MetaField`]s to
/// typed [`metadata_column::CellValue`]s so Title/Artist/Year/… columns surface for videos, with Year
/// sorting numerically (CPE-1040, epic CPE-707). Pure.
pub mod video_tag_column;

/// Metadata-column dispatcher — route a file's bytes + extension + a [`column_extract::MetaColumn`] to the
/// right per-family extractor (audio codecs / image header), the one entry point the column system calls
/// (CPE-975, epic CPE-707). Pure.
pub mod column_extract;

/// Document-metadata column extractor — count a PDF's pages by scanning raw bytes for `/Type /Page`
/// object markers (excluding the `/Pages` tree node) into a [`metadata_column::CellValue::Int`] cell for
/// a Pages column (CPE-1029, epic CPE-707). Pure byte scan; no PDF library, no new dependency.
pub mod doc_column;

/// Metadata-column cell extraction over real files — the file-I/O layer on top of
/// [`column_extract::extract_column`]: a capped-header read per path, the available-columns list (id +
/// label + applicable extensions) for the picker, and the shared streamed/collect-to-vec walker the
/// `metadata_column_cells`/`metadata_columns_available` commands dispatch into (CPE-1145, epic CPE-707).
pub mod column_cells;

/// Pure vector index — cosine-similarity top-k over stored embeddings + a persistent store, the
/// backend/model-agnostic core of AI semantic search (CPE-981, epic CPE-976). Pure std; the embedder is
/// separate + feature-gated (CPE-982).
pub mod vector_index;

/// Embedding seam — the `Embedder` trait that turns text into vectors for [`vector_index`], plus a
/// dependency-free deterministic `FakeEmbedder` so the pipeline is testable before a real model is chosen
/// (CPE-982, epic CPE-976). A real backend is deferred + feature-gated.
pub mod embedder;

/// Real, configurable embedder over an OpenAI-compatible `/embeddings` endpoint (LM Studio/Ollama local,
/// or OpenAI/others with a key) — a genuine [`embedder::Embedder`] behind the same seam as `FakeEmbedder`
/// (CPE-1273, epic CPE-976). The `ureq` transport is feature-gated (`http-embedder`) so the lean default
/// build pulls in zero HTTP/TLS code; the request-build/response-parse logic is a transport-injectable
/// seam, unit-tested with no network. Selected from persisted config in [`content_index`].
pub mod http_embedder;

/// Semantic index pipeline — chunk a document, embed each chunk via an [`embedder::Embedder`], index it in
/// [`vector_index`], and search per document (best chunk wins) (CPE-983, epic CPE-976). Pure.
pub mod semantic_index;

/// Search result fusion — blend semantic + lexical hits into one ranked list via Reciprocal Rank Fusion,
/// carrying per-source provenance (CPE-984, epic CPE-976). Pure.
pub mod search_fusion;

/// OCR seam + content-addressed text cache — the `OcrEngine` trait, a dependency-free `FakeOcr`, and an
/// `OcrCache` that recognises a given image once (SHA-256 content key). Zero engine weight; a real engine is
/// deferred + feature-gated (CPE-991, epic CPE-980). Pure std (+ existing sha2).
pub mod ocr;

/// OCR → semantic ingest glue — recognise an image's text via an [`ocr::OcrEngine`] (with optional
/// [`ocr::OcrCache`]) and index it into a [`semantic_index::SemanticIndex`], so an image-only file becomes
/// semantically searchable (CPE-996, epic CPE-980 ↔ CPE-976). Pure glue, no I/O.
pub mod semantic_ingest;

/// Content-index wiring — walk a folder's text-like files into a [`semantic_index::SemanticIndex`] (local
/// `FakeEmbedder`, no key/network), persist + reload it keyed by the indexed root, and search it into
/// ranked `{path, score, snippet}` hits (CPE-1262, epic CPE-976). The `content_index_build`/`content_search`
/// Tauri commands are thin dispatchers into this module. Framed honestly as file-CONTENT search
/// (embedder-pluggable), not oversold "semantic".
pub mod content_index;

/// Document text extraction for content search (CPE-1274, epic CPE-976): the extension-dispatched
/// `content_text_of` that turns a file's bytes into indexable text — plain-text unchanged, `.pdf` via
/// pdfium (`pdf-thumb`), `.docx`/`.xlsx`/`.pptx` via the existing `zip` reader + `doc_text`'s
/// extractors. The one call [`content_index`]'s walk (and its snippet re-read) both dispatch through.
pub mod content_text;

/// Batch metadata apply — apply one shared set of edits to a whole selection of files, with a per-file
/// result + run summary; builds on media_meta_edit (CPE-949, epic CPE-725).
pub mod media_meta_batch;

/// Server-side contract dispatch — route `Request` envelopes to domain functions (CPE-824, the Server
/// half of the remote RPC loop CPE-820).
pub mod dispatch;

/// Secure-delete pass planner — overwrite-pass schedules + honest SSD/copy-on-write erasure caveats
/// (CPE-941, epic CPE-738). Pure planning; the shred engine executes the passes.
pub mod secure_delete;

/// Secure-delete shred **engine** (CPE-1012, epic CPE-738): executes a `secure_delete` plan against
/// a real file — overwrite pass by pass, flush + sync, then unlink — and reports what ran.
pub mod secure_shred;

/// Encrypted-vault crypto **core** (CPE-1247, epic CPE-738): the pure, Tauri-free heart of the
/// encrypted-vaults feature. Packs a folder tree into one deterministic length-prefixed byte stream,
/// encrypts it with the `age` crate in passphrase mode (scrypt KDF + ChaCha20-Poly1305 AEAD) into a
/// `.cpevault` blob, and decrypts + authenticates it back — mapping a wrong passphrase, a tampered
/// blob, a bad magic, and an unsupported version to distinct [`vault_crypto::VaultError`] variants.
/// No hand-rolled crypto, no global state; a crafted blob can never panic.
pub mod vault_crypto;

/// Encrypted-vault **lifecycle manager** (CPE-1248, epic CPE-738): the state model + OS-keychain seam
/// over [`vault_crypto`]. Detects a vault by magic, creates one (with a verify-before-shred safety
/// invariant for the destructive "seal then destroy the original" path), unlocks/locks a vault into a
/// session directory (securely wiped on lock), and persists a passphrase through the [`vault_manager::
/// SecretAccess`] keychain seam — the ONLY place a passphrase persists. Tauri-free; the real keyring
/// backend + the managed [`vault_manager::VaultRegistry`] wiring live in the app adapter.
pub mod vault_manager;

/// AI-copilot operation-plan model — the pure, filesystem-free structured plan (a closed, whitelisted set of
/// move/rename/delete/mkdir/copy ops) an NL instruction compiles to, plus its scope+cap validator and a
/// dry-run summary (CPE-990, epic CPE-977). No LLM, no I/O; the safe, inspectable middle of the copilot.
pub mod op_plan;

/// AI-copilot **LLM seam** — natural-language → a candidate [`op_plan::FileOpPlan`] over an OpenAI-compatible
/// `/chat/completions` endpoint (LM Studio local, or OpenAI/others with a key) (CPE-1275, epic CPE-977). The
/// `ureq` transport is feature-gated (`copilot`) so the lean default build compiles zero HTTP/TLS code; the
/// prompt-build/response-parse logic is a transport-injectable seam, unit-tested with no network. Output is
/// only ever a closed whitelisted plan by construction — no free-form/shell escape. Mirrors `http_embedder`.
pub mod copilot_planner;

/// AI-copilot **command-layer glue** — the safe plan → confirm → execute → undo pipeline (CPE-1275, epic
/// CPE-977): [`copilot::plan_with`] validates a planner's output against the scope+cap envelope (no disk
/// change); [`copilot::execute_with`] RE-VALIDATES, checkpoints first (one-click undo), applies the
/// whitelisted ops, and routes deletes to the OS trash via the [`copilot::TrashBin`] seam (recoverable).
/// Tauri-free; the real keyring + trash + `TauriCtx` wiring live in the app adapter.
pub mod copilot;

/// Perceptual-hash + similarity-clustering core — dHash over decoded image bytes + Hamming-distance
/// single-link clustering, the pure engine behind near-duplicate / similar-image detection (CPE-998, epic
/// CPE-997). The complement of [`duplicates`] (byte-identical); this finds visually-similar images. Pure
/// over bytes; no filesystem I/O, no new dependencies.
pub mod perceptual;

/// Near-duplicate **image** scan pipeline — walks a folder, dHashes each decodable image via
/// [`perceptual`], and single-link-clusters visually-similar images into groups (CPE-1200, epic
/// CPE-997). The perceptual complement of [`duplicates`] (byte-identical); streams over the shared
/// walker like [`duplicates::stream_duplicates`].
pub mod image_similarity;

/// SimHash text fingerprinting + near-duplicate document clustering (CPE-999, epic CPE-997). Extends
/// [`perceptual`]'s clustering core to text: Charikar SimHash over tokenised text (stable 64-bit FNV-1a
/// per token) instead of an image dHash, reusing [`perceptual::cluster`] for the grouping. Pure; no new
/// dependencies.
pub mod simhash;

/// Near-duplicate **document** scan pipeline (CPE-1204, epic CPE-997 stretch). Walks a folder, reads
/// each candidate plain-text file, and single-link-clusters near-identical documents via [`simhash`].
/// The textual complement of [`image_similarity`]; streams over a shared walker like
/// [`duplicates::stream_duplicates`].
pub mod document_similarity;

/// Boolean query grouping — `OR`/`NOT`/parentheses over opaque leaf tokens (CPE-1062, epic CPE-703).
/// Complements the AND-only [`index_query`] grammar with real boolean structure; std-only, no deps,
/// tested with a stub leaf matcher so it needs none of the sibling filter modules.
pub mod query_group;

/// Archive-vs-folder diff (CPE-1057, epic CPE-705): classify an archive's entries against a
/// destination folder's listing — "what's in this zip I don't already have" before extracting.
/// Pure, no new deps.
pub mod archive_diff;

/// Minimap density rows (CPE-1052, epic CPE-724): downsample a source file's lines into a fixed number
/// of overview rows (fill density + min indent) for a future scaled-preview minimap. Pure, dependency-
/// free, integer-only arithmetic for determinism.
pub mod minimap;

/// Magic-byte file-type detection + extension-mismatch flagging (CPE-1001, epic CPE-1000). Sniffs a
/// curated set of common binary formats from their leading magic bytes and flags a claimed extension that
/// disagrees with them (e.g. a `.jpg` that is actually a PE executable), while treating ZIP-container
/// formats (`.docx`/`.xlsx`/`.epub`/…) as non-mismatches. Pure over a byte slice; no I/O, no new
/// dependencies.
pub mod file_type;

/// Disguised-file (extension-mismatch) **tree sweep** (CPE-1285, epic CPE-1000). Walks a real directory
/// tree (skip-unreadable), reads a capped ~64-byte header per regular file, and calls
/// [`file_type::mismatch`] to flag every file whose sniffed content disagrees with its claimed extension
/// — the tree-wide sweep companion to the per-row `TypeMismatch` metadata column. Container-safe (relies
/// on `file_type::mismatch`'s own ZIP-container handling). Pure adapter; command/UI wiring is CPE-1287.
pub mod type_mismatch_scan;

/// Text-encoding + line-ending detection (CPE-1003, epic CPE-1002 "File inspection & safety
/// utilities"). Guesses a text file's encoding from a leading byte-order mark or UTF-8 validity, and
/// separately reports which line-ending convention(s) a decoded string uses. Pure over a byte slice
/// / `&str`; no I/O, no new dependencies.
pub mod text_encoding;

/// Archive expansion-ratio / zip-bomb safety score (CPE-1004, epic CPE-1002). Pure per-entry
/// compressed/uncompressed size scoring against configurable thresholds — no extraction, no I/O, no
/// new dependencies.
pub mod archive_safety;

/// Archive expansion-ratio / zip-bomb safety **scan** adapter (CPE-1281, epic CPE-1002): opens a real
/// ZIP via the `zip` crate and scores it with [`archive_safety::expansion_ratio`] — the adapter
/// [`archive_safety`]'s own docs describe as a later caller's job.
pub mod archive_safety_scan;

/// Extract planner (CPE-1055, epic CPE-705). Given an archive's entry list and the destination's
/// existing entry names, computes resolved destination paths, zip-slip rejections, name collisions,
/// directories to create, and totals — without opening the archive or touching the filesystem. Reuses
/// [`archive::entry_name_is_safe`] rather than duplicating the zip-slip check; no new dependencies.
pub mod extract_plan;

/// Cascade-aware empty-folder finder (CPE-1005, epic CPE-1002). Given a caller-supplied directory
/// tree, finds directories that are empty or would become empty once nested-empty subdirectories
/// are removed, reporting only the topmost directory of each cascade. Pure recursive tree
/// algorithm; no I/O, no new dependencies.
pub mod empty_dirs;

/// Filesystem-walk adapter for [`empty_dirs::cascade_empty`] (CPE-1282, epic CPE-1002): recursively
/// walks a real directory tree (skip-unreadable, capped) into an `empty_dirs::DirNode` tree and reports
/// the topmost cascade-empty directory paths. Pure adapter; command/UI wiring is CPE-1287.
pub mod empty_dirs_scan;

/// Near-identical-folder detection via Jaccard similarity over file-content-hash sets (CPE-1007,
/// epic CPE-1002). A level up from [`duplicates`] (byte-identical *files*): finds folders that are
/// ~the same even though they aren't identical as a whole (e.g. `Photos/` vs. `Photos (backup)/`
/// with high file overlap). Pure over caller-supplied (path, hash-set) pairs; no I/O, no new
/// dependencies.
pub mod folder_similarity;

/// Near-identical-folder **scan** pipeline (CPE-1204, epic CPE-997 stretch). Walks a folder tree,
/// hashes each candidate subfolder's direct file children, and clusters near-identical folders via
/// [`folder_similarity`] — this is the adapter that [`folder_similarity`]'s own docs describe as the
/// caller's job.
pub mod folder_similarity_scan;

/// Pure broken/dangling + cyclic symlink classifier (CPE-1008, epic CPE-1002). Given a
/// caller-supplied list of symlink records (path, resolved target, target-existence), classifies
/// each as healthy, `Missing` (target doesn't exist), or `Cyclic` (the target chain loops back on
/// itself, incl. a self-loop). No real filesystem symlinks are created or read here — that's the
/// adapter's job; this module only classifies caller-supplied records. Pure std, no I/O, no new
/// dependencies.
pub mod dangling_links;

/// Dangling/cyclic symlink **scan** pipeline (CPE-1284, epic CPE-1002). Walks a folder tree,
/// resolves each symlink it finds, and hands the built records to [`dangling_links::scan_dangling`]
/// — the adapter that [`dangling_links`]'s own docs describe as the caller's job. No command/UI
/// wiring here (CPE-1287).
pub mod dangling_links_scan;

/// File-inspection composition (CPE-1009, epic CPE-1002): compose the pure detectors (encoding + line
/// endings + true type + extension mismatch) into one display-ready `FileInspection` for the Properties
/// panel. Pure over a file's leading bytes + name; the Tauri command reads the bytes.
pub mod inspect;

/// Per-folder metadata-column config store (CPE-1032, epic CPE-707): remembers the ordered list of
/// visible-column ids chosen for a folder, persisted as `column_config.json` in the app config dir.
/// Mirrors the `folder_template.rs` store; string column-ids only, no dependency on `column_extract`.
pub mod column_config;

/// 3D-model geometry/stats reader (CPE-1333, epic CPE-118 "Preview/edit support for 3D models"). Reads
/// binary/ASCII STL and Wavefront OBJ from an in-memory byte slice into a triangle/vertex count + bounding
/// box — the metadata-pane fallback the (blocked) interactive-viewer epic's acceptance criteria call for,
/// deliverable without the heavy WebGL dependency the full viewer needs. Pure, std-only, no new
/// dependencies.
pub mod model_3d;
