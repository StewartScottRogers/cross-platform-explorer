import { describe, it, expect } from "vitest";
import { joinFieldKey, splitFieldKey, buildMetaEdits } from "./metaEdits";

describe("metaEdits", () => {
  it("round-trips a key containing spaces (the Album Artist regression)", () => {
    // "Album Artist" and "Date Created" are real friendly keys the codecs emit — a space separator would
    // corrupt them. join → split must return the key intact.
    for (const [group, key] of [
      ["id3", "Album Artist"],
      ["vorbis", "Album Artist"],
      ["pdf", "Date Created"],
      ["pdf", "Date Modified"],
      ["id3", "Title"],
    ] as const) {
      expect(splitFieldKey(joinFieldKey(group, key))).toEqual({ group, key });
    }
  });

  it("buildMetaEdits emits a `set` for a space-bearing key without dropping the tail", () => {
    const edited = { [joinFieldKey("id3", "Album Artist")]: "Various Artists" };
    expect(buildMetaEdits(edited)).toEqual([
      { edit: "set", group: "id3", key: "Album Artist", value: "Various Artists" },
    ]);
  });

  it("treats an empty or whitespace-only value as a clear", () => {
    const edited = {
      [joinFieldKey("id3", "Comment")]: "",
      [joinFieldKey("id3", "Title")]: "   ",
      [joinFieldKey("id3", "Artist")]: "Boards of Canada",
    };
    expect(buildMetaEdits(edited)).toEqual([
      { edit: "clear", group: "id3", key: "Comment" },
      { edit: "clear", group: "id3", key: "Title" },
      { edit: "set", group: "id3", key: "Artist", value: "Boards of Canada" },
    ]);
  });

  it("handles an empty edit set", () => {
    expect(buildMetaEdits({})).toEqual([]);
  });
});
