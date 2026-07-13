/**
 * Preview backend loaders (CPE-234). Extracted so both the main window and the
 * torn-off floating preview window (FloatPreview) render previews the same way.
 * Each is a thin wrapper over a Rust command; they work in any window.
 */
import { invoke } from "@tauri-apps/api/core";
import type { ArchiveEntry } from "./provider";

/** Cap on how much of a text file the preview will load. */
export const PREVIEW_MAX_BYTES = 256 * 1024;

export const loadPreviewText = (path: string): Promise<string> =>
  invoke<string>("read_file_text", { path, maxBytes: PREVIEW_MAX_BYTES });

export const loadArchiveEntries = (path: string): Promise<ArchiveEntry[]> =>
  invoke<ArchiveEntry[]>("read_archive_entries", { path });

export const loadPreviewInfo = (path: string): Promise<string> =>
  invoke<string>("read_preview_info", { path });

export const loadImageData = (path: string): Promise<string> =>
  invoke<string>("read_image_data_url", { path });

export const savePreviewText = (path: string, contents: string): Promise<void> =>
  invoke("write_file_text", { path, contents });
