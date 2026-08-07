import { describe, it, expect } from "vitest";
import {
  Playlist,
  shuffledOrder,
  isMediaEntry,
  buildMediaPlaylist,
  mediaQuickLookAction,
} from "./mediaQuickLook";
import type { DirEntry } from "./types";

/** Minimal DirEntry factory — only the fields `categoryOf` + the playlist builder read. */
function entry(name: string, is_dir = false): DirEntry {
  const dot = name.lastIndexOf(".");
  const extension = dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
  return { name, path: `/dir/${name}`, is_dir, size: 0, modified: null, extension, hidden: false } as DirEntry;
}

describe("Playlist — mirrors the Rust playlist::Playlist (CPE-943)", () => {
  const pl = (n: number) => new Playlist(Array.from({ length: n }, (_, i) => `t${i}`));

  it("repeat off stops hard at both ends (no wrap)", () => {
    const p = pl(3);
    expect(p.current()).toBe("t0");
    expect(p.next()).toBe("t1");
    expect(p.next()).toBe("t2");
    expect(p.next()).toBeUndefined(); // hard stop
    expect(p.current()).toBe("t2"); // cursor unmoved past the end
    expect(p.prev()).toBe("t1");
    p.select(0);
    expect(p.prev()).toBeUndefined();
  });

  it("repeat all wraps around both ways", () => {
    const p = pl(3);
    p.setRepeat("all");
    expect(p.next()).toBe("t1");
    expect(p.next()).toBe("t2");
    expect(p.next()).toBe("t0"); // wrap forward
    expect(p.prev()).toBe("t2"); // wrap back
  });

  it("repeat one stays on the current track", () => {
    const p = pl(3);
    p.select(1);
    p.setRepeat("one");
    expect(p.next()).toBe("t1");
    expect(p.next()).toBe("t1");
    expect(p.prev()).toBe("t1");
  });

  it("cycleRepeat steps off → all → one → off", () => {
    const p = pl(2);
    expect(p.repeat).toBe("off");
    p.cycleRepeat(); expect(p.repeat).toBe("all");
    p.cycleRepeat(); expect(p.repeat).toBe("one");
    p.cycleRepeat(); expect(p.repeat).toBe("off");
  });

  it("shuffle is a permutation that keeps the current track under the cursor", () => {
    const p = pl(6);
    p.select(3);
    expect(p.current()).toBe("t3");
    p.setShuffle(true, 42);
    expect(p.isShuffled).toBe(true);
    expect(p.current()).toBe("t3"); // cursor followed the track
    // Every track still reachable exactly once when wrapping.
    p.setRepeat("all");
    const seen = new Set<string>([p.current()!]);
    for (let i = 0; i < 5; i++) seen.add(p.next()!);
    expect(seen.size).toBe(6);
    // Turning shuffle off keeps whatever track is current now.
    p.select(2);
    const cur = p.current();
    p.setShuffle(false, 0);
    expect(p.current()).toBe(cur);
    expect(p.isShuffled).toBe(false);
  });

  it("an empty playlist is safe", () => {
    const p = new Playlist<string>([]);
    expect(p.isEmpty).toBe(true);
    expect(p.current()).toBeUndefined();
    expect(p.next()).toBeUndefined();
    expect(p.prev()).toBeUndefined();
  });
});

describe("shuffledOrder", () => {
  it("is a deterministic permutation of 0..n for a given seed", () => {
    const a = shuffledOrder(10, 123);
    const b = shuffledOrder(10, 123);
    expect(a).toEqual(b); // deterministic
    expect([...a].sort((x, y) => x - y)).toEqual(Array.from({ length: 10 }, (_, i) => i)); // permutation
    // A different seed generally yields a different order (guards against a no-op shuffle).
    expect(shuffledOrder(10, 999)).not.toEqual(a);
  });
});

describe("isMediaEntry", () => {
  it("accepts audio + video files, rejects folders and non-media", () => {
    expect(isMediaEntry(entry("song.mp3"))).toBe(true);
    expect(isMediaEntry(entry("clip.mp4"))).toBe(true);
    expect(isMediaEntry(entry("photo.png"))).toBe(false);
    expect(isMediaEntry(entry("notes.txt"))).toBe(false);
    expect(isMediaEntry(entry("Music", true))).toBe(false);
  });
});

describe("buildMediaPlaylist", () => {
  const listing = [
    entry("Album", true),
    entry("a.mp3"),
    entry("readme.txt"),
    entry("b.mp4"),
    entry("c.wav"),
    entry("cover.png"),
  ];

  it("builds the folder's media playlist (dirs + non-media filtered out), positioned on the selection", () => {
    const p = buildMediaPlaylist(listing, "/dir/b.mp4");
    expect(p).not.toBeNull();
    expect(p!.length).toBe(3); // a.mp3, b.mp4, c.wav — not the folder, txt, or png
    const cur = p!.current()!;
    expect(cur.path).toBe("/dir/b.mp4");
    expect(cur.type).toBe("video");
    expect(p!.next()!.path).toBe("/dir/c.wav"); // steps in listing order
  });

  it("returns null when the selection is not a media file (Space must fall through)", () => {
    expect(buildMediaPlaylist(listing, "/dir/readme.txt")).toBeNull();
    expect(buildMediaPlaylist(listing, "/dir/cover.png")).toBeNull();
    expect(buildMediaPlaylist(listing, "/dir/Album")).toBeNull();
  });
});

describe("mediaQuickLookAction — keyboard mapping", () => {
  const k = (key: string, mods: Partial<KeyboardEvent> = {}) =>
    ({ key, ctrlKey: false, metaKey: false, altKey: false, ...mods }) as KeyboardEvent;

  it("Space and Esc close; arrows step; everything else is ignored", () => {
    expect(mediaQuickLookAction(k(" "))).toBe("close");
    expect(mediaQuickLookAction(k("Escape"))).toBe("close");
    expect(mediaQuickLookAction(k("ArrowRight"))).toBe("next");
    expect(mediaQuickLookAction(k("ArrowLeft"))).toBe("prev");
    expect(mediaQuickLookAction(k("a"))).toBeNull();
  });

  it("ignores modifier-chorded keys so app shortcuts still work underneath", () => {
    expect(mediaQuickLookAction(k(" ", { ctrlKey: true }))).toBeNull();
    expect(mediaQuickLookAction(k("ArrowRight", { altKey: true }))).toBeNull();
    expect(mediaQuickLookAction(k("ArrowLeft", { metaKey: true }))).toBeNull();
  });
});
