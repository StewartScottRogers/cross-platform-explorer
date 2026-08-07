/**
 * Pure stepping / keyboard core for the full-screen media quick-look (CPE-1430, epic CPE-720).
 *
 * Two framework-free pieces, both trivially unit-testable and holding NO DOM reference:
 *
 *  1. {@link Playlist} — a faithful TypeScript MIRROR of the shipped Rust `playlist::Playlist` (CPE-943):
 *     an ordered list with a cursor, plus `next`/`prev` that honour a **repeat** mode (off/one/all) and an
 *     optional deterministic **shuffle**. The Rust model is Tauri-side only (no ergonomic frontend binding
 *     exists), so rather than round-trip every arrow key to the backend the overlay steps in-process with
 *     this mirror. It reproduces the Rust semantics arm-for-arm (including the same seeded Fisher-Yates
 *     order via the same LCG, computed in BigInt to match u64 wrapping) so behaviour can never drift.
 *
 *  2. {@link mediaQuickLookAction} + {@link buildMediaPlaylist} — the folder→media-playlist projection and
 *     the Space/Esc/arrow key mapping, kept out of the Svelte component so the wiring is testable without a
 *     DOM.
 */
import type { DirEntry } from "./types";
import { categoryOf } from "./filetypes";

/** How playback loops — mirrors the Rust `RepeatMode` (`snake_case` serde, so `"off" | "one" | "all"`). */
export type RepeatMode = "off" | "one" | "all";

/** One playable track in the overlay: the file to load plus how to render it. */
export interface MediaTrack {
  path: string;
  name: string;
  type: "audio" | "video";
}

/**
 * An ordered playlist with a cursor — a structural port of the Rust `Playlist` (CPE-943). Generic over the
 * item type (the Rust model stores `String` paths; the overlay stores full {@link MediaTrack}s), but the
 * navigation semantics are identical: `order` is the play sequence (indices into `items`), the identity
 * until shuffled; `next`/`prev` move the cursor per {@link RepeatMode}; shuffle keeps the current item under
 * the cursor.
 */
export class Playlist<T = string> {
  private items: T[];
  private order: number[];
  private pos = 0;
  private mode: RepeatMode = "off";
  private shuffled = false;

  constructor(items: T[]) {
    this.items = items;
    this.order = items.map((_, i) => i);
  }

  get length(): number {
    return this.items.length;
  }
  get isEmpty(): boolean {
    return this.items.length === 0;
  }
  get repeat(): RepeatMode {
    return this.mode;
  }
  get isShuffled(): boolean {
    return this.shuffled;
  }
  /** 0-based cursor position within the play order (for a "3 / 12" counter). */
  get position(): number {
    return this.pos;
  }

  /** The track at the cursor, or `undefined` for an empty playlist. */
  current(): T | undefined {
    const i = this.order[this.pos];
    return i === undefined ? undefined : this.items[i];
  }

  /** Advance per the repeat mode; returns the new current track (or `undefined` at a hard end with `off`). */
  next(): T | undefined {
    if (this.items.length === 0) return undefined;
    switch (this.mode) {
      case "one":
        break;
      case "off":
        if (this.pos + 1 >= this.order.length) return undefined;
        this.pos += 1;
        break;
      case "all":
        this.pos = (this.pos + 1) % this.order.length;
        break;
    }
    return this.current();
  }

  /** Step back per the repeat mode; returns the new current track (or `undefined` before the start with `off`). */
  prev(): T | undefined {
    if (this.items.length === 0) return undefined;
    switch (this.mode) {
      case "one":
        break;
      case "off":
        if (this.pos === 0) return undefined;
        this.pos -= 1;
        break;
      case "all":
        this.pos = (this.pos + this.order.length - 1) % this.order.length;
        break;
    }
    return this.current();
  }

  setRepeat(mode: RepeatMode): void {
    this.mode = mode;
  }

  /** Cycle repeat off → all → one → off, the order the overlay's repeat button steps through. */
  cycleRepeat(): void {
    this.mode = this.mode === "off" ? "all" : this.mode === "all" ? "one" : "off";
  }

  /** Jump the cursor to the track at `itemIndex` (an index into the original items). No-op if out of range. */
  select(itemIndex: number): void {
    const p = this.order.indexOf(itemIndex);
    if (p >= 0) this.pos = p;
  }

  /**
   * Toggle shuffle. Rebuilds the play order (deterministically from `seed` when on, identity when off) and
   * keeps the CURRENT track under the cursor across the change — same contract as the Rust model.
   */
  setShuffle(on: boolean, seed: number): void {
    const currentItem = this.order[this.pos];
    this.order = on ? shuffledOrder(this.items.length, seed) : this.items.map((_, i) => i);
    this.shuffled = on;
    const p = currentItem === undefined ? -1 : this.order.indexOf(currentItem);
    this.pos = p >= 0 ? p : 0;
  }
}

/**
 * A deterministic Fisher-Yates shuffle of `0..n`, a permutation reproducible for a given seed — a direct
 * port of the Rust `shuffled_order`'s xorshift LCG. Uses BigInt masked to 64 bits so the wrapping matches
 * Rust's `u64` exactly, which keeps the two orderings identical for the same seed.
 */
export function shuffledOrder(n: number, seed: number): number[] {
  const v = Array.from({ length: n }, (_, i) => i);
  const MASK = (1n << 64n) - 1n;
  let state = (BigInt(Math.trunc(seed)) ^ 0x9e3779b97f4a7c15n) & MASK;
  for (let i = n - 1; i >= 1; i--) {
    state = (state ^ ((state << 13n) & MASK)) & MASK;
    state = (state ^ (state >> 7n)) & MASK;
    state = (state ^ ((state << 17n) & MASK)) & MASK;
    const j = Number(state % BigInt(i + 1));
    [v[i], v[j]] = [v[j], v[i]];
  }
  return v;
}

/** True when `entry` is a playable audio/video file — the exact gate the `media` preview provider uses. */
export function isMediaEntry(entry: DirEntry): boolean {
  if (entry.is_dir) return false;
  const cat = categoryOf(entry);
  return cat === "audio" || cat === "video";
}

/**
 * Build the current folder's media playlist from a directory listing, positioned on `selectedPath`.
 * Returns `null` when the selection isn't a media file, or the folder holds no media — the caller treats
 * that as "Space does nothing here" and lets the key fall through to its other bindings.
 */
export function buildMediaPlaylist(entries: DirEntry[], selectedPath: string): Playlist<MediaTrack> | null {
  const media = entries.filter(isMediaEntry);
  const tracks: MediaTrack[] = media.map((e) => ({
    path: e.path,
    name: e.name,
    type: categoryOf(e) === "video" ? "video" : "audio",
  }));
  const index = tracks.findIndex((t) => t.path === selectedPath);
  if (index < 0) return null; // selection isn't media (or not in this listing)
  const pl = new Playlist(tracks);
  pl.select(index);
  return pl;
}

/** What a key does while the media quick-look overlay owns the keyboard. `null` = not one of ours. */
export type MediaQuickLookAction = "close" | "next" | "prev" | null;

/**
 * Map a keyboard event to a quick-look action. Space and Esc close; ←/→ step. Modifier-chorded keys are
 * ignored so app shortcuts (Ctrl/Alt/Meta combos) still work "underneath" the overlay's own keys.
 */
export function mediaQuickLookAction(
  e: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey">,
): MediaQuickLookAction {
  if (e.ctrlKey || e.metaKey || e.altKey) return null;
  switch (e.key) {
    case "Escape":
    case " ":
      return "close";
    case "ArrowRight":
      return "next";
    case "ArrowLeft":
      return "prev";
    default:
      return null;
  }
}
