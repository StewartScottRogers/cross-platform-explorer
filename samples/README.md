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
| `documents/doc.pdf` | PDF             | `/Info` Title/Author/Subject/Keywords/Creator/Producer/Dates (read-only) |
| `video/clip.mp4`    | MP4 / iTunes    | `ilst` Title/Artist/Album/Year (read-only) |
| `text/notes.txt`, `readme.md`, `data.json`, `table.csv`, `hello.py` | Text | Plain-text/markdown/JSON/CSV/code preview + line/word counts |

The files are **minimal-but-valid**: the app's read codecs parse their metadata correctly. They are
baselines for *metadata* and general-explorer checks, not full studio-quality media (the audio/video files
carry only a token frame).

The automated guard lives in `crates/server/tests/sample_fixtures.rs`, which reads each file through the
shipped codecs and asserts the values above.
