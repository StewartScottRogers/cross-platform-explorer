/**
 * Pure helpers for the Instant Search overlay (CPE-1139, epic CPE-703) — kept out of the Svelte
 * component so the keyboard-nav math and the build-root/volume-id derivation are unit-testable without
 * mounting anything. The overlay itself streams `index_search` / `index_build` (CPE-1137) over a
 * `Channel`; this module has no backend dependency at all.
 */
import { driveRoot } from "./driveScheduler";

/**
 * Wrap `active` by `delta` within `[0, length)`; returns 0 when the list is empty. Mirrors
 * `CommandPalette`'s keyboard-nav math (CPE-602) so every keyboard-first overlay in the app behaves
 * identically for ↑/↓.
 */
export function moveActive(active: number, delta: number, length: number): number {
  if (length <= 0) return 0;
  return (active + delta + length) % length;
}

/**
 * Deterministic 32-bit FNV-1a hash of `root` (case-folded so `d:\` and `D:\` collide), used as the
 * `volume_id` `index_build` requires. `index_build`'s `volume_id` is caller-chosen — the backend has no
 * path→id mapping of its own (CPE-1137) — so hashing the root keeps re-building the *same* drive stable
 * across sessions: it replaces the prior resident/on-disk index for that drive instead of leaking a new
 * id (and therefore a new `<id>.idx` file) every time the user clicks "Build index".
 */
export function volumeIdForRoot(root: string): number {
  const upper = root.toUpperCase();
  let hash = 0x811c9dc5;
  for (let i = 0; i < upper.length; i++) {
    hash ^= upper.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0; // fold to an unsigned 32-bit int — always a valid (small, non-negative) u64
}

/**
 * The root to hand `index_build`: the whole drive that owns `currentPath` (an instant *cross-folder*
 * index only makes sense over a whole volume, not one folder), falling back to `homeDir` when there is
 * no current folder (the overlay can open from the Home screen). Empty when neither is known — the
 * caller shows an "open a folder first" state rather than calling `index_build("")`.
 */
export function resolveBuildRoot(currentPath: string, homeDir: string | null): string {
  if (currentPath) return driveRoot(currentPath);
  if (homeDir) return driveRoot(homeDir);
  return "";
}

/**
 * The lowercased extension (without the dot) of a bare file `name`, matching `DirEntry.extension`'s
 * format (`bindings.gen.ts`) — empty for a directory-shaped or extensionless/dotfile name. `IndexHit`
 * (an `index_search` result) carries only `name`, not a pre-split extension, so the result row derives
 * one client-side to hand `iconFor` (`filetypes.ts`) for a type-specific glyph instead of a generic one.
 */
export function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  if (dot <= 0 || dot === name.length - 1) return ""; // no dot, a leading dotfile, or a trailing dot
  return name.slice(dot + 1).toLowerCase();
}
