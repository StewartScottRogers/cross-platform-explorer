/**
 * Unit tests for the Spotlight item feed (CPE-1216, epic CPE-704): the pure source-builder functions
 * (kind tagging + caps) and the streaming file-hit fetcher. Mirrors the repo's established
 * component-test mocking (`InstantSearch.test.ts`) for the streaming part: mock `@tauri-apps/api/core`
 * since both the typed `commands.*` client and the raw `rawInvoke`/`createChannel` seam flow through it.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  SOURCE_CAPS,
  actionSource,
  folderSource,
  recentSource,
  fileSource,
  buildSources,
  streamFileHits,
  highlightByPositions,
  basenameMatchPositions,
  rowHighlightPositions,
} from "./spotlightSources";
import type { Command } from "./commandPalette";
import { createHistory, visit } from "./history";
import type { NameMatch, Place } from "./bindings.gen";
import type { Favorite } from "./types";

interface Deferred {
  args: any;
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
}
let streamCalls: Deferred[] = [];

const invoke = vi.fn((cmd: string, args?: any) => {
  // A real streaming command's own promise doesn't settle until the whole walk finishes — its
  // `onMatch` channel delivers batches separately, while it's still in flight. A deferred (not an
  // immediately-resolved promise) lets tests feed channel batches before the call "finishes".
  if (cmd === "find_files_by_name_stream")
    return new Promise((resolve, reject) => streamCalls.push({ args, resolve, reject }));
  return Promise.reject(new Error(`unexpected command: ${cmd}`));
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

beforeEach(() => {
  invoke.mockClear();
  streamCalls = [];
});

function cmd(id: string, label: string, enabled = true): Command {
  return { id, label, run: () => {}, enabled: () => enabled };
}

describe("spotlightSources — actionSource (CPE-1216)", () => {
  it("keeps only enabled commands' labels, in declaration order", () => {
    const commands = [cmd("a", "Alpha"), cmd("b", "Beta", false), cmd("c", "Gamma")];
    expect(actionSource(commands)).toEqual(["action", ["Alpha", "Gamma"]]);
  });

  it("caps at SOURCE_CAPS.action", () => {
    const commands = Array.from({ length: SOURCE_CAPS.action + 10 }, (_, i) => cmd(`c${i}`, `Cmd ${i}`));
    const [, labels] = actionSource(commands);
    expect(labels).toHaveLength(SOURCE_CAPS.action);
  });
});

describe("spotlightSources — folderSource (CPE-1216)", () => {
  const place = (path: string): Place => ({ name: path, path, kind: "drive" });
  const fav = (path: string, is_dir: boolean): Favorite => ({ path, name: path, is_dir });

  it("tags folder candidates and includes only directory favorites", () => {
    const [kind, paths] = folderSource(
      [place("C:\\"), place("D:\\")],
      [fav("C:\\Docs", true), fav("C:\\notes.txt", false)],
    );
    expect(kind).toBe("folder");
    expect(paths).toEqual(["C:\\", "D:\\", "C:\\Docs"]);
  });

  it("de-duplicates by path and caps at SOURCE_CAPS.folder", () => {
    const places = Array.from({ length: SOURCE_CAPS.folder + 5 }, (_, i) => place(`C:\\p${i}`));
    const favorites = [fav("C:\\p0", true)]; // duplicate of an existing place
    const [, paths] = folderSource(places, favorites);
    expect(paths).toHaveLength(SOURCE_CAPS.folder);
    expect(new Set(paths).size).toBe(paths.length);
  });
});

describe("spotlightSources — recentSource (CPE-1216)", () => {
  it("delegates to history.recentPaths, most-recent first, current excluded", () => {
    let h = createHistory("/a");
    h = visit(h, "/b");
    h = visit(h, "/c");
    expect(recentSource(h)).toEqual(["recent", ["/b", "/a"]]);
  });
});

describe("spotlightSources — fileSource (CPE-1216)", () => {
  const hit = (path: string): NameMatch => ({ path, name: path, is_dir: false });

  it("maps NameMatch hits to their paths, tagged 'file'", () => {
    expect(fileSource([hit("/a.txt"), hit("/b.txt")])).toEqual(["file", ["/a.txt", "/b.txt"]]);
  });

  it("caps at the given (or default) cap", () => {
    const hits = Array.from({ length: 10 }, (_, i) => hit(`/f${i}.txt`));
    expect(fileSource(hits, 3)[1]).toHaveLength(3);
  });
});

describe("spotlightSources — buildSources (CPE-1216)", () => {
  it("drops empty sources and keeps the non-empty ones tagged", () => {
    const sources = buildSources(
      [cmd("a", "Alpha")],
      [],
      [],
      createHistory(), // no history → empty recent
      [],
    );
    expect(sources).toEqual([["action", ["Alpha"]]]);
  });
});

describe("spotlightSources — streamFileHits (CPE-1216)", () => {
  it("is a no-op for a blank root or query", async () => {
    const onBatch = vi.fn();
    expect(await streamFileHits("", "abc", onBatch)).toEqual([]);
    expect(await streamFileHits("/root", "   ", onBatch)).toEqual([]);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("accumulates streamed batches, capped, and reports each growing batch to onBatch", async () => {
    const batches: NameMatch[][] = [];
    const promise = streamFileHits("/root", "abc", (hits) => batches.push(hits), 3);
    await Promise.resolve(); // let the invoke call land
    expect(streamCalls).toHaveLength(1);
    expect(streamCalls[0].args).toEqual(expect.objectContaining({ root: "/root", query: "abc" }));

    const channel = streamCalls[0].args.onMatch;
    channel.onmessage([
      { path: "/root/a.txt", name: "a.txt", is_dir: false },
      { path: "/root/b.txt", name: "b.txt", is_dir: false },
    ]);
    channel.onmessage([
      { path: "/root/c.txt", name: "c.txt", is_dir: false },
      { path: "/root/d.txt", name: "d.txt", is_dir: false }, // beyond the cap of 3 — dropped
    ]);
    streamCalls[0].resolve({ matches: [], dirs_scanned: 5, truncated: false }); // the walk finishes
    const result = await promise;
    expect(result.map((h) => h.path)).toEqual(["/root/a.txt", "/root/b.txt", "/root/c.txt"]);
    expect(batches.at(-1)?.map((h) => h.path)).toEqual(["/root/a.txt", "/root/b.txt", "/root/c.txt"]);
  });

  it("resolves with whatever streamed in before a failure, instead of throwing", async () => {
    invoke.mockImplementationOnce((cmd: string, args: any) => {
      streamCalls.push({ args, resolve: () => {}, reject: () => {} });
      return Promise.reject(new Error("walk failed"));
    });
    const result = await streamFileHits("/root", "abc", () => {});
    expect(result).toEqual([]);
  });
});

describe("spotlightSources — highlightByPositions (CPE-1216)", () => {
  it("returns the whole text unmatched when there are no positions", () => {
    expect(highlightByPositions("readme.md", [])).toEqual([{ text: "readme.md", match: false }]);
  });

  it("splits into matched/unmatched runs at the given character indices", () => {
    // r(0) … m(4) e(5), matching spotlight.rs's own doc example.
    expect(highlightByPositions("readme.md", [0, 4, 5])).toEqual([
      { text: "r", match: true },
      { text: "ead", match: false },
      { text: "me", match: true },
      { text: ".md", match: false },
    ]);
  });

  it("handles a fully-matched string as one run", () => {
    expect(highlightByPositions("abc", [0, 1, 2])).toEqual([{ text: "abc", match: true }]);
  });
});

describe("spotlightSources — basenameMatchPositions (CPE-1223)", () => {
  it("matches the full query fresh within the basename, even when a prefix char could confuse a full-path greedy match", () => {
    // This is the exact PR #529 failure case: a full-path greedy match of "marker" consumes the "m" of
    // "Temp" before ever reaching the filename, leaving only "arker" if you just filter those positions
    // down to the basename. Matching "marker" directly against the basename alone must find the WHOLE
    // word, unaffected by what's in the prefix.
    const text = "C:\\Users\\me\\Temp\\CPE-1045-marker.txt";
    const basenameStart = text.lastIndexOf("\\") + 1;
    const markerStart = text.toLowerCase().indexOf("marker");
    expect(basenameMatchPositions(text, "marker")).toEqual([
      markerStart, markerStart + 1, markerStart + 2, markerStart + 3, markerStart + 4, markerStart + 5,
    ]);
    // Sanity: the whole run really is inside the basename.
    expect(markerStart).toBeGreaterThanOrEqual(basenameStart);
  });

  it("still finds every run of a scattered in-filename match (rdme -> README.md)", () => {
    const text = "/home/dev/project/README.md";
    const basenameStart = text.lastIndexOf("/") + 1;
    // r(0) d(3) m(4) e(5) relative to the basename "README.md".
    expect(basenameMatchPositions(text, "rdme")).toEqual([
      basenameStart + 0, basenameStart + 3, basenameStart + 4, basenameStart + 5,
    ]);
  });

  it("is case-insensitive, mirroring fuzzy_score", () => {
    expect(basenameMatchPositions("/tmp/README.md", "rEaDme")).toEqual([5, 6, 7, 8, 9, 10]);
  });

  it("is a no-op path-wise for text with no separator (action labels etc.)", () => {
    expect(basenameMatchPositions("Rename", "re")).toEqual([0, 1]);
  });

  it("returns null when the query cannot fully match within the basename alone", () => {
    // "temp" only exists in the prefix, not in the filename "marker.txt".
    expect(basenameMatchPositions("/tmp/Temp/marker.txt", "temp")).toBeNull();
  });

  it("returns null for an empty/blank query", () => {
    expect(basenameMatchPositions("/tmp/marker.txt", "")).toBeNull();
    expect(basenameMatchPositions("/tmp/marker.txt", "   ")).toBeNull();
  });
});

describe("spotlightSources — rowHighlightPositions (CPE-1223)", () => {
  it("prefers the basename-direct match over the backend's raw full-path positions", () => {
    const text = "C:\\Users\\me\\Temp\\marker.txt";
    const basenameStart = text.lastIndexOf("\\") + 1;
    // Backend positions (full-path greedy match): stray "m" in "Temp" (index 14) + "arker" in the
    // basename — the #529 partial-highlight shape.
    const backendPositions = [14, basenameStart + 1, basenameStart + 2, basenameStart + 3, basenameStart + 4, basenameStart + 5];
    expect(rowHighlightPositions(text, backendPositions, "marker")).toEqual([
      basenameStart, basenameStart + 1, basenameStart + 2, basenameStart + 3, basenameStart + 4, basenameStart + 5,
    ]);
  });

  it("falls back to the backend positions unchanged when there's nothing to highlight", () => {
    expect(rowHighlightPositions("/tmp/marker.txt", [], "marker")).toEqual([]);
  });

  it("falls back to the backend positions when the basename-direct match can't fully match the query", () => {
    const backendPositions = [1, 2, 3, 4];
    expect(rowHighlightPositions("/tmp/Temp/marker.txt", backendPositions, "temp")).toBe(backendPositions);
  });
});
