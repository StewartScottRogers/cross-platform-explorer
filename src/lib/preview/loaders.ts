/**
 * Preview backend loaders (CPE-234). Extracted so both the main window and the
 * torn-off floating preview window (FloatPreview) render previews the same way.
 * Each is a thin wrapper over a Rust command; they work in any window.
 */
import { unwrap } from "../invoke";
import { commands } from "../bindings.gen"; // typed client (CPE-964)
import type { ArchiveEntry } from "./provider";

/** Cap on how much of a text file the preview will load. */
export const PREVIEW_MAX_BYTES = 256 * 1024;

export const loadPreviewText = (path: string): Promise<string> =>
  commands.readFileText(path, PREVIEW_MAX_BYTES).then(unwrap);

export const loadArchiveEntries = (path: string): Promise<ArchiveEntry[]> =>
  commands.readArchiveEntries(path).then(unwrap);

export const loadPreviewInfo = (path: string): Promise<string> =>
  commands.readPreviewInfo(path).then(unwrap);

export const loadImageData = (path: string): Promise<string> =>
  commands.readImageDataUrl(path).then(unwrap);

export const loadRawImageData = (path: string): Promise<string> =>
  commands.readRawPreviewDataUrl(path).then(unwrap);

export const loadDicomImageData = (path: string): Promise<string> =>
  commands.readDicomImageDataUrl(path).then(unwrap);

export const loadDicomTags = (path: string): Promise<[string, string][]> =>
  commands.readDicomTags(path).then(unwrap);

export const savePreviewText = (path: string, contents: string): Promise<void> =>
  commands.writeFileText(path, contents).then((r) => void unwrap(r));
