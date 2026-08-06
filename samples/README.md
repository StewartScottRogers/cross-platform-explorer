# `samples/` — pristine sample-data baseline (CPE-1042)

A small set of **synthetic, known-metadata** files used as a stable baseline for manual/GUI checks (e.g.
verifying the **Metadata Studio**) and as automated fixtures. No copyrighted media — every file is generated
by [`scripts/gen_samples.py`](../scripts/gen_samples.py) and reproduces byte-for-byte.

## ⚠️ Pristine — never edit these in place

Treat everything under `samples/` as **read-only**. Tests and manual checks that only *read* metadata can
point straight at these files. Anything that **modifies** a file (e.g. editing tags in the Studio and
saving) must work on a **copy**, so the baseline stays trustworthy.

Copy the tree into a git-ignored sandbox first:

```pwsh
# Windows (PowerShell)
scripts\new-sample-sandbox.ps1            # → .sandbox\<timestamp>\  (prints the path)
```
```bash
# macOS / Linux
scripts/new-sample-sandbox.sh             # → .sandbox/<timestamp>/  (prints the path)
```

`.sandbox/` is git-ignored, so edited copies never pollute the repo. Delete it anytime.

## Regenerate

```bash
python scripts/gen_samples.py
```

Deterministic: it rewrites the same bytes. If you change the generator, update the baseline below and the
`crates/server/tests/sample_fixtures.rs` assertions to match.

## The baseline

All media files carry this fixed metadata (the single source of truth is the top of `gen_samples.py`):

| Field  | Value             |
|--------|-------------------|
| Title  | `Baseline Sample` |
| Artist | `CPE Test Suite`  |
| Album  | `Known Fixtures`  |
| Year   | `2026`            |
| Track  | `3` (`3/10` in ID3)|
| Genre  | `Ambient`         |

### Files

| File | Kind | What it exercises |
|------|------|-------------------|
| `audio/track.mp3`   | MP3 / ID3v2.3   | Title/Artist/Album/Year/Track/Genre + a Comment; **editable** in the Studio |
| `audio/track.flac`  | FLAC / Vorbis   | Same tags via Vorbis comments; **editable** in the Studio |
| `audio/track.ogg`   | OGG / Vorbis    | Same tags; read-only in the Studio (no OGG writer yet) |
| `images/photo.jpg`  | JPEG + EXIF     | EXIF Make/Model/ImageDescription/Artist/Copyright (descriptive tags editable, intrinsics read-only) |
| `images/pixel.png`  | PNG (2×2)       | A real, openable raster (thumbnail/preview path) |
| `documents/doc.pdf` | PDF             | `/Info` Title/Author/Subject/Keywords/Creator/Producer/Dates (read-only); a genuinely valid, loadable 2-page PDF (byte-accurate `xref`) — see "PDF fixtures" below |
| `video/clip.mp4`    | MP4 / iTunes    | `ilst` Title/Artist/Album/Year (read-only) |
| `text/notes.txt`, `readme.md`, `data.json`, `table.csv`, `table.tsv`, `hello.py` | Text | Plain-text/markdown/JSON/CSV/TSV/code preview + line/word counts |

The files are **minimal-but-valid**: the app's read codecs parse their metadata correctly. They are
baselines for *metadata* and general-explorer checks, not full studio-quality media (the audio/video files
carry only a token frame).

The automated guard lives in `crates/server/tests/sample_fixtures.rs`, which reads each file through the
shipped codecs and asserts the values above.

## PDF fixtures (CPE-1357/1358)

`documents/doc.pdf` used to be a **degenerate** fixture (`/Kids [] /Count 0`, no `xref` table at all) —
opening it in the preview pane crashed the app (CPE-1357). It has been replaced with a genuinely valid,
loadable 2-page PDF (real `xref`/`startxref`/`%%EOF`), carrying the SAME full `/Info` metadata baseline
every other sample format documents here (Title/Author/Subject/Keywords/Creator/Producer/Dates — an
intermediate commit briefly swapped in a Pillow-rendered raster PDF with a slimmer `/Info` dict; this
generator-produced fixture, and `sample_fixtures.rs::pdf_info_baseline`, both restore the full baseline).

The **old, broken bytes are preserved unchanged** as `documents/malformed.pdf` — a deliberate regression
fixture for the crash: `gui-smoke/specs/samples.smoke.ts` opens it (last, after every other sample) and
asserts the app survives. CPE-1357 landed a validate-before-embed fix
(`cpe_server::media_meta_read::pdf_validity`): a PDF with no resolvable `startxref` or a declared
zero-page `/Pages` tree — exactly `malformed.pdf`'s shape — is rejected BEFORE ever reaching WebView2's
PDF viewer, falling back to the metadata pane instead. So this fixture now doubles as CPE-1357's
regression pin: opening it must keep degrading gracefully (never crash) for as long as that fix stands.

## Sample-coverage ratchet (CPE-1358)

Every supported preview **kind** (`src/lib/preview/provider.ts`) has at least one valid sample below, so
opening any format the app claims to support has real fixture coverage:

| Preview kind    | Sample(s)                                             |
|------------------|-------------------------------------------------------|
| `image`          | `images/photo.jpg`, `images/pixel.png`                |
| `decoded-image`  | `images/photo.tiff`                                   |
| `raw-image`      | `raw/sunset.cr2`                                       |
| `dicom`          | `medical/ct-scan.dcm`                                  |
| `heic`           | `images/iphone-photo.heic`                             |
| `audio`          | `audio/track.mp3`, `audio/track.flac`, `audio/track.ogg` |
| `video`          | `video/clip.mp4`                                        |
| `pdf`            | `documents/doc.pdf` (valid), `documents/malformed.pdf` (CPE-1357 regression trigger) |
| `json`           | `text/data.json`                                        |
| `csv`            | `text/table.csv`                                        |
| `tsv`            | `text/table.tsv`                                        |
| `archive`        | `archives/sample.zip` (`archives/sample.rar` also exists but is NOT wired into the frontend's `ARCHIVE_EXT` list — it renders via the generic hex-dump provider instead; see the note in `sampleCoverage.test.ts`) |
| `font`           | `fonts/mini.ttf`                                        |
| `data-grid`      | `database/mini.sqlite`                                  |
| `info`           | `other/tiny.wasm`                                       |
| `markdown`       | `text/readme.md`                                        |
| `text`           | `text/notes.txt`, `text/hello.py`                        |
| `hex`            | `archives/sample.rar` (any file no richer provider claims falls back to the hex view) |

The headless guard is `src/lib/sampleCoverage.test.ts` (vitest): it computes the real preview-provider
`kind` for every file under `samples/` (via `pickProvider`, the exact production code path) and fails if
any kind above has zero samples — a new preview format shipped without a sample breaks CI. The end-to-end
guard is `gui-smoke/specs/samples.smoke.ts`: it walks the same `samples/` tree on a real built app and
asserts every file's preview renders (or degrades gracefully) without crashing.
