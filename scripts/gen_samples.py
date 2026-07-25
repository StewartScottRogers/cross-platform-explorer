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
    "text/hello.py": 'def main() -> None:\n    print("baseline sample")\n\n\nif __name__ == "__main__":\n    main()\n',
}


def main() -> None:
    print(f"Generating pristine samples under {SAMPLES} …")
    write("audio/track.mp3", make_mp3())
    write("audio/track.flac", make_flac())
    write("audio/track.ogg", make_ogg())
    write("images/photo.jpg", make_jpeg_exif())
    write("images/pixel.png", make_png())
    write("documents/doc.pdf", make_pdf())
    write("video/clip.mp4", make_mp4())
    for rel, txt in TEXT_FILES.items():
        write(rel, txt.encode("utf-8"))
    print("Done. Remember: samples/ is PRISTINE — copy to .sandbox/ before editing (see samples/README.md).")


if __name__ == "__main__":
    main()
