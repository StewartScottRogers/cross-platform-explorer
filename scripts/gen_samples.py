#!/usr/bin/env python3
"""Generate the ``samples/`` fixture tree (CPE-1042, made *substantial* in CPE-1361).

Every file here is **synthetic** (no copyrighted media) and, where practical, **deterministic**. Unlike the
original CPE-1042 baseline — which was a set of tiny metadata-only stubs (an mp3 that was 395 B of ID3 with a
single silent frame, an 8-byte "wasm", a 2x2-px image, a one-file zip …) — these fixtures are *real and
substantial* so the preview panes and the gui-smoke walk (CPE-1358) get a robust workout:

  * **Audio** (mp3 / flac / ogg): a real few-second stereo 44.1 kHz arpeggio synthesised with the stdlib
    ``wave`` module, then encoded by **ffmpeg** with the metadata baseline embedded (Title/Artist/Album/…).
  * **Video** (mp4): a real H.264/AAC clip (**ffmpeg** ``testsrc`` + a sine tone), a few seconds, that plays
    in the webview, carrying the same iTunes-style tag baseline.
  * **Images** (jpg / png / tiff): real ~800x600 synthetic scenes rendered with **Pillow (PIL)**; the JPEG
    keeps the baseline EXIF tags.
  * **Archives** (zip / rar): a real multi-file, multi-**folder** tree (docs/, images/, data/, src/, a nested
    docs/sub/) — the zip via stdlib ``zipfile``, the rar as a hand-built RAR4 with STORED files (no rar
    encoder exists) mirroring the same tree.
  * **Font** (ttf): a hand-built TrueType font with a **full printable-ASCII glyph set** (0x20-0x7E), each a
    real outline, so ``thumb_font::render_glyph_sheet`` renders an actual specimen.
  * **wasm**: a real module exporting ``add`` and ``fib`` (hand-assembled), so ``binary_preview::wasm_info``
    disassembles it to meaningful WAT.
  * **sqlite**: a real database with several tables + dozens of realistic rows for the data-grid preview.
  * **pdf**: a valid, loadable multi-page PDF (byte-accurate ``xref``) with per-page colour + text, keeping
    the ``/Info`` metadata baseline.
  * **text** (json/csv/tsv/md/py/txt): realistic multi-line content.

The metadata baseline (Title="Baseline Sample", Artist="CPE Test Suite", …) is preserved across the rewrite
so ``crates/server/tests/sample_fixtures.rs`` stays green — that test reads each fixture through the shipped
codecs and asserts these values.

Authoring tools: **ffmpeg** (audio + video), **Pillow/PIL** (images + archive logo). Both are invoked only at
authoring time; the committed ``samples/`` bytes are what CI reads, so CI needs neither. If ffmpeg or PIL is
absent the script still runs, falling back to a minimal-but-valid stub for the affected format (so the tree is
always producible and always parses), and prints a note. **Determinism caveat:** the ffmpeg-encoded media
(mp3/flac/ogg/mp4) and the PIL images are real encodes and are *not* guaranteed byte-identical across ffmpeg/
PIL versions; every hand-built fixture (zip/rar/ttf/wasm/pdf/text, and the sqlite schema/rows) is deterministic.

Run from the repo root:  ``python scripts/gen_samples.py``  (use a Python with Pillow installed for real
images; ffmpeg must be on PATH for real media).
"""
from __future__ import annotations
import io
import json
import math
import os
import shutil
import struct
import subprocess
import tempfile
import wave
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SAMPLES = os.path.join(ROOT, "samples")

# ── The single source of truth for the baseline metadata ────────────────────────────────────────────
TITLE = "Baseline Sample"
ARTIST = "CPE Test Suite"
ALBUM = "Known Fixtures"
YEAR = "2026"
TRACK = "3"
GENRE = "Ambient"
COMMENT = "Pristine CPE sample - do not edit in place"

FFMPEG = shutil.which("ffmpeg")

try:
    from PIL import Image, ImageDraw  # type: ignore

    HAVE_PIL = True
except Exception:  # pragma: no cover - depends on the interpreter used
    HAVE_PIL = False

_NOTES: list[str] = []


def note(msg: str) -> None:
    _NOTES.append(msg)


def write(rel: str, data: bytes) -> None:
    path = os.path.join(SAMPLES, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)
    print(f"  {rel:28} {len(data):>8} bytes")


# ════════════════════════════════════════════════════════════════════════════════════════════════════
# Audio — synth a real WAV melody (stdlib `wave`) then encode with ffmpeg, embedding the tag baseline.
# ════════════════════════════════════════════════════════════════════════════════════════════════════
def synth_wav(seconds: float = 4.0, rate: int = 44100) -> bytes:
    """A real, audible stereo 44.1 kHz PCM arpeggio built with only the stdlib ``wave`` module. A rising
    C-major-ish scale of pure sine tones with a soft attack/decay envelope (no clicks) and a slight L/R
    detune for stereo width — deterministic (pure math, no RNG)."""
    notes = [261.63, 293.66, 329.63, 349.23, 392.00, 440.00, 493.88, 523.25]  # C4..C5
    total = int(seconds * rate)
    seg = total // len(notes)
    fade = max(1, int(0.02 * rate))  # 20 ms attack/decay
    frames = bytearray()
    for n, freq in enumerate(notes):
        for i in range(seg):
            t = i / rate
            env = 1.0
            if i < fade:
                env = i / fade
            elif i > seg - fade:
                env = max(0.0, (seg - i) / fade)
            # fundamental + a soft second harmonic for a little timbre
            base = math.sin(2 * math.pi * freq * t) + 0.25 * math.sin(2 * math.pi * 2 * freq * t)
            amp = 0.28 * env
            left = int(max(-1.0, min(1.0, amp * base)) * 32767)
            rbase = math.sin(2 * math.pi * (freq * 1.003) * t) + 0.25 * math.sin(2 * math.pi * 2 * freq * t)
            right = int(max(-1.0, min(1.0, amp * rbase)) * 32767)
            frames += struct.pack("<hh", left, right)
    buf = io.BytesIO()
    w = wave.open(buf, "wb")
    w.setnchannels(2)
    w.setsampwidth(2)
    w.setframerate(rate)
    w.writeframes(bytes(frames))
    w.close()
    return buf.getvalue()


def _ffmpeg_encode(wav_bytes: bytes, out_name: str, args: list[str]) -> bytes | None:
    """Write ``wav_bytes`` to a temp file, run ffmpeg with ``args`` producing ``out_name``, return the
    encoded bytes (or None if ffmpeg is missing/fails so the caller can fall back to a stub)."""
    if not FFMPEG:
        return None
    with tempfile.TemporaryDirectory(prefix="cpe-samples-") as d:
        wav_path = os.path.join(d, "in.wav")
        out_path = os.path.join(d, out_name)
        with open(wav_path, "wb") as f:
            f.write(wav_bytes)
        cmd = [FFMPEG, "-y", "-hide_banner", "-loglevel", "error", "-i", wav_path] + args + [out_path]
        try:
            subprocess.run(cmd, check=True, capture_output=True)
        except Exception as e:  # pragma: no cover
            note(f"ffmpeg failed for {out_name} ({e}); wrote a stub instead")
            return None
        with open(out_path, "rb") as f:
            return f.read()


AUDIO_TAGS_VORBIS = [
    "-metadata", f"title={TITLE}",
    "-metadata", f"artist={ARTIST}",
    "-metadata", f"album={ALBUM}",
    "-metadata", f"date={YEAR}",
    "-metadata", f"track={TRACK}",
    "-metadata", f"genre={GENRE}",
    "-metadata", f"comment={COMMENT}",
]


def _strip_id3v2(data: bytes) -> bytes:
    """Drop a leading ID3v2 tag (``ID3`` + syncsafe size) if present, returning the raw MPEG audio frames."""
    if len(data) >= 10 and data[:3] == b"ID3":
        size = ((data[6] & 0x7F) << 21) | ((data[7] & 0x7F) << 14) | ((data[8] & 0x7F) << 7) | (data[9] & 0x7F)
        return data[10 + size:]
    return data


def make_mp3(wav: bytes) -> bytes:
    """Real MPEG audio from ffmpeg with a hand-built ID3v2.3 tag prepended. We build the tag ourselves (rather
    than let ffmpeg write it) so the baseline is exact and deterministic — in particular ffmpeg maps
    ``-metadata comment`` to a ``TXXX`` user-text frame, but the studio/read codec baseline expects a real
    ``COMM`` frame, which ``_id3v23_tag`` emits. The audio payload is genuine, encoded by libmp3lame."""
    audio = _ffmpeg_encode(wav, "track.mp3", ["-c:a", "libmp3lame", "-b:a", "128k", "-write_id3v2", "0", "-write_id3v1", "0"])
    if audio is None:
        return _make_mp3_stub()
    return _id3v23_tag() + _strip_id3v2(audio)


def make_flac(wav: bytes) -> bytes:
    data = _ffmpeg_encode(wav, "track.flac", ["-c:a", "flac", "-compression_level", "5"] + AUDIO_TAGS_VORBIS)
    return data if data is not None else _make_flac_stub()


def make_ogg(wav: bytes) -> bytes:
    data = _ffmpeg_encode(wav, "track.ogg", ["-c:a", "libvorbis", "-q:a", "4"] + AUDIO_TAGS_VORBIS)
    return data if data is not None else _make_ogg_stub()


def make_mp4() -> bytes:
    """A real short H.264/AAC clip: an ffmpeg ``testsrc`` colour-bars animation + a 440 Hz sine tone, a few
    seconds, carrying the iTunes-style (``moov/udta/meta/ilst``) tag baseline read by ``video_meta_read``."""
    if FFMPEG:
        with tempfile.TemporaryDirectory(prefix="cpe-samples-") as d:
            out_path = os.path.join(d, "clip.mp4")
            cmd = [
                FFMPEG, "-y", "-hide_banner", "-loglevel", "error",
                "-f", "lavfi", "-i", "testsrc=duration=4:size=640x480:rate=30",
                "-f", "lavfi", "-i", "sine=frequency=440:duration=4",
                "-c:v", "libx264", "-preset", "veryfast", "-crf", "28", "-pix_fmt", "yuv420p",
                "-c:a", "aac", "-b:a", "96k", "-shortest", "-movflags", "+faststart",
                "-metadata", f"title={TITLE}",
                "-metadata", f"artist={ARTIST}",
                "-metadata", f"album={ALBUM}",
                "-metadata", f"date={YEAR}",
                out_path,
            ]
            try:
                subprocess.run(cmd, check=True, capture_output=True)
                with open(out_path, "rb") as f:
                    return f.read()
            except Exception as e:  # pragma: no cover
                note(f"ffmpeg failed for clip.mp4 ({e}); wrote a stub instead")
    return _make_mp4_stub()


# ════════════════════════════════════════════════════════════════════════════════════════════════════
# Images — real ~800x600 synthetic scenes via Pillow; JPEG keeps the baseline EXIF tags.
# ════════════════════════════════════════════════════════════════════════════════════════════════════
def _scene(w: int = 800, h: int = 600):
    """A recognisable synthetic scene: a vertical sky-to-ground gradient, a sun, a couple of hills and a
    little house — enough real structure for the preview to show something meaningful. Returns a PIL image."""
    img = Image.new("RGB", (w, h), (135, 206, 235))
    px = img.load()
    for y in range(h):
        # sky gradient (top) blending toward a paler horizon
        f = y / h
        r = int(135 + 60 * f)
        g = int(206 + 30 * f)
        b = int(235 - 40 * f)
        for x in range(w):
            px[x, y] = (r, g, b)
    d = ImageDraw.Draw(img)
    # sun
    d.ellipse([w - 180, 60, w - 80, 160], fill=(255, 221, 92))
    # rolling hills
    d.ellipse([-200, int(h * 0.65), int(w * 0.6), h + 300], fill=(94, 153, 84))
    d.ellipse([int(w * 0.4), int(h * 0.72), w + 200, h + 300], fill=(76, 133, 70))
    # ground band
    d.rectangle([0, int(h * 0.85), w, h], fill=(70, 120, 64))
    # a small house
    hx, hy = int(w * 0.18), int(h * 0.6)
    d.rectangle([hx, hy, hx + 130, hy + 110], fill=(200, 170, 140))
    d.polygon([(hx - 15, hy), (hx + 145, hy), (hx + 65, hy - 70)], fill=(150, 70, 55))
    d.rectangle([hx + 50, hy + 55, hx + 85, hy + 110], fill=(90, 60, 40))  # door
    return img


def make_jpeg() -> bytes:
    if not HAVE_PIL:
        return _make_jpeg_stub()
    img = _scene()
    exif = img.getexif()
    exif[0x010E] = "Baseline EXIF sample"  # ImageDescription
    exif[0x010F] = "CPE"                   # Make
    exif[0x0110] = "Fixture Cam"           # Model
    exif[0x0131] = "gen_samples.py"        # Software
    exif[0x0132] = "2026:07:25 11:00:00"   # DateTime
    exif[0x013B] = ARTIST                  # Artist
    exif[0x8298] = "CC0"                   # Copyright
    buf = io.BytesIO()
    img.save(buf, format="JPEG", quality=88, exif=exif)
    return buf.getvalue()


def make_png() -> bytes:
    if not HAVE_PIL:
        return _make_png_stub()
    buf = io.BytesIO()
    _scene().save(buf, format="PNG", optimize=True)
    return buf.getvalue()


def make_tiff() -> bytes:
    if not HAVE_PIL:
        return _make_tiff_stub()
    buf = io.BytesIO()
    # LZW-compressed TIFF — the backend (`image_preview::read_image_data_url`, via the `image` crate's `tiff`
    # decoder, which supports LZW) transcodes it to a PNG data URL for the decoded-image preview. LZW keeps
    # the ~800x600 scene to tens of KB rather than the ~1.4 MB an uncompressed RGB strip would take.
    _scene().save(buf, format="TIFF", compression="tiff_lzw")
    return buf.getvalue()


def _logo_png(size: int = 96) -> bytes:
    """A small square logo PNG for the archive fixtures (a rounded blue tile with a white 'C')."""
    if not HAVE_PIL:
        return _make_png_stub()
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([4, 4, size - 4, size - 4], radius=16, fill=(37, 99, 235, 255))
    d.arc([size * 0.28, size * 0.24, size * 0.78, size * 0.76], start=60, end=300, fill=(255, 255, 255, 255), width=10)
    buf = io.BytesIO()
    img.save(buf, format="PNG")
    return buf.getvalue()


# ════════════════════════════════════════════════════════════════════════════════════════════════════
# Archive contents — a real multi-folder tree shared by the zip and rar fixtures (CPE-1360).
# ════════════════════════════════════════════════════════════════════════════════════════════════════
ARCHIVE_README = (
    "# Sample Archive\n\n"
    "This is a **real** multi-folder archive used to exercise the archive-preview pane\n"
    "(inner-file drill-in, CPE-1360). It mirrors a tiny project layout:\n\n"
    "- `docs/`   documentation (this file + `sub/notes.txt`)\n"
    "- `images/` a logo PNG\n"
    "- `data/`   a CSV table\n"
    "- `src/`    a Python entry point\n"
).encode("utf-8")

ARCHIVE_NOTES = (
    "Release notes\n"
    "=============\n\n"
    "v1 - initial sample tree.\n"
    "   - added docs, images, data and src folders.\n"
    "   - every file has real, previewable content.\n"
).encode("utf-8")

ARCHIVE_TABLE_CSV = (
    "id,region,units,revenue\n"
    "1,North,120,15400.00\n"
    "2,South,98,11230.50\n"
    "3,East,143,20890.75\n"
    "4,West,167,24510.20\n"
    "5,Central,88,9910.00\n"
).encode("utf-8")

ARCHIVE_MAIN_PY = (
    "\"\"\"Sample entry point packed inside the archive fixture.\"\"\"\n\n\n"
    "def greet(name: str) -> str:\n"
    "    return f\"Hello, {name}!\"\n\n\n"
    "def main() -> None:\n"
    "    for who in (\"world\", \"CPE\"):\n"
    "        print(greet(who))\n\n\n"
    "if __name__ == \"__main__\":\n"
    "    main()\n"
).encode("utf-8")


def archive_tree() -> list[tuple[str, bytes]]:
    """The (path, content) list for regular files in the archive fixtures, in a stable order. Directory
    entries are derived from these paths by the individual builders."""
    return [
        ("docs/readme.md", ARCHIVE_README),
        ("docs/sub/notes.txt", ARCHIVE_NOTES),
        ("images/logo.png", _logo_png()),
        ("data/table.csv", ARCHIVE_TABLE_CSV),
        ("src/main.py", ARCHIVE_MAIN_PY),
    ]


def make_zip() -> bytes:
    """A real, reproducible multi-file, multi-folder ZIP (stdlib ``zipfile``) with explicit directory
    entries and a fixed timestamp so the bytes are deterministic run-to-run."""
    import zipfile

    files = archive_tree()
    dirs = ["docs/", "docs/sub/", "images/", "data/", "src/"]
    buf = io.BytesIO()
    dt = (2026, 7, 25, 11, 0, 0)
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        for name in dirs:
            zi = zipfile.ZipInfo(name, date_time=dt)
            zi.external_attr = (0o755 << 16) | 0x10  # directory bit
            zf.writestr(zi, b"")
        for path, content in files:
            zi = zipfile.ZipInfo(path, date_time=dt)
            zi.external_attr = 0o644 << 16
            zi.compress_type = zipfile.ZIP_DEFLATED
            zf.writestr(zi, content)
    return buf.getvalue()


# ── RAR4 (hand-built; no rar encoder exists) ─────────────────────────────────────────────────────────
# There is no free RAR compressor, so we hand-assemble a RAR **4** archive with STORED (method 0x30,
# uncompressed) file blocks + explicit directory blocks, mirroring the zip's tree. This is exactly what
# `cpe_server::rar::rar_entries` walks: a 7-byte marker, then self-describing blocks (each states its own
# `head_size`; file blocks additionally carry a `pack_size` data payload). Verified by `rar_entries` in
# `sample_fixtures.rs`. Real content bytes are stored verbatim as each file block's data payload.
RAR4_MARKER = bytes([0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00])
_R4_MAIN = 0x73
_R4_FILE = 0x74
_R4_ENDARC = 0x7B
_R4_LONG_BLOCK = 0x8000
_R4_DIRECTORY = 0x00E0  # all three LHD_WINDOWMASK bits set marks a directory
_DOS_DATETIME = 0x594F5600  # a fixed DOS date/time (irrelevant to listing; keeps output deterministic)


def _rar4_block(head_type: int, flags: int, body: bytes) -> bytes:
    head_size = 7 + len(body)
    return (
        struct.pack("<H", 0)          # crc16 (unchecked by the reader)
        + bytes([head_type])
        + struct.pack("<H", flags)
        + struct.pack("<H", head_size)
        + body
    )


def _rar4_file_body(name: str, content: bytes, is_dir: bool) -> bytes:
    name_bytes = name.encode("utf-8")
    pack = 0 if is_dir else len(content)
    unp = 0 if is_dir else len(content)
    crc = 0 if is_dir else (zlib.crc32(content) & 0xFFFFFFFF)
    attr = 0x10 if is_dir else 0x20
    return (
        struct.pack("<I", pack)             # pack_size
        + struct.pack("<I", unp)            # unp_size
        + bytes([0])                        # host_os (MS-DOS)
        + struct.pack("<I", crc)            # file_crc
        + struct.pack("<I", _DOS_DATETIME)  # ftime
        + bytes([20])                       # unp_ver (2.0)
        + bytes([0x30])                     # method: stored
        + struct.pack("<H", len(name_bytes))
        + struct.pack("<I", attr)
        + name_bytes
    )


def make_rar() -> bytes:
    files = archive_tree()
    dirs = ["docs", "docs/sub", "images", "data", "src"]
    buf = bytearray(RAR4_MARKER)
    # MAIN_HEAD: a 6-byte reserved body (reserved1 u16 + reserved2 u32), like a real RAR4 archive header.
    buf += _rar4_block(_R4_MAIN, 0, struct.pack("<HI", 0, 0))
    for d in dirs:
        buf += _rar4_block(_R4_FILE, _R4_DIRECTORY, _rar4_file_body(d, b"", True))
    for path, content in files:
        buf += _rar4_block(_R4_FILE, _R4_LONG_BLOCK, _rar4_file_body(path, content, False))
        buf += content  # the STORED data payload immediately follows the file header
    buf += _rar4_block(_R4_ENDARC, 0, b"")
    return bytes(buf)


# ════════════════════════════════════════════════════════════════════════════════════════════════════
# SQLite — a real database with several tables + dozens of realistic rows (data-grid preview, CPE-1358).
# ════════════════════════════════════════════════════════════════════════════════════════════════════
def make_sqlite() -> bytes:
    import sqlite3

    tmp_path = os.path.join(SAMPLES, "database", ".mini.sqlite.tmp")
    os.makedirs(os.path.dirname(tmp_path), exist_ok=True)
    if os.path.exists(tmp_path):
        os.remove(tmp_path)
    conn = sqlite3.connect(tmp_path)
    try:
        conn.executescript(
            """
            CREATE TABLE users (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT NOT NULL,
                city TEXT,
                signed_up TEXT
            );
            CREATE TABLE products (
                id INTEGER PRIMARY KEY,
                sku TEXT NOT NULL,
                name TEXT NOT NULL,
                price REAL NOT NULL,
                in_stock INTEGER
            );
            CREATE TABLE orders (
                id INTEGER PRIMARY KEY,
                user_id INTEGER NOT NULL,
                product_id INTEGER NOT NULL,
                qty INTEGER NOT NULL,
                ordered_at TEXT,
                FOREIGN KEY(user_id) REFERENCES users(id),
                FOREIGN KEY(product_id) REFERENCES products(id)
            );
            CREATE VIEW order_summary AS
                SELECT o.id AS order_id, u.name AS customer, p.name AS product, o.qty AS qty
                FROM orders o JOIN users u ON u.id = o.user_id JOIN products p ON p.id = o.product_id;
            """
        )
        first = ["Ann", "Bo", "Cai", "Dana", "Eli", "Faye", "Gus", "Hana", "Ivan", "Jo",
                 "Kim", "Lars", "Mira", "Ned", "Ola", "Pia", "Quinn", "Ravi", "Sam", "Tess"]
        cities = ["Oslo", "Berlin", "Kyoto", "Lima", "Cairo", "Perth", "Quito", "Reno"]
        users = [
            (i + 1, nm, f"{nm.lower()}@example.com", cities[i % len(cities)], f"2026-0{(i % 9) + 1}-15")
            for i, nm in enumerate(first)
        ]
        conn.executemany("INSERT INTO users VALUES (?,?,?,?,?)", users)

        prod_names = [
            "Widget", "Gadget", "Gizmo", "Sprocket", "Cog", "Lever", "Pulley", "Bolt",
            "Bracket", "Clamp", "Hinge", "Spring",
        ]
        products = [
            (i + 1, f"SKU-{1000 + i}", nm, round(4.5 + i * 3.25, 2), (i * 7 + 3) % 50)
            for i, nm in enumerate(prod_names)
        ]
        conn.executemany("INSERT INTO products VALUES (?,?,?,?,?)", products)

        orders = []
        for i in range(40):
            uid = (i % len(users)) + 1
            pid = (i * 5 % len(products)) + 1
            qty = (i % 6) + 1
            orders.append((i + 1, uid, pid, qty, f"2026-07-{(i % 27) + 1:02d}"))
        conn.executemany("INSERT INTO orders VALUES (?,?,?,?,?)", orders)
        conn.commit()
        conn.execute("VACUUM")
    finally:
        conn.close()
    with open(tmp_path, "rb") as f:
        data = f.read()
    os.remove(tmp_path)
    return data


# ════════════════════════════════════════════════════════════════════════════════════════════════════
# WASM — a real module exporting `add` and `fib` (hand-assembled), disassembled by `binary_preview`.
# ════════════════════════════════════════════════════════════════════════════════════════════════════
def _uleb(n: int) -> bytes:
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def _wasm_section(sid: int, body: bytes) -> bytes:
    return bytes([sid]) + _uleb(len(body)) + body


def make_wasm() -> bytes:
    """A genuinely-valid WebAssembly module with two exported functions — ``add(i32,i32)->i32`` and a
    recursive ``fib(i32)->i32`` — so ``wasmprinter`` (via ``binary_preview::wasm_info``) prints meaningful
    WAT with real ``func``/``export``/``call`` structure, not just an empty magic header."""
    I32 = 0x7F
    # Type section: type0 = (i32,i32)->i32 ; type1 = (i32)->i32
    t0 = bytes([0x60, 0x02, I32, I32, 0x01, I32])
    t1 = bytes([0x60, 0x01, I32, 0x01, I32])
    types = _wasm_section(1, _uleb(2) + t0 + t1)
    # Function section: func0 -> type0, func1 -> type1
    funcs = _wasm_section(3, _uleb(2) + _uleb(0) + _uleb(1))
    # Export section: "add" -> func0, "fib" -> func1
    def export(name: bytes, idx: int) -> bytes:
        return _uleb(len(name)) + name + bytes([0x00]) + _uleb(idx)
    exports = _wasm_section(7, _uleb(2) + export(b"add", 0) + export(b"fib", 1))
    # Code section.
    add_body = bytes([0x00, 0x20, 0x00, 0x20, 0x01, 0x6A, 0x0B])  # locals=0; get0 get1 i32.add end
    fib_body = bytes([
        0x00,             # 0 local decls
        0x20, 0x00,       # local.get 0
        0x41, 0x02,       # i32.const 2
        0x48,             # i32.lt_s
        0x04, I32,        # if (result i32)
        0x20, 0x00,       #   local.get 0
        0x05,             # else
        0x20, 0x00,       #   local.get 0
        0x41, 0x01,       #   i32.const 1
        0x6B,             #   i32.sub
        0x10, 0x01,       #   call 1
        0x20, 0x00,       #   local.get 0
        0x41, 0x02,       #   i32.const 2
        0x6B,             #   i32.sub
        0x10, 0x01,       #   call 1
        0x6A,             #   i32.add
        0x0B,             # end (if)
        0x0B,             # end (func)
    ])
    code = _wasm_section(
        10,
        _uleb(2) + _uleb(len(add_body)) + add_body + _uleb(len(fib_body)) + fib_body,
    )
    return b"\x00asm" + struct.pack("<I", 1) + types + funcs + exports + code


# ════════════════════════════════════════════════════════════════════════════════════════════════════
# TTF — a hand-built TrueType font with a FULL printable-ASCII glyph set (0x20-0x7E), each a real outline.
# ════════════════════════════════════════════════════════════════════════════════════════════════════
# Each printable ASCII character gets its own glyph with a real (if minimalist slab-serif-ish) box outline,
# sized by category (caps tall, lowercase short, digits medium, punctuation small) so a specimen sheet shows
# varied shapes. `thumb_font::render_glyph_sheet` renders "Aa" from this — both map to real outlines, so the
# thumbnail has actual ink. Hand-built (stdlib only): glyf/loca (real contours), a format-4 cmap covering the
# whole 0x20-0x7E range, and per-glyph hmtx. Byte-accurate table checksums + the two-pass checkSumAdjustment.
_FIRST_CHAR = 0x20
_LAST_CHAR = 0x7E
_NUM_ASCII = _LAST_CHAR - _FIRST_CHAR + 1  # 95
_NUM_GLYPHS = _NUM_ASCII + 1               # + .notdef (glyph 0)
_UPM = 1000


def _glyph_box(c: int) -> tuple[int, int, int, int] | None:
    """Return the (x0,y0,x1,y1) box outline for character code ``c``, or None for a blank glyph (space)."""
    if c == 0x20:
        return None  # space: blank, advance only
    if 0x41 <= c <= 0x5A:      # A-Z: tall
        return (120, 0, 520, 700)
    if 0x61 <= c <= 0x7A:      # a-z: short (x-height)
        return (120, 0, 470, 500)
    if 0x30 <= c <= 0x39:      # 0-9: medium
        return (120, 0, 500, 660)
    return (160, 0, 360, 320)  # punctuation: small


def _simple_glyph(box: tuple[int, int, int, int]) -> bytes:
    x0, y0, x1, y1 = box
    xs = [x0, x1, x1, x0]
    ys = [y0, y0, y1, y1]
    out = struct.pack(">h", 1)                        # numberOfContours
    out += struct.pack(">hhhh", x0, y0, x1, y1)       # bbox
    out += struct.pack(">H", 3)                       # endPtsOfContours[0] = 3 (4 points)
    out += struct.pack(">H", 0)                       # instructionLength
    out += bytes([0x01, 0x01, 0x01, 0x01])            # flags: 4 on-curve points
    prev = 0
    for x in xs:
        out += struct.pack(">h", x - prev)            # x delta (int16)
        prev = x
    prev = 0
    for y in ys:
        out += struct.pack(">h", y - prev)            # y delta (int16)
        prev = y
    assert len(out) % 2 == 0
    return out


def _sfnt_checksum(data: bytes) -> int:
    padded = data + b"\x00" * ((4 - len(data) % 4) % 4)
    total = 0
    for i in range(0, len(padded), 4):
        total = (total + struct.unpack(">I", padded[i:i + 4])[0]) & 0xFFFFFFFF
    return total


def _sfnt_pad4(data: bytes) -> bytes:
    return data + b"\x00" * ((4 - len(data) % 4) % 4)


def make_ttf() -> bytes:
    # glyf + loca (short format): glyph 0 = .notdef (empty), glyph i (1..95) = char (0x1F + i).
    glyf = b""
    loca_offsets = [0]
    for g in range(_NUM_GLYPHS):
        if g == 0:
            entry = b""  # .notdef, empty
        else:
            box = _glyph_box(_FIRST_CHAR + (g - 1))
            entry = b"" if box is None else _simple_glyph(box)
        glyf += entry
        loca_offsets.append(len(glyf))
    loca = b"".join(struct.pack(">H", off // 2) for off in loca_offsets)  # short loca (offset/2)

    # cmap format 4 covering 0x20..0x7E → glyphs 1..95 (idDelta = -31), plus the required 0xFFFF terminator.
    sub = struct.pack(">H", 4)      # format
    sub += struct.pack(">H", 0)     # length placeholder
    sub += struct.pack(">H", 0)     # language
    sub += struct.pack(">H", 4)     # segCountX2 (2 segments)
    sub += struct.pack(">H", 4)     # searchRange
    sub += struct.pack(">H", 1)     # entrySelector
    sub += struct.pack(">H", 0)     # rangeShift
    sub += struct.pack(">HH", _LAST_CHAR, 0xFFFF)   # endCode
    sub += struct.pack(">H", 0)                      # reservedPad
    sub += struct.pack(">HH", _FIRST_CHAR, 0xFFFF)  # startCode
    sub += struct.pack(">hh", _FIRST_CHAR * -1 + 1, 1)  # idDelta: (1 - 0x20) = -31, and 1 for terminator
    sub += struct.pack(">HH", 0, 0)                  # idRangeOffset
    sub = sub[:2] + struct.pack(">H", len(sub)) + sub[4:]  # patch length
    cmap = struct.pack(">HH", 0, 1) + struct.pack(">HHI", 3, 1, 12) + sub

    # head (54 bytes).
    created = 3_900_000_000
    head_fields = [
        (">I", 0x00010000), (">I", 0x00010000), (">I", 0), (">I", 0x5F0F3CF5),
        (">H", 0), (">H", _UPM),
        (">q", created), (">q", created),
        (">h", 120), (">h", 0), (">h", 520), (">h", 700),  # global bbox
        (">H", 0), (">H", 8), (">h", 0), (">h", 0), (">h", 0),  # indexToLocFormat=0 (short)
    ]
    head = b"".join(struct.pack(fmt, val) for fmt, val in head_fields)
    assert len(head) == 54, len(head)

    # hhea (36 bytes) — numberOfHMetrics == numGlyphs.
    hhea_fields = [
        (">I", 0x00010000), (">h", 800), (">h", -200), (">h", 100), (">H", 600),
        (">h", 0), (">h", 520), (">h", _UPM),
        (">h", 1), (">h", 0), (">h", 0),
        (">h", 0), (">h", 0), (">h", 0), (">h", 0),
        (">h", 0), (">H", _NUM_GLYPHS),
    ]
    hhea = b"".join(struct.pack(fmt, val) for fmt, val in hhea_fields)
    assert len(hhea) == 36, len(hhea)

    # hmtx: one longHorMetric per glyph (advance 600, lsb 0).
    hmtx = b"".join(struct.pack(">Hh", 600, 0) for _ in range(_NUM_GLYPHS))

    # maxp v1.0 (32 bytes).
    maxp = struct.pack(">I", 0x00010000) + struct.pack(">H", _NUM_GLYPHS)
    maxp += struct.pack(">H", 4)   # maxPoints
    maxp += struct.pack(">H", 1)   # maxContours
    maxp += struct.pack(">H", 0)   # maxCompositePoints
    maxp += struct.pack(">H", 0)   # maxCompositeContours
    maxp += struct.pack(">H", 2)   # maxZones
    maxp += b"\x00" * (32 - len(maxp))
    assert len(maxp) == 32, len(maxp)

    # name: required nameIDs (1,2,3,4,6), Windows/Unicode-BMP/en-US, UTF-16BE.
    name_records = [
        (1, "CPE Sample Mono"), (2, "Regular"),
        (3, "CPE Sample Mono Regular 2026 gen_samples.py"),
        (4, "CPE Sample Mono Regular"), (6, "CPESampleMono-Regular"),
    ]
    storage = b""
    recs = b""
    for name_id, text in name_records:
        enc = text.encode("utf-16-be")
        recs += struct.pack(">HHHHHH", 3, 1, 0x0409, name_id, len(enc), len(storage))
        storage += enc
    name = struct.pack(">HHH", 0, len(name_records), 6 + len(recs)) + recs + storage

    # OS/2 v0 (78 bytes).
    os2 = struct.pack(">H", 0) + struct.pack(">h", 600) + struct.pack(">H", 400) + struct.pack(">H", 5) + struct.pack(">H", 0)
    os2 += b"".join(struct.pack(">h", 0) for _ in range(8))  # sub/superscript metrics
    os2 += struct.pack(">h", 50) + struct.pack(">h", 300)    # strikeout size/position
    os2 += struct.pack(">h", 0)                              # sFamilyClass
    os2 += b"\x00" * 10                                      # panose
    os2 += struct.pack(">IIII", 0, 0, 0, 0)                  # unicode ranges
    os2 += b"NONE"                                           # achVendID
    os2 += struct.pack(">H", 0x0040)                         # fsSelection: REGULAR
    os2 += struct.pack(">HH", _FIRST_CHAR, _LAST_CHAR)       # usFirstCharIndex/usLastCharIndex
    os2 += struct.pack(">hhh", 800, -200, 200)               # sTypo Ascender/Descender/LineGap
    os2 += struct.pack(">HH", 800, 200)                      # usWinAscent/Descent
    assert len(os2) == 78, len(os2)

    # post v3.0 (32 bytes).
    post = struct.pack(">iiHHIIIII", 0x00030000, 0, 0, 0, 0, 0, 0, 0, 0)
    assert len(post) == 32, len(post)

    tables = {
        b"OS/2": os2, b"cmap": cmap, b"glyf": glyf, b"head": head, b"hhea": hhea,
        b"hmtx": hmtx, b"loca": loca, b"maxp": maxp, b"name": name, b"post": post,
    }
    tags = sorted(tables.keys())
    num_tables = len(tags)
    search_pow2 = 1
    while search_pow2 * 2 <= num_tables:
        search_pow2 *= 2
    search_range = search_pow2 * 16
    entry_selector = search_pow2.bit_length() - 1
    range_shift = num_tables * 16 - search_range

    header = struct.pack(">IHHHH", 0x00010000, num_tables, search_range, entry_selector, range_shift)
    dir_size = 16 * num_tables
    offset = 12 + dir_size
    directory = b""
    body = b""
    for tag in tags:
        data = tables[tag]
        directory += struct.pack(">4sIII", tag, _sfnt_checksum(data), offset, len(data))
        padded = _sfnt_pad4(data)
        body += padded
        offset += len(padded)

    font = header + directory + body

    # Two-pass checkSumAdjustment patched into `head` (offset 8 within the table).
    head_index = tags.index(b"head")
    head_file_offset = 12 + dir_size + sum(len(_sfnt_pad4(tables[t])) for t in tags[:head_index])
    checksum_adjustment = (0xB1B0AFBA - _sfnt_checksum(font)) & 0xFFFFFFFF
    font = font[:head_file_offset + 8] + struct.pack(">I", checksum_adjustment) + font[head_file_offset + 12:]
    return font


# ════════════════════════════════════════════════════════════════════════════════════════════════════
# PDF — a valid, loadable multi-page PDF (byte-accurate xref) with per-page colour + text; /Info baseline.
# ════════════════════════════════════════════════════════════════════════════════════════════════════
def make_pdf() -> bytes:
    """A real, VALID 3-page PDF with a byte-accurate ``xref`` table, a shared Helvetica font resource and a
    coloured rectangle + a line of text on each page — so WebView2's built-in PDF viewer renders real content.
    Keeps the full ``/Info`` dictionary baseline (Title/Author/Subject/Keywords/Creator/Producer/dates) that
    ``read_pdf`` surfaces and ``sample_fixtures.rs::pdf_info_baseline`` asserts."""
    info_dict = (
        b"<< /Title (" + TITLE.encode() + b") /Author (" + ARTIST.encode() + b")"
        b" /Subject (Baseline fixture) /Keywords (cpe,sample,baseline)"
        b" /Creator (gen_samples.py) /Producer (gen_samples.py)"
        b" /CreationDate (D:20260725110000) /ModDate (D:20260725110000) >>"
    )
    colours = [b"0 0 1", b"0 0.6 0", b"0.8 0 0"]  # blue / green / red
    labels = [b"Baseline Sample - Page 1 of 3", b"Baseline Sample - Page 2 of 3", b"Baseline Sample - Page 3 of 3"]
    contents = []
    for col, lbl in zip(colours, labels):
        contents.append(
            b"q\n" + col + b" rg\n40 40 320 220 re\nf\nQ\n"
            b"BT\n/F1 20 Tf\n0 0 0 rg\n60 270 Td\n(" + lbl + b") Tj\nET\n"
        )

    # Fixed object layout (numbers referenced across objects, so keep them stable):
    #   1 Catalog, 2 Pages, 3/4/5 Page objects, 6/7/8 Content streams, 9 Font, 10 Info.
    objs = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 300] /Contents 6 0 R"
           b" /Resources << /Font << /F1 9 0 R >> >> >>",
        4: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 300] /Contents 7 0 R"
           b" /Resources << /Font << /F1 9 0 R >> >> >>",
        5: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 300] /Contents 8 0 R"
           b" /Resources << /Font << /F1 9 0 R >> >> >>",
        6: b"<< /Length " + str(len(contents[0])).encode() + b" >>\nstream\n" + contents[0] + b"endstream",
        7: b"<< /Length " + str(len(contents[1])).encode() + b" >>\nstream\n" + contents[1] + b"endstream",
        8: b"<< /Length " + str(len(contents[2])).encode() + b" >>\nstream\n" + contents[2] + b"endstream",
        9: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        10: info_dict,
    }
    count = len(objs)
    body = b"%PDF-1.4\n"
    offsets: dict[int, int] = {}
    for num in range(1, count + 1):
        offsets[num] = len(body)
        body += f"{num} 0 obj\n".encode() + objs[num] + b"\nendobj\n"

    xref_start = len(body)
    xref = f"xref\n0 {count + 1}\n0000000000 65535 f \n".encode()
    for num in range(1, count + 1):
        xref += f"{offsets[num]:010d} 00000 n \n".encode()
    trailer = (
        f"trailer\n<< /Size {count + 1} /Root 1 0 R /Info {count} 0 R >>\n"
        f"startxref\n{xref_start}\n%%EOF"
    ).encode()
    return body + xref + trailer


def make_malformed_pdf() -> bytes:
    """The ORIGINAL degenerate fixture (byte-identical to the pre-CPE-1358 ``doc.pdf``): a zero-page
    ``/Kids [] /Count 0`` page tree, a ``trailer`` but no ``xref`` table — not a loadable PDF. Kept, unchanged,
    as ``documents/malformed.pdf``: the deliberate CPE-1357 regression trigger — opening it must NOT crash the
    app (it degrades to the metadata pane). See ``gui-smoke/specs/samples.smoke.ts``."""
    return (
        b"%PDF-1.4\n"
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n"
        b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n"
        b"3 0 obj\n<< /Title (" + TITLE.encode() + b") /Author (" + ARTIST.encode() + b")"
        b" /Subject (Baseline fixture) /Keywords (cpe,sample,baseline)"
        b" /Creator (gen_samples.py) /Producer (gen_samples.py)"
        b" /CreationDate (D:20260725110000) /ModDate (D:20260725110000) >>\nendobj\n"
        b"trailer\n<< /Root 1 0 R /Info 3 0 R /Size 4 >>\n%%EOF"
    )


# ════════════════════════════════════════════════════════════════════════════════════════════════════
# Text — realistic multi-line content for the plain-text / markdown / json / csv / tsv / code previews.
# ════════════════════════════════════════════════════════════════════════════════════════════════════
TEXT_FILES = {
    "text/notes.txt": (
        "Baseline sample text file\n"
        "=========================\n\n"
        "This file exercises the plain-text preview: line and word counts, scrolling,\n"
        "and encoding detection. It deliberately spans multiple paragraphs and lines so\n"
        "the reader has real content to lay out.\n\n"
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor\n"
        "incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis\n"
        "nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.\n\n"
        "- A short bullet list\n"
        "- with a few items\n"
        "- to vary the line shapes\n\n"
        "End of sample.\n"
    ),
    "text/readme.md": (
        "# Baseline Markdown Sample\n\n"
        "A **markdown** fixture for the docs/preview path. It covers the common block types so the\n"
        "rendered-markdown preview has real structure to show.\n\n"
        "## Features\n\n"
        "- Headings (h1/h2)\n"
        "- **Bold** and *italic* text\n"
        "- A fenced code block\n"
        "- A small table\n\n"
        "## Example\n\n"
        "```python\n"
        "def greet(name):\n"
        "    return f\"Hello, {name}!\"\n"
        "```\n\n"
        "## Table\n\n"
        "| Field  | Value            |\n"
        "|--------|------------------|\n"
        "| Title  | Baseline Sample  |\n"
        "| Artist | CPE Test Suite   |\n\n"
        "> A blockquote to round things out.\n"
    ),
    "text/data.json": (
        "{\n"
        '  "name": "baseline",\n'
        '  "version": 2,\n'
        '  "generated_by": "gen_samples.py",\n'
        '  "tags": ["cpe", "sample", "baseline", "json"],\n'
        '  "metadata": {\n'
        '    "title": "Baseline Sample",\n'
        '    "artist": "CPE Test Suite",\n'
        '    "year": 2026\n'
        "  },\n"
        '  "items": [\n'
        '    { "id": 1, "label": "alpha", "value": 10.5, "active": true },\n'
        '    { "id": 2, "label": "beta", "value": 20.0, "active": false },\n'
        '    { "id": 3, "label": "gamma", "value": 30.25, "active": true }\n'
        "  ]\n"
        "}\n"
    ),
    "text/table.csv": (
        "id,name,department,city,salary,start_date\n"
        "1,Ann,Engineering,Oslo,82000,2024-01-15\n"
        "2,Bo,Design,Berlin,74000,2024-03-02\n"
        "3,Cai,Engineering,Kyoto,90000,2023-11-20\n"
        "4,Dana,Marketing,Lima,68000,2025-02-10\n"
        "5,Eli,Sales,Cairo,71000,2024-07-30\n"
        "6,Faye,Engineering,Perth,88000,2023-09-05\n"
        "7,Gus,Support,Quito,60000,2025-01-22\n"
        "8,Hana,Design,Reno,76000,2024-05-14\n"
    ),
    "text/table.tsv": (
        "id\tname\tdepartment\tcity\tsalary\tstart_date\n"
        "1\tAnn\tEngineering\tOslo\t82000\t2024-01-15\n"
        "2\tBo\tDesign\tBerlin\t74000\t2024-03-02\n"
        "3\tCai\tEngineering\tKyoto\t90000\t2023-11-20\n"
        "4\tDana\tMarketing\tLima\t68000\t2025-02-10\n"
        "5\tEli\tSales\tCairo\t71000\t2024-07-30\n"
        "6\tFaye\tEngineering\tPerth\t88000\t2023-09-05\n"
    ),
    "text/hello.py": (
        '#!/usr/bin/env python3\n'
        '"""A small but real Python sample for the code-preview + syntax-highlight path."""\n'
        'from __future__ import annotations\n\n'
        'import sys\n\n\n'
        'def fib(n: int) -> int:\n'
        '    """Return the n-th Fibonacci number (iterative)."""\n'
        '    a, b = 0, 1\n'
        '    for _ in range(n):\n'
        '        a, b = b, a + b\n'
        '    return a\n\n\n'
        'def main(argv: list[str]) -> int:\n'
        '    count = int(argv[1]) if len(argv) > 1 else 10\n'
        '    for i in range(count):\n'
        '        print(f"fib({i}) = {fib(i)}")\n'
        '    return 0\n\n\n'
        'if __name__ == "__main__":\n'
        '    raise SystemExit(main(sys.argv))\n'
    ),
    # A real Jupyter notebook (CPE-1616): markdown + code cells, and every output kind the notebook
    # preview renders (stream stdout, an execute_result text/plain, a display_data image/png, and an
    # error traceback), plus a raw cell. Pure JSON — deterministic, no ffmpeg/PIL needed. Kept as a plain
    # Python dict -> json.dumps (rather than a hand-typed JSON string) so it can never drift out of valid
    # nbformat shape.
    "text/notebook.ipynb": json.dumps(
        {
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {
                "kernelspec": {"display_name": "Python 3", "language": "python", "name": "python3"},
                "language_info": {"name": "python", "version": "3.11"},
            },
            "cells": [
                {
                    "cell_type": "markdown",
                    "metadata": {},
                    "source": [
                        "# Baseline Notebook Sample\n",
                        "\n",
                        "A **Jupyter** notebook fixture for the notebook preview path (CPE-1616). It exercises\n",
                        "markdown, code, and every supported output kind.\n",
                    ],
                },
                {
                    "cell_type": "code",
                    "execution_count": 1,
                    "metadata": {},
                    "outputs": [
                        {"output_type": "stream", "name": "stdout", "text": ["fib(5) = 5\n"]},
                        {
                            "output_type": "execute_result",
                            "execution_count": 1,
                            "metadata": {},
                            "data": {"text/plain": ["5"]},
                        },
                    ],
                    "source": [
                        "def fib(n):\n",
                        "    a, b = 0, 1\n",
                        "    for _ in range(n):\n",
                        "        a, b = b, a + b\n",
                        "    return a\n",
                        "\n",
                        "print(f\"fib(5) = {fib(5)}\")\n",
                        "fib(5)",
                    ],
                },
                {
                    "cell_type": "code",
                    "execution_count": 2,
                    "metadata": {},
                    "outputs": [
                        {
                            "output_type": "display_data",
                            "metadata": {},
                            "data": {
                                "image/png": (
                                    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk"
                                    "+A8AAQUBAScY42YAAAAASUVORK5CYII="
                                )
                            },
                        },
                    ],
                    "source": [
                        "# A tiny inline plot placeholder (1x1 PNG) exercising the image/png output path\n",
                        "show_plot()",
                    ],
                },
                {
                    "cell_type": "code",
                    "execution_count": 3,
                    "metadata": {},
                    "outputs": [
                        {
                            "output_type": "error",
                            "ename": "ZeroDivisionError",
                            "evalue": "division by zero",
                            "traceback": [
                                "Traceback (most recent call last):",
                                "ZeroDivisionError: division by zero",
                            ],
                        },
                    ],
                    "source": ["1 / 0"],
                },
                {
                    "cell_type": "raw",
                    "metadata": {},
                    "source": [
                        "A raw cell — shown as plain preformatted text, never rendered as markdown or highlighted.\n"
                    ],
                },
            ],
        },
        indent=1,
        ensure_ascii=False,
    )
    + "\n",
    # A mixed-level log fixture for the log-preview path (CPE-1618): every recognized shape (bracketed
    # timestamp + level, ISO timestamp + level, "LEVEL:" prefix, "[LEVEL]" prefix, Android logcat
    # single-letter), one line that deliberately mentions "error" in prose with no real level marker
    # (must NOT be misclassified), and one line wrapped in real ANSI SGR colour codes (must render clean,
    # not literal escape-code garbage).
    "text/app.log": (
        "[2026-08-11 09:14:01] INFO  Starting sample-service on port 8080\n"
        "[2026-08-11 09:14:01] DEBUG Loaded configuration from /etc/sample-service/config.yaml\n"
        "[2026-08-11 09:14:02] WARN  Config value 'cache_ttl' missing, using default 300s\n"
        "[2026-08-11 09:14:02] TRACE Socket state transition: CLOSED -> CONNECTING\n"
        "2026-08-11T09:14:03Z ERROR Failed to connect to database: connection refused\n"
        "[2026-08-11 09:14:03] DEBUG Retrying connection (attempt 1/3)\n"
        "[2026-08-11 09:14:04] INFO  Database connection established\n"
        "E/NetworkClient: Failed to reach api.example.com (timeout after 5000ms)\n"
        "W/NetworkClient: Falling back to cached response\n"
        "I/ActivityManager: Displaying com.example.app\n"
        "ERROR: Unhandled exception in request handler\n"
        "[WARN] disk usage above 90% on /var\n"
        "Request payload: userId=42 action=checkout total=59.99\n"
        "User asked about a checkout error they saw yesterday "
        "— this line intentionally has no real level marker.\n"
        "\x1b[31mERROR\x1b[0m Payment gateway timeout after 3 retries\n"
        "[2026-08-11 09:14:10] INFO  Shutting down gracefully\n"
    ),
}


# ════════════════════════════════════════════════════════════════════════════════════════════════════
# Generic binary blob (hex-dump preview kind, CPE-1359).
# ════════════════════════════════════════════════════════════════════════════════════════════════════
def make_hex_blob() -> bytes:
    """A 256-byte non-text blob under an unrecognised extension (`.pak`) — the catch-all ``hex`` preview
    kind's coverage sample. Deterministic pseudo-random-looking bytes."""
    return bytes((i * 37 + 11) & 0xFF for i in range(256))


# ════════════════════════════════════════════════════════════════════════════════════════════════════
# Minimal-but-valid STUB fallbacks — used only when ffmpeg / PIL is unavailable, so the tree is always
# producible and every fixture still parses. These reproduce the ORIGINAL CPE-1042 tiny-but-valid files.
# ════════════════════════════════════════════════════════════════════════════════════════════════════
def _id3_text_frame(fid: bytes, text: str) -> bytes:
    body = b"\x00" + text.encode("latin-1")
    return fid + struct.pack(">I", len(body)) + b"\x00\x00" + body


def _id3_comm(text: str) -> bytes:
    body = b"\x00" + b"eng" + b"\x00" + text.encode("latin-1")
    return b"COMM" + struct.pack(">I", len(body)) + b"\x00\x00" + body


def _syncsafe(n: int) -> bytes:
    return bytes([(n >> 21) & 0x7F, (n >> 14) & 0x7F, (n >> 7) & 0x7F, n & 0x7F])


def _id3v23_tag() -> bytes:
    """The exact ID3v2.3 baseline tag (Title/Artist/Album/Year/Track/Genre + a COMM comment) — prepended to
    the real ffmpeg-encoded MPEG audio by ``make_mp3`` and used whole by ``_make_mp3_stub``."""
    frames = (
        _id3_text_frame(b"TIT2", TITLE) + _id3_text_frame(b"TPE1", ARTIST)
        + _id3_text_frame(b"TALB", ALBUM) + _id3_text_frame(b"TYER", YEAR)
        + _id3_text_frame(b"TRCK", f"{TRACK}/10") + _id3_text_frame(b"TCON", GENRE)
        + _id3_comm(COMMENT)
    )
    return b"ID3" + b"\x03\x00" + b"\x00" + _syncsafe(len(frames)) + frames


def _make_mp3_stub() -> bytes:
    return _id3v23_tag() + b"\xFF\xFB\x90\x00" + b"\x00" * 200


def _vorbis_comment_block(vendor: str = "gen_samples.py") -> bytes:
    comments = [
        f"TITLE={TITLE}", f"ARTIST={ARTIST}", f"ALBUM={ALBUM}",
        f"DATE={YEAR}", f"TRACKNUMBER={TRACK}", f"GENRE={GENRE}",
    ]
    out = struct.pack("<I", len(vendor)) + vendor.encode("utf-8")
    out += struct.pack("<I", len(comments))
    for c in comments:
        cb = c.encode("utf-8")
        out += struct.pack("<I", len(cb)) + cb
    return out


def _make_flac_stub() -> bytes:
    out = b"fLaC"
    streaminfo = bytes(34)
    out += bytes([0x00]) + struct.pack(">I", len(streaminfo))[1:] + streaminfo
    vc = _vorbis_comment_block()
    out += bytes([0x84]) + struct.pack(">I", len(vc))[1:] + vc
    out += b"\x00\x00\x00\x00"
    return out


def _ogg_crc(data: bytes) -> int:
    buf = bytearray(data)
    buf[22:26] = b"\x00\x00\x00\x00"
    crc = 0
    for b in buf:
        crc ^= b << 24
        crc &= 0xFFFFFFFF
        for _ in range(8):
            crc = ((crc << 1) ^ 0x04C11DB7) & 0xFFFFFFFF if crc & 0x80000000 else (crc << 1) & 0xFFFFFFFF
    return crc


def _make_ogg_stub() -> bytes:
    packet = b"\x03vorbis" + _vorbis_comment_block()
    segs = []
    rem = len(packet)
    while rem >= 255:
        segs.append(255)
        rem -= 255
    segs.append(rem)
    page = (b"OggS" + b"\x00" + b"\x00" + b"\x00" * 8 + struct.pack("<I", 1)
            + struct.pack("<I", 0) + b"\x00\x00\x00\x00" + bytes([len(segs)]) + bytes(segs) + packet)
    crc = _ogg_crc(page)
    return page[:22] + struct.pack("<I", crc) + page[26:]


def _mp4_box(kind: bytes, payload: bytes) -> bytes:
    return struct.pack(">I", len(payload) + 8) + kind + payload


def _ilst_atom(name: bytes, text: str) -> bytes:
    data = struct.pack(">I", 1) + b"\x00\x00\x00\x00" + text.encode("utf-8")
    return _mp4_box(name, _mp4_box(b"data", data))


def _make_mp4_stub() -> bytes:
    cp = b"\xA9"
    ilst = _mp4_box(b"ilst",
                    _ilst_atom(cp + b"nam", TITLE) + _ilst_atom(cp + b"ART", ARTIST)
                    + _ilst_atom(cp + b"alb", ALBUM) + _ilst_atom(cp + b"day", YEAR))
    meta = _mp4_box(b"meta", b"\x00\x00\x00\x00" + ilst)
    udta = _mp4_box(b"udta", meta)
    moov = _mp4_box(b"moov", udta)
    ftyp = _mp4_box(b"ftyp", b"isom" + struct.pack(">I", 0x200) + b"isomiso2mp41")
    return ftyp + moov


def _png_chunk(kind: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)


def _make_png_stub() -> bytes:
    w = h = 2
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)
    raw = b""
    for _ in range(h):
        raw += b"\x00" + b"\x4c\x8b\xf5\xff\x99\x66"[: 3 * w]
    idat = zlib.compress(raw, 9)
    return b"\x89PNG\r\n\x1a\n" + _png_chunk(b"IHDR", ihdr) + _png_chunk(b"IDAT", idat) + _png_chunk(b"IEND", b"")


def _make_jpeg_stub() -> bytes:
    entries = [
        (0x010E, "Baseline EXIF sample"), (0x010F, "CPE"), (0x0110, "Fixture Cam"),
        (0x0131, "gen_samples.py"), (0x0132, "2026:07:25 11:00:00"), (0x013B, ARTIST), (0x8298, "CC0"),
    ]
    n = len(entries)
    ifd_start = 8
    data_start = ifd_start + 2 + 12 * n + 4
    ifd = struct.pack("<H", n)
    blob = b""
    for tag, val in entries:
        b = val.encode("ascii") + b"\x00"
        if len(b) <= 4:
            valfield = b + b"\x00" * (4 - len(b))
        else:
            valfield = struct.pack("<I", data_start + len(blob))
            blob += b
        ifd += struct.pack("<HHI", tag, 2, len(b)) + valfield
    ifd += struct.pack("<I", 0)
    tiff = b"II" + struct.pack("<H", 0x2A) + struct.pack("<I", ifd_start) + ifd + blob
    app1_payload = b"Exif\x00\x00" + tiff
    app1 = b"\xFF\xE1" + struct.pack(">H", len(app1_payload) + 2) + app1_payload
    return b"\xFF\xD8" + app1 + b"\xFF\xD9"


def _make_tiff_stub() -> bytes:
    w = h = 2
    row = bytes([0x4C, 0x8B, 0xF5, 0x99, 0x66, 0x33])
    strip = row * h
    strip_offset = 8
    ifd_offset = strip_offset + len(strip)
    SHORT, LONG, RATIONAL = 3, 4, 5
    entries = [
        (256, SHORT, 1, struct.pack("<H", w)), (257, SHORT, 1, struct.pack("<H", h)),
        (258, SHORT, 3, struct.pack("<I", 0)), (259, SHORT, 1, struct.pack("<H", 1)),
        (262, SHORT, 1, struct.pack("<H", 2)), (273, LONG, 1, struct.pack("<I", strip_offset)),
        (277, SHORT, 1, struct.pack("<H", 3)), (278, SHORT, 1, struct.pack("<H", h)),
        (279, LONG, 1, struct.pack("<I", len(strip))), (282, RATIONAL, 1, struct.pack("<I", 0)),
        (283, RATIONAL, 1, struct.pack("<I", 0)), (296, SHORT, 1, struct.pack("<H", 2)),
    ]
    ifd_size = 2 + 12 * len(entries) + 4
    bits_off = ifd_offset + ifd_size
    xres_off = bits_off + 6
    yres_off = xres_off + 8
    entries[2] = (258, SHORT, 3, struct.pack("<I", bits_off))
    entries[9] = (282, RATIONAL, 1, struct.pack("<I", xres_off))
    entries[10] = (283, RATIONAL, 1, struct.pack("<I", yres_off))
    ifd = struct.pack("<H", len(entries))
    for tag, typ, count, val in entries:
        ifd += struct.pack("<HHI", tag, typ, count) + val.ljust(4, b"\x00")
    ifd += struct.pack("<I", 0)
    extra = struct.pack("<HHH", 8, 8, 8) + struct.pack("<II", 72, 1) + struct.pack("<II", 72, 1)
    header = b"II" + struct.pack("<H", 42) + struct.pack("<I", ifd_offset)
    return header + strip + ifd + extra


# ════════════════════════════════════════════════════════════════════════════════════════════════════
def main() -> None:
    print(f"Generating samples under {SAMPLES} …")
    print(f"  tools: ffmpeg={'yes' if FFMPEG else 'NO (audio/video stubs)'}, PIL={'yes' if HAVE_PIL else 'NO (image stubs)'}")

    wav = synth_wav()
    write("audio/track.mp3", make_mp3(wav))
    write("audio/track.flac", make_flac(wav))
    write("audio/track.ogg", make_ogg(wav))
    write("images/photo.jpg", make_jpeg())
    write("images/pixel.png", make_png())
    write("documents/doc.pdf", make_pdf())
    write("documents/malformed.pdf", make_malformed_pdf())
    write("video/clip.mp4", make_mp4())
    for rel, txt in TEXT_FILES.items():
        write(rel, txt.encode("utf-8"))
    write("images/photo.tiff", make_tiff())
    write("archives/sample.zip", make_zip())
    write("archives/sample.rar", make_rar())
    write("database/mini.sqlite", make_sqlite())
    write("other/tiny.wasm", make_wasm())
    write("other/blob.pak", make_hex_blob())
    write("fonts/mini.ttf", make_ttf())

    for msg in _NOTES:
        print(f"  NOTE: {msg}")
    print("Done. Remember: samples/ is PRISTINE — copy to .sandbox/ before editing (see samples/README.md).")


if __name__ == "__main__":
    main()
