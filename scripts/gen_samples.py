#!/usr/bin/env python3
"""Generate the pristine sample-data baseline under ``samples/`` (CPE-1042).

Every file is small, synthetic (no copyrighted media), and **deterministic** — re-running this script
reproduces byte-identical output. Each media file carries the FIXED, known metadata recorded in
``samples/README.md`` so the tree is a stable baseline for manual/GUI checks and automated fixtures.

The formats are minimal-but-valid: the app's read codecs (``cpe_server::media_meta::read_all``) parse the
metadata correctly. They are baselines for *metadata* checking, not full studio-quality media.

Run from the repo root:  ``python scripts/gen_samples.py``
"""
from __future__ import annotations
import os
import struct
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


def write(rel: str, data: bytes) -> None:
    path = os.path.join(SAMPLES, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)
    print(f"  {rel:28} {len(data):>7} bytes")


# ── ID3v2.3 (mp3) ───────────────────────────────────────────────────────────────────────────────────
def id3_text_frame(fid: bytes, text: str) -> bytes:
    body = b"\x00" + text.encode("latin-1")  # encoding 0 = Latin-1
    return fid + struct.pack(">I", len(body)) + b"\x00\x00" + body


def id3_comm(text: str) -> bytes:
    body = b"\x00" + b"eng" + b"\x00" + text.encode("latin-1")  # enc + lang + empty desc + text
    return b"COMM" + struct.pack(">I", len(body)) + b"\x00\x00" + body


def syncsafe(n: int) -> bytes:
    return bytes([(n >> 21) & 0x7F, (n >> 14) & 0x7F, (n >> 7) & 0x7F, n & 0x7F])


def make_mp3() -> bytes:
    frames = (
        id3_text_frame(b"TIT2", TITLE)
        + id3_text_frame(b"TPE1", ARTIST)
        + id3_text_frame(b"TALB", ALBUM)
        + id3_text_frame(b"TYER", YEAR)
        + id3_text_frame(b"TRCK", f"{TRACK}/10")
        + id3_text_frame(b"TCON", GENRE)
        + id3_comm(COMMENT)
    )
    tag = b"ID3" + b"\x03\x00" + b"\x00" + syncsafe(len(frames)) + frames
    # A single silent MPEG-1 Layer III frame header (0xFFFB...) + padding, so the file isn't tag-only.
    audio = b"\xFF\xFB\x90\x00" + b"\x00" * 200
    return tag + audio


# ── Vorbis comments (flac + ogg) ─────────────────────────────────────────────────────────────────────
def vorbis_comment_block(vendor: str = "gen_samples.py") -> bytes:
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


def make_flac() -> bytes:
    out = b"fLaC"
    # STREAMINFO (type 0), not last — 34 bytes of (mostly zero) minimal stream info.
    streaminfo = bytes(34)
    out += bytes([0x00]) + struct.pack(">I", len(streaminfo))[1:] + streaminfo
    # VORBIS_COMMENT (type 4), last metadata block (0x80 | 4).
    vc = vorbis_comment_block()
    out += bytes([0x84]) + struct.pack(">I", len(vc))[1:] + vc
    out += b"\x00\x00\x00\x00"  # a token "audio frame" tail
    return out


def make_ogg() -> bytes:
    # read_ogg scans for b"\x03vorbis" then parses the comment block that follows, so a minimal OggS page
    # carrying the vorbis comment header packet is enough.
    packet = b"\x03vorbis" + vorbis_comment_block()
    # One OggS page: magic + ver + header-type + granule(8) + serial(4) + seq(4) + crc(4) + segcount + table
    segs = []
    rem = len(packet)
    while rem >= 255:
        segs.append(255)
        rem -= 255
    segs.append(rem)
    page = (b"OggS" + b"\x00" + b"\x00" + b"\x00" * 8 + struct.pack("<I", 1)
            + struct.pack("<I", 0) + b"\x00\x00\x00\x00" + bytes([len(segs)]) + bytes(segs) + packet)
    # Fix the CRC32 (Ogg uses a non-reflected CRC with poly 0x04C11DB7, init 0, no final xor).
    crc = ogg_crc(page)
    return page[:22] + struct.pack("<I", crc) + page[26:]


def ogg_crc(data: bytes) -> int:
    # CRC field (bytes 22..26) is treated as zero while computing.
    buf = bytearray(data)
    buf[22:26] = b"\x00\x00\x00\x00"
    crc = 0
    for b in buf:
        crc ^= b << 24
        crc &= 0xFFFFFFFF
        for _ in range(8):
            if crc & 0x80000000:
                crc = ((crc << 1) ^ 0x04C11DB7) & 0xFFFFFFFF
            else:
                crc = (crc << 1) & 0xFFFFFFFF
    return crc


# ── JPEG + EXIF (little-endian TIFF in an APP1 segment) ───────────────────────────────────────────────
def make_jpeg_exif() -> bytes:
    # IFD0 ASCII tags → (tag id, value). kamadak-exif surfaces these as Make/Model/Artist/etc.
    entries = [
        (0x010E, "Baseline EXIF sample"),  # ImageDescription
        (0x010F, "CPE"),                   # Make
        (0x0110, "Fixture Cam"),           # Model
        (0x0131, "gen_samples.py"),        # Software
        (0x0132, "2026:07:25 11:00:00"),   # DateTime
        (0x013B, ARTIST),                  # Artist
        (0x8298, "CC0"),                   # Copyright
    ]
    n = len(entries)
    # Layout: TIFF header(8) + IFD(2 + 12n + 4) + overflow ASCII data.
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
        ifd += struct.pack("<HHI", tag, 2, len(b)) + valfield  # type 2 = ASCII
    ifd += struct.pack("<I", 0)  # next-IFD offset = 0
    tiff = b"II" + struct.pack("<H", 0x2A) + struct.pack("<I", ifd_start) + ifd + blob
    app1_payload = b"Exif\x00\x00" + tiff
    app1 = b"\xFF\xE1" + struct.pack(">H", len(app1_payload) + 2) + app1_payload
    return b"\xFF\xD8" + app1 + b"\xFF\xD9"  # SOI + APP1 + EOI


# ── PNG (a real, openable 2×2 image) ─────────────────────────────────────────────────────────────────
def png_chunk(kind: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)


def make_png() -> bytes:
    w = h = 2
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)  # 8-bit RGB
    raw = b""
    for _ in range(h):
        raw += b"\x00" + b"\x4c\x8b\xf5\xff\x99\x66" [: 3 * w]  # filter byte + pixels
    idat = zlib.compress(raw, 9)
    return (b"\x89PNG\r\n\x1a\n" + png_chunk(b"IHDR", ihdr) + png_chunk(b"IDAT", idat) + png_chunk(b"IEND", b""))


# ── PDF (/Info document dictionary) ──────────────────────────────────────────────────────────────────
def make_pdf() -> bytes:
    """A real, VALID 2-page PDF (CPE-1358): byte-accurate `xref` table + a 2-item `/Kids` page tree, so
    WebView2's built-in PDF viewer (the `<iframe class="preview-pdf">` preview, PreviewPane.svelte) can
    actually load it — unlike the OLD fixture this replaces (`/Kids [] /Count 0`, no `xref` table at
    all), which was a degenerate, unloadable "PDF" that crashed the app (CPE-1357; the bad bytes are
    preserved, unchanged, as the regression fixture `make_malformed_pdf()` below). Keeps the exact same
    `/Info` dictionary fields as the old fixture so `sample_fixtures.rs`'s `pdf_info_baseline` test (and
    the Metadata Studio) still see the documented baseline metadata — `read_pdf` (media_meta_read.rs)
    text-scans for `/Info N G R` and never required a real xref anyway, so only the *loadability* half of
    the fixture was ever broken, not the metadata half."""
    info_dict = (
        b"<< /Title (" + TITLE.encode() + b") /Author (" + ARTIST.encode() + b")"
        b" /Subject (Baseline fixture) /Keywords (cpe,sample,baseline)"
        b" /Creator (gen_samples.py) /Producer (gen_samples.py)"
        b" /CreationDate (D:20260725110000) /ModDate (D:20260725110000) >>"
    )
    # Two distinct, visibly different one-page content streams so a real PDF viewer shows real content
    # on each page (not just a blank canvas) — page 1 a blue square, page 2 a green square.
    content1 = b"0 0 1 rg\n20 20 160 160 re\nf\n"
    content2 = b"0 1 0 rg\n20 20 160 160 re\nf\n"

    objs = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>",
        3: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << >> >>",
        4: b"<< /Length " + str(len(content1)).encode() + b" >>\nstream\n" + content1 + b"endstream",
        5: b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 6 0 R /Resources << >> >>",
        6: b"<< /Length " + str(len(content2)).encode() + b" >>\nstream\n" + content2 + b"endstream",
        7: info_dict,
    }

    body = b"%PDF-1.4\n"
    offsets: dict[int, int] = {}
    for num in range(1, 8):
        offsets[num] = len(body)
        body += f"{num} 0 obj\n".encode() + objs[num] + b"\nendobj\n"

    xref_start = len(body)
    xref = f"xref\n0 {8}\n0000000000 65535 f \n".encode()
    for num in range(1, 8):
        xref += f"{offsets[num]:010d} 00000 n \n".encode()
    trailer = f"trailer\n<< /Size 8 /Root 1 0 R /Info 7 0 R >>\nstartxref\n{xref_start}\n%%EOF".encode()
    return body + xref + trailer


def make_malformed_pdf() -> bytes:
    """The ORIGINAL degenerate fixture (byte-identical to the pre-CPE-1358 `doc.pdf`): a zero-page
    `/Kids [] /Count 0` page tree, a `trailer` but no `xref` table at all — not a loadable PDF. Kept,
    unchanged, as `documents/malformed.pdf`: the deliberate CPE-1357 regression trigger — opening it must
    NOT crash the app (it should render/degrade gracefully) once that bug is fixed. See
    `gui-smoke/specs/samples.smoke.ts`, which opens this LAST (after every other sample) precisely so a
    still-open crash here doesn't block coverage of the rest of the walk."""
    body = (
        b"%PDF-1.4\n"
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n"
        b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n"
        b"3 0 obj\n<< /Title (" + TITLE.encode() + b") /Author (" + ARTIST.encode() + b")"
        b" /Subject (Baseline fixture) /Keywords (cpe,sample,baseline)"
        b" /Creator (gen_samples.py) /Producer (gen_samples.py)"
        b" /CreationDate (D:20260725110000) /ModDate (D:20260725110000) >>\nendobj\n"
        b"trailer\n<< /Root 1 0 R /Info 3 0 R /Size 4 >>\n%%EOF"
    )
    return body


# ── MP4 (iTunes ilst tags) ───────────────────────────────────────────────────────────────────────────
def mp4_box(kind: bytes, payload: bytes) -> bytes:
    return struct.pack(">I", len(payload) + 8) + kind + payload


def ilst_atom(name: bytes, text: str) -> bytes:
    data = struct.pack(">I", 1) + b"\x00\x00\x00\x00" + text.encode("utf-8")  # type-flag 1 = UTF-8, +locale
    return mp4_box(name, mp4_box(b"data", data))


def make_mp4() -> bytes:
    cp = b"\xA9"  # the © in ©nam etc.
    ilst = mp4_box(b"ilst",
                   ilst_atom(cp + b"nam", TITLE)
                   + ilst_atom(cp + b"ART", ARTIST)
                   + ilst_atom(cp + b"alb", ALBUM)
                   + ilst_atom(cp + b"day", YEAR))
    meta = mp4_box(b"meta", b"\x00\x00\x00\x00" + ilst)  # 4-byte version/flags prelude + ilst
    udta = mp4_box(b"udta", meta)
    moov = mp4_box(b"moov", udta)
    ftyp = mp4_box(b"ftyp", b"isom" + struct.pack(">I", 0x200) + b"isomiso2mp41")
    return ftyp + moov


# ── plain text ───────────────────────────────────────────────────────────────────────────────────────
TEXT_FILES = {
    "text/notes.txt": "Baseline sample text file.\nUsed to verify plain-text preview + line/word counts.\n",
    "text/readme.md": "# Baseline\n\nA **markdown** sample for the docs/preview path.\n\n- one\n- two\n",
    "text/data.json": '{\n  "name": "baseline",\n  "count": 3,\n  "tags": ["cpe", "sample"]\n}\n',
    "text/table.csv": "id,name,value\n1,alpha,10\n2,beta,20\n3,gamma,30\n",
    "text/table.tsv": "id\tname\tvalue\n1\talpha\t10\n2\tbeta\t20\n3\tgamma\t30\n",
    "text/hello.py": 'def main() -> None:\n    print("baseline sample")\n\n\nif __name__ == "__main__":\n    main()\n',
}


# ── TIFF (decoded-image preview path, CPE-1358) ─────────────────────────────────────────────────────
# The webview can't render TIFF natively, so the backend transcodes it to a PNG data: URL
# (DECODED_IMAGE_EXT in src/lib/preview/provider.ts). This is a real, minimal, uncompressed 2x2 RGB
# baseline TIFF — hand-built (no imaging library) with the 12 baseline tags a decoder needs
# (ImageWidth/Length, BitsPerSample, Compression=none, PhotometricInterpretation=RGB, StripOffsets/
# SamplesPerPixel/RowsPerStrip/StripByteCounts, X/YResolution, ResolutionUnit). Verified by opening it
# with Pillow (`Image.open(...).load()`) during authoring — see the PR description for that ad hoc check;
# Pillow itself is NOT a runtime dependency of this script.
def tiff_ifd_entry(tag: int, typ: int, count: int, value: bytes) -> bytes:
    return struct.pack("<HHI", tag, typ, count) + value.ljust(4, b"\x00")


def make_tiff() -> bytes:
    w = h = 2
    row = bytes([0x4C, 0x8B, 0xF5, 0x99, 0x66, 0x33])  # 2 RGB pixels/row, 6 bytes, no row padding
    strip = row * h
    strip_offset = 8
    ifd_offset = strip_offset + len(strip)

    SHORT, LONG, RATIONAL = 3, 4, 5
    entries = [
        (256, SHORT, 1, struct.pack("<H", w)),
        (257, SHORT, 1, struct.pack("<H", h)),
        (258, SHORT, 3, struct.pack("<I", 0)),  # BitsPerSample offset — patched below
        (259, SHORT, 1, struct.pack("<H", 1)),  # Compression: none
        (262, SHORT, 1, struct.pack("<H", 2)),  # PhotometricInterpretation: RGB
        (273, LONG, 1, struct.pack("<I", strip_offset)),  # StripOffsets
        (277, SHORT, 1, struct.pack("<H", 3)),  # SamplesPerPixel
        (278, SHORT, 1, struct.pack("<H", h)),  # RowsPerStrip
        (279, LONG, 1, struct.pack("<I", len(strip))),  # StripByteCounts
        (282, RATIONAL, 1, struct.pack("<I", 0)),  # XResolution offset — patched below
        (283, RATIONAL, 1, struct.pack("<I", 0)),  # YResolution offset — patched below
        (296, SHORT, 1, struct.pack("<H", 2)),  # ResolutionUnit: inches
    ]
    ifd_size = 2 + 12 * len(entries) + 4
    assert ifd_size == 150, ifd_size

    bits_off = ifd_offset + ifd_size
    xres_off = bits_off + 6
    yres_off = xres_off + 8
    entries[2] = (258, SHORT, 3, struct.pack("<I", bits_off))
    entries[9] = (282, RATIONAL, 1, struct.pack("<I", xres_off))
    entries[10] = (283, RATIONAL, 1, struct.pack("<I", yres_off))

    ifd = struct.pack("<H", len(entries))
    for tag, typ, count, val in entries:
        ifd += tiff_ifd_entry(tag, typ, count, val)
    ifd += struct.pack("<I", 0)  # no next IFD
    assert len(ifd) == ifd_size

    extra = struct.pack("<HHH", 8, 8, 8) + struct.pack("<II", 72, 1) + struct.pack("<II", 72, 1)
    header = b"II" + struct.pack("<H", 42) + struct.pack("<I", ifd_offset)
    return header + strip + ifd + extra


# ── ZIP archive (archive preview path, CPE-1358) ────────────────────────────────────────────────────
# Uses the stdlib `zipfile` module (not `.rar` — the frontend's archive preview provider does not list
# `rar` in ARCHIVE_EXT, so `samples/archives/sample.rar` renders via the generic hex-dump provider, not
# the archive-listing one; this .zip is the real "archive" preview-kind coverage sample). Fixed
# `date_time` so the output is reproducible across runs.
def make_zip() -> bytes:
    import io
    import zipfile

    buf = io.BytesIO()
    info = zipfile.ZipInfo("hello.txt", date_time=(2026, 7, 25, 11, 0, 0))
    info.compress_type = zipfile.ZIP_STORED
    info.external_attr = 0o644 << 16
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_STORED) as zf:
        zf.writestr(info, "packed inside a real .zip for CPE-1358 (archive preview coverage)\n")
    return buf.getvalue()


# ── SQLite (data-grid preview path, CPE-1358) ───────────────────────────────────────────────────────
# Uses the stdlib `sqlite3` module to write a genuinely valid, openable database with one small table —
# the DATA_GRID_EXT preview path (DataBrowser.svelte) needs a real SQLite file, not hand-rolled bytes.
# NOTE on determinism: unlike every other generator in this script, the exact page-layout bytes here can
# differ slightly across sqlite3/Python versions (no external timestamps are embedded, but internal free-
# list/page bookkeeping isn't spec-pinned) — it is always a VALID file, just not guaranteed byte-for-byte
# identical to a previous run on a different sqlite3 build. Written via a throwaway temp file (sqlite3
# needs a real path to produce final on-disk bytes, unlike ":memory:") and immediately deleted.
def make_sqlite() -> bytes:
    import sqlite3

    tmp_path = os.path.join(SAMPLES, "database", ".mini.sqlite.tmp")
    os.makedirs(os.path.dirname(tmp_path), exist_ok=True)
    if os.path.exists(tmp_path):
        os.remove(tmp_path)
    conn = sqlite3.connect(tmp_path)
    try:
        conn.execute("CREATE TABLE sample (id INTEGER PRIMARY KEY, name TEXT, value INTEGER)")
        conn.executemany(
            "INSERT INTO sample (name, value) VALUES (?, ?)",
            [("alpha", 10), ("beta", 20), ("gamma", 30)],
        )
        conn.commit()
    finally:
        conn.close()
    with open(tmp_path, "rb") as f:
        data = f.read()
    os.remove(tmp_path)
    return data


# ── WASM (info preview path, CPE-1358) ──────────────────────────────────────────────────────────────
# The smallest possible genuinely-valid WebAssembly module: just the magic + version, no sections — the
# WASM binary format spec explicitly permits this. Enough for the INFO_EXT hex/text-summary preview
# path (read_preview_info) to open a real, well-formed module.
def make_wasm() -> bytes:
    return b"\x00asm" + struct.pack("<I", 1)


# ── TTF (font preview path, CPE-1358) ───────────────────────────────────────────────────────────────
# A minimal, genuinely-valid single-glyph (.notdef only, no visible outline) TrueType font: hand-built
# sfnt wrapper (OS/2, cmap, glyf, head, hhea, hmtx, loca, maxp, name, post — the OpenType "required
# tables" list for a TrueType-outline font), byte-accurate table checksums + the two-pass
# `checkSumAdjustment` the spec requires. No font-authoring library (kept stdlib-only, matching the rest
# of this script) — verified by loading it with `fontTools.ttLib.TTFont(...)` during authoring (ad hoc
# check only, not a runtime dependency here). The app's font preview (PreviewPane.svelte's
# `loadFontFor`) already degrades gracefully via `FontFace.load()`'s own catch on ANY font it can't use,
# so this only needs to be a well-formed sfnt, not a font with real glyph outlines.
def sfnt_checksum(data: bytes) -> int:
    padded = data + b"\x00" * ((4 - len(data) % 4) % 4)
    total = 0
    for i in range(0, len(padded), 4):
        total = (total + struct.unpack(">I", padded[i : i + 4])[0]) & 0xFFFFFFFF
    return total


def sfnt_pad4(data: bytes) -> bytes:
    return data + b"\x00" * ((4 - len(data) % 4) % 4)


def make_ttf() -> bytes:
    units_per_em = 1000
    num_glyphs = 1

    # glyf/loca: one empty (.notdef) glyph, no outline.
    glyf = b""
    loca = struct.pack(">HH", 0, 0)  # short format (offsets/2); both entries at 0

    # cmap: format-4 subtable carrying only the required terminator segment (no real mappings).
    cmap_sub = struct.pack(">HHHHHHH", 4, 0, 0, 2, 2, 0, 0)  # format,length(placeholder),lang,segCountX2,searchRange,entrySelector,rangeShift
    cmap_sub += struct.pack(">H", 0xFFFF)  # endCode[0]
    cmap_sub += struct.pack(">H", 0)  # reservedPad
    cmap_sub += struct.pack(">H", 0xFFFF)  # startCode[0]
    cmap_sub += struct.pack(">h", 1)  # idDelta[0]
    cmap_sub += struct.pack(">H", 0)  # idRangeOffset[0]
    cmap_sub = cmap_sub[:2] + struct.pack(">H", len(cmap_sub)) + cmap_sub[4:]  # patch `length`
    cmap = struct.pack(">HH", 0, 1) + struct.pack(">HHI", 3, 1, 12) + cmap_sub  # version,numTables + one Windows/Unicode-BMP encoding record

    # head (54 bytes, spec-fixed layout).
    created_1904 = 3_900_000_000  # arbitrary fixed epoch-1904 seconds — deterministic, date itself is irrelevant to validity
    head_fields = [
        (">I", 0x00010000), (">I", 0x00010000), (">I", 0), (">I", 0x5F0F3CF5),
        (">H", 0), (">H", units_per_em),
        (">q", created_1904), (">q", created_1904),
        (">h", 0), (">h", 0), (">h", 0), (">h", 0),  # xMin/yMin/xMax/yMax — empty glyph, zero bbox
        (">H", 0), (">H", 8), (">h", 2), (">h", 0), (">h", 0),
    ]
    head = b"".join(struct.pack(fmt, val) for fmt, val in head_fields)
    assert len(head) == 54, len(head)

    # hhea (36 bytes).
    hhea_fields = [
        (">I", 0x00010000), (">h", 800), (">h", -200), (">h", 200), (">H", 600),
        (">h", 0), (">h", 0), (">h", units_per_em),
        (">h", 1), (">h", 0), (">h", 0),
        (">h", 0), (">h", 0), (">h", 0), (">h", 0),
        (">h", 0), (">H", 1),
    ]
    hhea = b"".join(struct.pack(fmt, val) for fmt, val in hhea_fields)
    assert len(hhea) == 36, len(hhea)

    # hmtx: one longHorMetric (numberOfHMetrics == numGlyphs == 1).
    hmtx = struct.pack(">Hh", 600, 0)

    # maxp v1.0 (32 bytes).
    maxp_fields = [(">I", 0x00010000), (">H", num_glyphs)] + [(">H", 0)] * 4 + [(">H", 1)] + [(">H", 0)] * 8
    maxp = b"".join(struct.pack(fmt, val) for fmt, val in maxp_fields)
    assert len(maxp) == 32, len(maxp)

    # name: required nameIDs (1,2,3,4,6), Windows/Unicode-BMP/en-US, UTF-16BE strings.
    name_records = [
        (1, "CPE Sample Font"), (2, "Regular"),
        (3, "CPE Sample Font Regular 2026 gen_samples.py"),
        (4, "CPE Sample Font Regular"), (6, "CPESampleFont-Regular"),
    ]
    storage = b""
    recs = b""
    for name_id, text in name_records:
        enc = text.encode("utf-16-be")
        recs += struct.pack(">HHHHHH", 3, 1, 0x0409, name_id, len(enc), len(storage))
        storage += enc
    name = struct.pack(">HHH", 0, len(name_records), 6 + len(recs)) + recs + storage

    # OS/2 v0 (78 bytes).
    os2_fields = (
        [(">H", 0), (">h", 0), (">H", 400), (">H", 5), (">H", 0)]
        + [(">h", 0)] * 8  # sub/superscript size/offset x/y
        + [(">h", 0), (">h", 0)]  # strikeout size/position
        + [(">h", 0)]  # sFamilyClass
    )
    os2 = b"".join(struct.pack(fmt, val) for fmt, val in os2_fields)
    os2 += b"\x00" * 10  # panose
    os2 += struct.pack(">IIII", 0, 0, 0, 0)  # unicode ranges
    os2 += b"NONE"  # achVendID
    os2 += struct.pack(">H", 0x0040)  # fsSelection: REGULAR
    os2 += struct.pack(">HH", 0x0000, 0xFFFF)  # usFirstCharIndex/usLastCharIndex
    os2 += struct.pack(">hhh", 800, -200, 200)  # sTypoAscender/Descender/LineGap
    os2 += struct.pack(">HH", 800, 200)  # usWinAscent/Descent
    assert len(os2) == 78, len(os2)

    # post v3.0 — no glyph names (32 bytes).
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
        directory += struct.pack(">4sIII", tag, sfnt_checksum(data), offset, len(data))
        padded = sfnt_pad4(data)
        body += padded
        offset += len(padded)

    font = header + directory + body

    # Patch checkSumAdjustment into `head` (2nd field, byte offset 8 within the table) — computed over
    # the WHOLE file with the field zeroed first, per the sfnt spec's two-pass algorithm. The `head`
    # table's own directory-entry checksum (already written above) is intentionally left as computed
    # with the field zeroed; only the actual bytes in the body change here.
    head_index = tags.index(b"head")
    head_file_offset = 12 + dir_size + sum(len(sfnt_pad4(tables[t])) for t in tags[:head_index])
    checksum_adjustment = (0xB1B0AFBA - sfnt_checksum(font)) & 0xFFFFFFFF
    font = (
        font[: head_file_offset + 8]
        + struct.pack(">I", checksum_adjustment)
        + font[head_file_offset + 12 :]
    )
    return font


def main() -> None:
    print(f"Generating pristine samples under {SAMPLES} …")
    write("audio/track.mp3", make_mp3())
    write("audio/track.flac", make_flac())
    write("audio/track.ogg", make_ogg())
    write("images/photo.jpg", make_jpeg_exif())
    write("images/pixel.png", make_png())
    write("documents/doc.pdf", make_pdf())
    write("documents/malformed.pdf", make_malformed_pdf())
    write("video/clip.mp4", make_mp4())
    for rel, txt in TEXT_FILES.items():
        write(rel, txt.encode("utf-8"))
    # CPE-1358: sample-coverage fill — one valid fixture per remaining supported preview KIND
    # (src/lib/preview/provider.ts) that didn't already have one. See samples/README.md's coverage
    # table and src/lib/sampleCoverage.test.ts (the CI ratchet asserting this coverage never regresses).
    write("images/photo.tiff", make_tiff())
    write("archives/sample.zip", make_zip())
    write("database/mini.sqlite", make_sqlite())
    write("other/tiny.wasm", make_wasm())
    write("fonts/mini.ttf", make_ttf())
    print("Done. Remember: samples/ is PRISTINE — copy to .sandbox/ before editing (see samples/README.md).")


if __name__ == "__main__":
    main()
