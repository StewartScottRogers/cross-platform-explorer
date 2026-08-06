// CPE-1358 — sample-coverage ratchet: every preview KIND the app claims to support
// (`src/lib/preview/provider.ts`) must have at least one real sample under `samples/`. This is the
// cheap, headless half of the sample-navigation QA program (the heavier end-to-end half is
// `gui-smoke/specs/samples.smoke.ts`, which actually opens each sample on a real built app).
//
// This test computes the ACTUAL preview kind for every file under `samples/` using the real production
// `pickProvider` code path (not a hand-maintained duplicate mapping), so it can never silently drift
// from what the app really does — e.g. it correctly reports that `other/blob.pak` (a generic binary
// under an unrecognised extension) resolves to the "hex" catch-all kind, matching the app's real
// behaviour rather than what a human might assume from the file's extension. (`archives/sample.rar`
// resolves to "archive" as of CPE-1359, which added `rar` to `provider.ts`'s `ARCHIVE_EXT`.)
//
// A new preview kind added to `provider.ts` without a matching sample fails THIS test — the ratchet.
import { describe, it, expect } from "vitest";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { pickProvider, providers } from "./preview/provider";
import type { PreviewKind } from "./preview/provider";
import type { DirEntry } from "./types";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// src/lib/ -> repo root is two levels up.
const SAMPLES_DIR = path.resolve(__dirname, "..", "..", "samples");

/** Every preview kind that can actually be picked for a FILE (not a folder) — derived from the real
 *  registered `providers` array (`provider.ts`), not a hand-maintained duplicate, so a new provider
 *  automatically becomes a coverage requirement here with no second list to update. `FALLBACK`'s "none"
 *  kind is deliberately absent from `providers` (it's returned separately by `pickProvider` only for
 *  folders/null entries — no real FILE ever resolves to it, since the "hex" provider's
 *  `canPreview: (e) => !e.is_dir` already claims every remaining file), so it's correctly never required
 *  here either. */
const REQUIRED_KINDS: PreviewKind[] = [...new Set(providers.map((p) => p.kind))];

/** Builds a minimal `DirEntry`-shaped object for a file, matching `provider.test.ts`'s own helper —
 *  `pickProvider` only reads `is_dir`/`extension`/`name` off it. */
function fileEntry(relPath: string): DirEntry {
  const name = path.basename(relPath);
  const dot = name.lastIndexOf(".");
  const extension = dot > 0 && dot < name.length - 1 ? name.slice(dot + 1).toLowerCase() : "";
  return {
    name,
    path: relPath,
    is_dir: false,
    size: 1,
    modified: 0,
    extension,
    hidden: false,
    is_symlink: false,
  };
}

/** Recursively lists every regular file under `dir`, as paths relative to `SAMPLES_DIR` (posix-style
 *  separators so the output reads the same on every OS). Skips `README.md` — documentation, not a
 *  fixture. */
function listSampleFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const abs = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...listSampleFiles(abs));
    } else if (entry.isFile() && entry.name !== "README.md") {
      out.push(path.relative(SAMPLES_DIR, abs).split(path.sep).join("/"));
    }
  }
  return out;
}

describe("sample-coverage ratchet (CPE-1358)", () => {
  it("samples/ exists and is non-empty", () => {
    expect(fs.existsSync(SAMPLES_DIR), `expected ${SAMPLES_DIR} to exist`).toBe(true);
    const files = listSampleFiles(SAMPLES_DIR);
    expect(files.length, "expected at least one file under samples/").toBeGreaterThan(0);
  });

  it("every supported preview kind has at least one real sample", () => {
    const files = listSampleFiles(SAMPLES_DIR);
    const byKind = new Map<PreviewKind, string[]>();
    for (const rel of files) {
      const kind = pickProvider(fileEntry(rel)).kind;
      const list = byKind.get(kind) ?? [];
      list.push(rel);
      byKind.set(kind, list);
    }

    const missing = REQUIRED_KINDS.filter((kind) => !byKind.get(kind)?.length);
    expect(
      missing,
      `no sample under samples/ resolves to preview kind(s): ${missing.join(", ")}. ` +
        `Coverage found: ${JSON.stringify(Object.fromEntries(byKind), null, 2)}`,
    ).toEqual([]);
  });

  it("every sample file is non-empty (a real, openable fixture, not a placeholder)", () => {
    for (const rel of listSampleFiles(SAMPLES_DIR)) {
      const size = fs.statSync(path.join(SAMPLES_DIR, rel)).size;
      expect(size, `${rel} is empty`).toBeGreaterThan(0);
    }
  });

  it("documents/malformed.pdf exists — the deliberate CPE-1357 crash-regression fixture", () => {
    // Kept separate from the now-valid documents/doc.pdf (see samples/README.md's "PDF fixtures"
    // section): the ORIGINAL degenerate (0-page, no-xref) bytes that crashed the PDF preview, preserved
    // as its own file so gui-smoke can still reproduce the regression without doc.pdf itself being
    // invalid again.
    expect(fs.existsSync(path.join(SAMPLES_DIR, "documents", "malformed.pdf"))).toBe(true);
  });
});
