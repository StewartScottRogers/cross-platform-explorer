/**
 * Spotlight item feed (CPE-1216, epic CPE-704): builds the kind-tagged candidate lists
 * (`[ResultKind, string[]][]`) the `spotlight_search` backend command (CPE-1214) fuzzy-ranks, plus the
 * streaming fetch of file/folder-name hits from the current folder that feeds the "file" source.
 *
 * Pure data-shaping functions (jsdom-testable, no DOM) + one thin streaming fetcher mirroring
 * `FileNameSearchDialog.svelte`'s `find_files_by_name_stream` usage
 * ([[prefer-streaming-liveness]] — a huge folder must not block the overlay's first paint). The
 * Spotlight overlay owns scoring/rendering (`SpotSection`/`SpotResult` from `spotlight_search`); this
 * module only assembles what gets scored.
 */
import type { ResultKind, NameMatch, Place } from "./bindings.gen";
import type { Favorite } from "./types";
import { rawInvoke, createChannel } from "./invoke";
import { isEnabled, type Command } from "./commandPalette";
import { recentPaths, type History } from "./history";

/**
 * Caps on how many candidate strings each source contributes to a single `spotlight_search` call —
 * keeps the IPC payload and the Rust-side fuzzy scan small even when a source could otherwise be huge
 * (the file source in particular; drives/favorites/recents/actions are small in practice already).
 */
export const SOURCE_CAPS: Record<ResultKind, number> = {
  action: 200,
  folder: 100,
  file: 500,
  recent: 20,
};

/** Action candidates: every currently-enabled palette command's label, in declaration order. Disabled
 *  commands (context-invalid right now) are excluded — matching what the Command Palette itself shows. */
export function actionSource(commands: Command[]): [ResultKind, string[]] {
  const labels = commands.filter(isEnabled).map((c) => c.label).slice(0, SOURCE_CAPS.action);
  return ["action", labels];
}

/** Folder candidates: quick-access places (drives + special folders) plus favorited folders,
 *  de-duplicated by path and capped. */
export function folderSource(places: Place[], favorites: Favorite[]): [ResultKind, string[]] {
  const paths = [...places.map((p) => p.path), ...favorites.filter((f) => f.is_dir).map((f) => f.path)];
  const seen = new Set<string>();
  const out: string[] = [];
  for (const p of paths) {
    if (seen.has(p)) continue;
    seen.add(p);
    out.push(p);
    if (out.length >= SOURCE_CAPS.folder) break;
  }
  return ["folder", out];
}

/** Recent-location candidates from the active tab's navigation history — the same list the Command
 *  Palette's "Recent" group jumps to (`recentPaths`, history.ts). */
export function recentSource(history: History, max = SOURCE_CAPS.recent): [ResultKind, string[]] {
  return ["recent", recentPaths(history, max)];
}

/** File candidates: paths already fetched via `streamFileHits` below, capped. Kept separate from the
 *  fetcher so it stays a trivial, synchronous, pure transform. */
export function fileSource(hits: NameMatch[], cap = SOURCE_CAPS.file): [ResultKind, string[]] {
  return ["file", hits.slice(0, cap).map((h) => h.path)];
}

/**
 * Assemble the full `sources` array `spotlightSearch` expects from every feed, dropping any source
 * that came back empty (an empty candidate list is a no-op for the backend's `aggregate`, but skipping
 * it keeps the request small).
 */
export function buildSources(
  paletteCommands: Command[],
  places: Place[],
  favorites: Favorite[],
  history: History,
  fileHits: NameMatch[],
): [ResultKind, string[]][] {
  return [
    actionSource(paletteCommands),
    folderSource(places, favorites),
    recentSource(history),
    fileSource(fileHits),
  ].filter(([, items]) => items.length > 0);
}

/**
 * Stream file/folder-name hits from `root` for `query` (CPE-666's engine): pushes progressively
 * growing hit lists to `onBatch` as the recursive walk streams matches over an IPC `Channel`
 * ([[prefer-streaming-liveness]] — mirrors `FileNameSearchDialog.svelte`), so the overlay's file
 * section fills in live instead of waiting for the whole folder tree to finish scanning. Capped at
 * `cap` matches. A blank query or root is a no-op ([]); a failed walk resolves with whatever streamed
 * in before the error rather than throwing (best-effort — an overlay's file section going stale is
 * better than the whole overlay erroring out).
 */
export async function streamFileHits(
  root: string,
  query: string,
  onBatch: (hits: NameMatch[]) => void,
  cap = SOURCE_CAPS.file,
): Promise<NameMatch[]> {
  const q = query.trim();
  if (!root || !q) return [];
  const collected: NameMatch[] = [];
  const channel = createChannel<NameMatch[]>();
  channel.onmessage = (batch) => {
    if (collected.length >= cap) return;
    const room = cap - collected.length;
    collected.push(...batch.slice(0, room));
    onBatch(collected.slice());
  };
  try {
    await rawInvoke("find_files_by_name_stream", { root, query: q, onMatch: channel });
  } catch {
    // best-effort — whatever streamed in before the error still stands
  }
  return collected.slice();
}

/** One run of a `SpotResult.text` (rendered highlighted) or plain text, split at the matched `char`
 *  indices the backend fuzzy-matcher reports in `SpotResult.positions`. Unlike
 *  `contentSearch.ts`'s substring-based `highlightSegments`, this highlights arbitrary, possibly
 *  non-contiguous character positions (a subsequence match) — exactly what `spotlight::fuzzy_score`
 *  produces. Pure. An empty `positions` (no fuzzy match — e.g. the frecency default view) yields the
 *  whole text as one unmatched segment. */
export interface HighlightSegment {
  text: string;
  match: boolean;
}
export function highlightByPositions(text: string, positions: number[]): HighlightSegment[] {
  if (positions.length === 0) return [{ text, match: false }];
  const chars = Array.from(text); // char-by-char, matching the Rust side's `chars().enumerate()` indices
  const matchSet = new Set(positions);
  const segments: HighlightSegment[] = [];
  let buf = "";
  let bufMatch = false;
  for (let i = 0; i < chars.length; i++) {
    const isMatch = matchSet.has(i);
    if (buf && isMatch !== bufMatch) {
      segments.push({ text: buf, match: bufMatch });
      buf = "";
    }
    buf += chars[i];
    bufMatch = isMatch;
  }
  if (buf) segments.push({ text: buf, match: bufMatch });
  return segments;
}

/**
 * Restricts fuzzy-match positions to the basename (CPE-1223): `spotlight_search` scores/highlights
 * candidates over the FULL path (`fileSource` feeds `h.path`), so the greedy subsequence matcher can
 * plant a lone highlighted character in the path *prefix* ahead of the real match run in the filename
 * — e.g. querying "marker" faintly highlights the "m" in `.../Temp/...` before the "marker" run in the
 * filename itself. That reads as noise, not an intentional match.
 *
 * Ranking stays exactly as `spotlight_search` returned it (full-path scoring is untouched — this only
 * changes which characters get `<mark>`ed); positions at/after the last path separator (the basename)
 * pass through unchanged, including every run of a genuine multi-character match *within* the filename
 * (e.g. "rdme" → "README" still highlights all its scattered-but-in-filename runs). Positions before
 * the separator (the directory portion) are dropped. A no-op for text with no separator (action labels,
 * bare filenames) — returns `positions` unchanged.
 */
export function basenamePositions(text: string, positions: number[]): number[] {
  if (positions.length === 0) return positions;
  const chars = Array.from(text); // char indices, matching positions' indexing (see highlightByPositions)
  let basenameStart = 0;
  for (let i = 0; i < chars.length; i++) {
    if (chars[i] === "/" || chars[i] === "\\") basenameStart = i + 1;
  }
  if (basenameStart === 0) return positions; // no separator — whole text is already "the basename"
  return positions.filter((p) => p >= basenameStart);
}
