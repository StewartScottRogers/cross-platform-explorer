import { describe, it, expect, vi } from "vitest";
import { pickProvider, mediaType, visibleActions, providers, type PreviewAction, type PreviewActionCtx, type PreviewProvider } from "./provider";
import type { DirEntry } from "../types";

const entry = (over: Partial<DirEntry>): DirEntry => ({
  name: "x",
  path: "/x",
  is_dir: false,
  size: 1,
  modified: 0,
  extension: "",
  hidden: false,
  is_symlink: false,
  ...over,
});

describe("pickProvider", () => {
  it("picks the image provider for image files", () => {
    expect(pickProvider(entry({ name: "a.png", extension: "png" })).kind).toBe("image");
    expect(pickProvider(entry({ name: "a.jpg", extension: "jpg" })).kind).toBe("image");
  });

  it("picks markdown before text for .md files", () => {
    expect(pickProvider(entry({ name: "readme.md", extension: "md" })).kind).toBe("markdown");
    expect(pickProvider(entry({ name: "notes.markdown", extension: "markdown" })).kind).toBe("markdown");
  });

  it("picks the text provider for text and code files", () => {
    expect(pickProvider(entry({ name: "a.txt", extension: "txt" })).kind).toBe("text");
    expect(pickProvider(entry({ name: "a.ts", extension: "ts" })).kind).toBe("text");
    expect(pickProvider(entry({ name: "a.css", extension: "css" })).kind).toBe("text");
  });

  it("picks the media provider for audio/video and pdf by category (CPE-059/1429)", () => {
    expect(pickProvider(entry({ name: "a.mp3", extension: "mp3" })).kind).toBe("media");
    expect(pickProvider(entry({ name: "a.mp4", extension: "mp4" })).kind).toBe("media");
    expect(pickProvider(entry({ name: "a.pdf", extension: "pdf" })).kind).toBe("pdf");
  });

  it("picks json/csv before the generic text provider", () => {
    expect(pickProvider(entry({ name: "a.json", extension: "json" })).kind).toBe("json");
    expect(pickProvider(entry({ name: "a.csv", extension: "csv" })).kind).toBe("csv");
  });

  it("picks the archive provider for .zip (CPE-064)", () => {
    expect(pickProvider(entry({ name: "a.zip", extension: "zip" })).kind).toBe("archive");
  });

  it("picks the archive provider for zip-family/tar/gzip (CPE-109/112/217)", () => {
    for (const ext of ["jar", "apk", "war", "ipa", "xpi", "tar", "tgz", "gz"]) {
      expect(pickProvider(entry({ name: `a.${ext}`, extension: ext })).kind).toBe("archive");
    }
  });

  it("picks the tsv provider and marks it editable (CPE-083)", () => {
    const p = pickProvider(entry({ name: "a.tsv", extension: "tsv" }));
    expect(p.kind).toBe("tsv");
    expect(p.editable).toBe(true);
  });

  it("marks text-based kinds editable and binary/media kinds not (CPE-067)", () => {
    expect(pickProvider(entry({ name: "a.txt", extension: "txt" })).editable).toBe(true);
    expect(pickProvider(entry({ name: "a.md", extension: "md" })).editable).toBe(true);
    expect(pickProvider(entry({ name: "a.json", extension: "json" })).editable).toBe(true);
    expect(pickProvider(entry({ name: "a.csv", extension: "csv" })).editable).toBe(true);
    expect(pickProvider(entry({ name: "a.png", extension: "png" })).editable).toBe(false);
    expect(pickProvider(entry({ name: "a.mp3", extension: "mp3" })).editable).toBe(false);
    expect(pickProvider(entry({ name: "a.zip", extension: "zip" })).editable).toBe(false);
    expect(pickProvider(entry({ name: "dir", is_dir: true })).editable).toBe(false);
  });

  it("renders bitmap/vector/animated images via the image provider (CPE-095/096/098/100/103)", () => {
    for (const ext of ["gif", "svg", "avif", "ico", "bmp"]) {
      expect(pickProvider(entry({ name: `a.${ext}`, extension: ext })).kind).toBe("image");
    }
  });

  it("plays WAV/FLAC audio and MKV/MOV video via the media provider (CPE-104/105/107/108/1429)", () => {
    for (const ext of ["wav", "flac", "mkv", "mov"]) {
      expect(pickProvider(entry({ name: `a.${ext}`, extension: ext })).kind).toBe("media");
    }
  });

  it("routes the media provider ahead of the generic text/hex handlers (CPE-1429)", () => {
    // Every listed audio/video extension resolves to `media`, not text or the hex last-resort.
    const audio = ["mp3", "wav", "ogg", "flac", "m4a", "aac", "opus"];
    const video = ["mp4", "webm", "mov"];
    for (const ext of [...audio, ...video]) {
      expect(pickProvider(entry({ name: `clip.${ext}`, extension: ext })).kind).toBe("media");
    }
  });

  it("routes .ogg to audio and .mp4 to video via mediaType (CPE-1429)", () => {
    expect(mediaType(entry({ name: "a.ogg", extension: "ogg" }))).toBe("audio");
    expect(mediaType(entry({ name: "a.mp3", extension: "mp3" }))).toBe("audio");
    expect(mediaType(entry({ name: "a.mp4", extension: "mp4" }))).toBe("video");
    expect(mediaType(entry({ name: "a.mov", extension: "mov" }))).toBe("video");
  });

  it("previews HTML and Jupyter notebooks as editable source (CPE-078/114)", () => {
    const html = pickProvider(entry({ name: "a.html", extension: "html" }));
    expect(html.kind).toBe("text");
    expect(html.editable).toBe(true);
    const nb = pickProvider(entry({ name: "a.ipynb", extension: "ipynb" }));
    expect(nb.kind).toBe("text");
    expect(nb.editable).toBe(true);
  });

  it("previews binary formats as read-only info text (CPE-210/214/215/216/218)", () => {
    for (const ext of ["exe", "dll", "wasm", "torrent", "mid", "midi", "bin", "dat"]) {
      const p = pickProvider(entry({ name: `a.${ext}`, extension: ext }));
      expect(p.kind).toBe("info");
      expect(p.editable).toBe(false);
    }
  });

  it("previews office/ebook documents as extracted text (CPE-070/071/072/077)", () => {
    for (const ext of ["rtf", "docx", "odt", "epub"]) {
      expect(pickProvider(entry({ name: `a.${ext}`, extension: ext })).kind).toBe("info");
    }
  });

  it("previews SQLite, spreadsheets and parquet in the interactive data-grid (CPE-849)", () => {
    for (const ext of ["sqlite", "db", "xlsx", "ods", "parquet"]) {
      expect(pickProvider(entry({ name: `a.${ext}`, extension: ext })).kind).toBe("data-grid");
    }
  });

  it("previews 7z and ISO via the archive provider (CPE-110/113)", () => {
    expect(pickProvider(entry({ name: "a.7z", extension: "7z" })).kind).toBe("archive");
    expect(pickProvider(entry({ name: "a.iso", extension: "iso" })).kind).toBe("archive");
  });

  it("previews .rar via the archive provider — lists entries, not a hex dump (CPE-1359)", () => {
    // read_archive_entries dispatches .rar to the pure-Rust rar_entries walker (CPE-1347/1348); the
    // preview provider must route .rar to the archive kind, not fall through to hex/info.
    expect(pickProvider(entry({ name: "a.rar", extension: "rar" })).kind).toBe("archive");
    // Browse-only: RAR is not extractable, but the preview provider only needs the listing kind.
    expect(pickProvider(entry({ name: "a.rar", extension: "rar" })).editable).toBe(false);
  });

  it("routes .eml to the email provider, before the generic text provider (CPE-1434)", () => {
    const p = pickProvider(entry({ name: "message.eml", extension: "eml" }));
    expect(p.kind).toBe("email");
    // Read-only structured card, not editable source.
    expect(p.editable).toBe(false);
    // sanity: an ordinary text file still uses the text provider, not email.
    expect(pickProvider(entry({ name: "a.txt", extension: "txt" })).kind).toBe("text");
  });

  it("routes .ics to the calendar provider, before the generic text provider (CPE-1435)", () => {
    for (const ext of ["ics", "ical"]) {
      const p = pickProvider(entry({ name: `invite.${ext}`, extension: ext }));
      expect(p.kind).toBe("calendar");
      expect(p.editable).toBe(false);
    }
    // sanity: an ordinary text file still uses the text provider, not calendar.
    expect(pickProvider(entry({ name: "a.txt", extension: "txt" })).kind).toBe("text");
  });

  it("routes .vcf to the vcard provider, before the generic text provider (CPE-1436)", () => {
    for (const ext of ["vcf", "vcard"]) {
      const p = pickProvider(entry({ name: `contact.${ext}`, extension: ext }));
      expect(p.kind).toBe("vcard");
      expect(p.editable).toBe(false);
    }
    // sanity: an ordinary text file still uses the text provider, not vcard.
    expect(pickProvider(entry({ name: "a.txt", extension: "txt" })).kind).toBe("text");
  });

  it("previews fonts via the font provider (CPE-117)", () => {
    for (const ext of ["ttf", "otf", "woff", "woff2"]) {
      expect(pickProvider(entry({ name: `a.${ext}`, extension: ext })).kind).toBe("font");
    }
  });

  it("decodes TIFF/PSD via the decoded-image provider, beating native image (CPE-099/101)", () => {
    for (const ext of ["tiff", "tif", "psd"]) {
      expect(pickProvider(entry({ name: `a.${ext}`, extension: ext })).kind).toBe("decoded-image");
    }
    // sanity: a native image still uses the plain image provider
    expect(pickProvider(entry({ name: "a.png", extension: "png" })).kind).toBe("image");
  });

  it("extracts camera-RAW embedded previews via the raw-image provider, beating native image (CPE-1349)", () => {
    for (const ext of ["cr2", "nef", "arw"]) {
      const p = pickProvider(entry({ name: `a.${ext}`, extension: ext }));
      expect(p.kind).toBe("raw-image");
      expect(p.editable).toBe(false);
    }
    // sanity: a native image still uses the plain image provider, not raw-image
    expect(pickProvider(entry({ name: "a.png", extension: "png" })).kind).toBe("image");
  });

  it("decodes DICOM (.dcm) files via the dicom provider, not the generic image/hex fallback (CPE-1350)", () => {
    const p = pickProvider(entry({ name: "a.dcm", extension: "dcm" }));
    expect(p.kind).toBe("dicom");
    expect(p.editable).toBe(false);
    // sanity: a native image still uses the plain image provider, not dicom
    expect(pickProvider(entry({ name: "a.png", extension: "png" })).kind).toBe("image");
  });

  it("decodes HEIC/HEIF (heic/heif/hif) via the heic provider, beating native image (CPE-1351)", () => {
    for (const ext of ["heic", "heif", "hif"]) {
      const p = pickProvider(entry({ name: `a.${ext}`, extension: ext }));
      expect(p.kind).toBe("heic");
      expect(p.editable).toBe(false);
    }
    // sanity: a native image still uses the plain image provider, not heic
    expect(pickProvider(entry({ name: "a.png", extension: "png" })).kind).toBe("image");
  });

  it("routes a directory to the folder-peek browser, not the metadata fallback (CPE-1426)", () => {
    const p = pickProvider(entry({ name: "dir", is_dir: true, extension: "" }));
    expect(p.kind).toBe("folder");
    expect(p.editable).toBe(false);
  });

  it("falls back to metadata only when nothing is selected", () => {
    expect(pickProvider(null).kind).toBe("none");
    expect(pickProvider(undefined).kind).toBe("none");
  });

  it("declares two copy actions on the jwt provider, gated on the claims/header values (CPE-1570)", () => {
    const jwt = providers.find((p) => p.id === "jwt")!;
    expect(jwt.actions?.map((a) => a.id)).toEqual(["copy-claims", "copy-header"]);
    expect(jwt.actions?.map((a) => a.labelKey)).toEqual(["pv.action.copyClaims", "pv.action.copyHeader"]);
  });

  it("opens an unrecognised (binary) file type in the hex view (CPE-773)", () => {
    expect(pickProvider(entry({ name: "a.qqq", extension: "qqq" })).kind).toBe("hex");
    expect(pickProvider(entry({ name: "noext", extension: "" })).kind).toBe("hex");
  });

  it("routes single-file compression formats to the info provider, not archive/hex (CPE-1439)", () => {
    // xz/bz2/zst/lz/lzma are categorised "archive" in filetypes.ts but have no decoder wired in
    // (unlike gzip via flate2) and no entry list to browse. They must land on the read-only "compressed
    // file" info summary — never the archive lister (which would error) and never the raw hex fallback.
    for (const ext of ["xz", "bz2", "zst", "lz", "lzma"]) {
      const p = pickProvider(entry({ name: `a.${ext}`, extension: ext }));
      expect(p.kind).toBe("info");
      expect(p.editable).toBe(false);
    }
  });

  it("leaves dmg/cab on the hex fallback — no container reader is wired in (won't-fix, CPE-1439)", () => {
    // Apple disk images and MS cabinet files need a real container reader that doesn't exist in this
    // codebase; building one is out of scope. They keep falling through to the last-resort hex view
    // rather than being routed to a lister/info path that doesn't actually understand them.
    for (const ext of ["dmg", "cab"]) {
      expect(pickProvider(entry({ name: `a.${ext}`, extension: ext })).kind).toBe("hex");
    }
  });
});

// CPE-1570 (epic CPE-1568): the declarative per-provider actions mechanism — filtering/enablement logic,
// independent of any component mounting (the component-level render→run path is covered by
// PreviewPane.jwtActions.test.ts's worked example).
describe("visibleActions (CPE-1570)", () => {
  const baseCtx = (over: Partial<PreviewActionCtx> = {}): PreviewActionCtx => ({
    entry: entry({}),
    values: {},
    copyToClipboard: vi.fn(async () => {}),
    invoke: vi.fn(async () => undefined) as unknown as PreviewActionCtx["invoke"],
    ...over,
  });

  const fakeProvider = (actions: PreviewAction[]): PreviewProvider => ({
    id: "fake",
    label: "Fake",
    kind: "text",
    editable: false,
    canPreview: () => true,
    actions,
  });

  it("shows an action that declares no `enabled` guard unconditionally", () => {
    const provider = fakeProvider([{ id: "a", labelKey: "k.a", icon: "copy", run: () => {} }]);
    expect(visibleActions(provider, baseCtx()).map((a) => a.id)).toEqual(["a"]);
  });

  it("filters out an action whose `enabled(ctx)` returns false", () => {
    const provider = fakeProvider([
      { id: "on", labelKey: "k.on", icon: "copy", enabled: () => true, run: () => {} },
      { id: "off", labelKey: "k.off", icon: "copy", enabled: () => false, run: () => {} },
    ]);
    expect(visibleActions(provider, baseCtx()).map((a) => a.id)).toEqual(["on"]);
  });

  it("re-evaluates `enabled(ctx)` against the values the ctx carries (JWT-style gating)", () => {
    const provider = fakeProvider([
      { id: "copy-x", labelKey: "k.x", icon: "copy", enabled: (ctx) => !!ctx.values["copy-x"], run: () => {} },
    ]);
    expect(visibleActions(provider, baseCtx({ values: {} })).map((a) => a.id)).toEqual([]);
    expect(visibleActions(provider, baseCtx({ values: { "copy-x": "hello" } })).map((a) => a.id)).toEqual([
      "copy-x",
    ]);
  });

  it("returns an empty array for a provider with no declared actions", () => {
    const provider = fakeProvider([]);
    expect(visibleActions(provider, baseCtx())).toEqual([]);
    const { actions: _actions, ...noActionsField } = provider as PreviewProvider & { actions?: unknown };
    expect(visibleActions(noActionsField as PreviewProvider, baseCtx())).toEqual([]);
  });

  it("runs the action's `run(ctx)` with the same ctx it was filtered against", async () => {
    const run = vi.fn();
    const provider = fakeProvider([{ id: "a", labelKey: "k.a", icon: "copy", run }]);
    const ctx = baseCtx({ values: { a: "value" } });
    const [action] = visibleActions(provider, ctx);
    await action.run(ctx);
    expect(run).toHaveBeenCalledWith(ctx);
  });
});
