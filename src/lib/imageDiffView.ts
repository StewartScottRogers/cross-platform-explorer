// Pure helpers for the image-compare pane (CPE-1508, epic CPE-722, parent CPE-1490). Kept
// framework-agnostic + unit-tested, mirroring how `treeDiff.ts`/`byteDiff.ts` split their pure logic out
// of the compare dialog component — `ImageCompareView.svelte` is the only importer of these.

import { bytesToBase64 } from "./terminalClient";

/**
 * Wire-format decision (CPE-1508 ticket Work Log): `diff_images` returns `maskPng` as raw PNG bytes
 * (`number[]`) over the typed/serde command boundary — the same shape as every other byte-array field
 * that crosses IPC (see `read_file_range`'s `number[]`), rather than a bespoke pre-base64'd string just
 * for this one field. Base64-encoding a thumbnail-scale mask client-side is cheap (a handful of KB), so
 * this reuses the exact chunked byte→base64 routine `terminalClient.ts` already uses for PTY output
 * (chunked so a large mask can't blow the call stack via `String.fromCharCode(...bytes)` spreading the
 * whole array at once) rather than asking the backend to change its convention for one caller.
 */
export function maskPngToDataUrl(maskPng: number[]): string {
  return `data:image/png;base64,${bytesToBase64(Uint8Array.from(maskPng))}`;
}

export interface DiffBBoxLike {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface RectPercent {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * `bbox`'s position/size as CSS percentages of the union canvas (`ImageDiff.width`/`height`) — for
 * drawing a highlight rectangle over an `<img>`/onion stack that's sized to fill its container
 * (`width:100%; height:100%`), so the rectangle lines up regardless of the container's actual pixel size.
 * `null` when there's no bbox (nothing changed) or the canvas has zero extent.
 */
export function bboxRectPercent(
  bbox: DiffBBoxLike | null | undefined,
  canvasWidth: number,
  canvasHeight: number,
): RectPercent | null {
  if (!bbox || canvasWidth <= 0 || canvasHeight <= 0) return null;
  return {
    left: (bbox.x / canvasWidth) * 100,
    top: (bbox.y / canvasHeight) * 100,
    width: (bbox.width / canvasWidth) * 100,
    height: (bbox.height / canvasHeight) * 100,
  };
}

export interface ZoomPan {
  zoom: number;
  panX: number;
  panY: number;
}

/**
 * Zoom/pan state that brings `bbox` centered into view inside a `viewportWidth` x `viewportHeight`
 * viewport, with `padding` slack around it (1.4 ⇒ the bbox occupies ~71% of the shorter viewport edge).
 * Pure (no DOM), so it's unit-testable: apply as
 * `transform: scale(zoom) translate(panX px, panY px); transform-origin: 0 0` — translate is expressed in
 * PRE-scale (image-space) px because CSS `transform` applies the rightmost function first. Clamps zoom to
 * `[1, 8]` so "zoom to region" never shrinks below the image's natural fit or explodes into a useless
 * pixel mush on a tiny bbox.
 */
export function zoomToBBox(
  bbox: DiffBBoxLike,
  canvasWidth: number,
  canvasHeight: number,
  viewportWidth: number,
  viewportHeight: number,
  padding = 1.4,
): ZoomPan {
  if (bbox.width <= 0 || bbox.height <= 0 || canvasWidth <= 0 || canvasHeight <= 0 || viewportWidth <= 0 || viewportHeight <= 0) {
    return { zoom: 1, panX: 0, panY: 0 };
  }
  const fitZoom = Math.min(
    viewportWidth / (bbox.width * padding),
    viewportHeight / (bbox.height * padding),
  );
  const zoom = Math.max(1, Math.min(8, fitZoom));
  const cx = bbox.x + bbox.width / 2;
  const cy = bbox.y + bbox.height / 2;
  const panX = viewportWidth / (2 * zoom) - cx;
  const panY = viewportHeight / (2 * zoom) - cy;
  return { zoom, panX, panY };
}

/** Clamp a proposed zoom level (e.g. from a wheel event) to the same `[1, 8]` range `zoomToBBox` uses,
 *  so manual zoom and "zoom to region" never disagree on bounds. */
export function clampZoom(zoom: number): number {
  return Math.max(1, Math.min(8, zoom));
}

/** Format `percentDifferent` (0-100) for display — one decimal place, e.g. `"3.2%"` / `"0.0%"`. */
export function formatPercentDifferent(percentDifferent: number): string {
  return `${percentDifferent.toFixed(1)}%`;
}
