import { describe, it, expect } from "vitest";
import {
  joinFieldKey,
  splitFieldKey,
  buildMetaEdits,
  stageFieldEdits,
  isFieldDirty,
  revertFieldEdit,
} from "./metaEdits";
import type { MetaField } from "./bindings.gen";

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

describe("stageFieldEdits (CPE-1326 batch ops)", () => {
  const fields: MetaField[] = [
    { group: "id3", key: "Title", value: "Old Title", editable: true },
    { group: "id3", key: "Album Artist", value: "Various Artists", editable: true },
    { group: "id3", key: "Comment", value: "", editable: true },
  ];

  it("'Strip editable metadata' stages every field to empty, which buildMetaEdits turns into `clear`", () => {
    const edited = stageFieldEdits(fields, () => "");
    expect(edited).toEqual({
      [joinFieldKey("id3", "Title")]: "",
      [joinFieldKey("id3", "Album Artist")]: "",
      [joinFieldKey("id3", "Comment")]: "",
    });
    expect(buildMetaEdits(edited)).toEqual([
      { edit: "clear", group: "id3", key: "Title" },
      { edit: "clear", group: "id3", key: "Album Artist" },
      { edit: "clear", group: "id3", key: "Comment" },
    ]);
  });

  it("'Copy from first' stages every field to the primary's own value, unconditionally", () => {
    // Every field is included even though (for the primary itself) the staged value trivially equals
    // its current value — the record is later applied to OTHER files whose current values differ, so
    // there is no "skip if unchanged" short-circuit here (that would silently drop fields that need
    // overwriting on the non-primary targets).
    const edited = stageFieldEdits(fields, (f) => f.value);
    expect(edited).toEqual({
      [joinFieldKey("id3", "Title")]: "Old Title",
      [joinFieldKey("id3", "Album Artist")]: "Various Artists",
      [joinFieldKey("id3", "Comment")]: "",
    });
    expect(buildMetaEdits(edited)).toEqual([
      { edit: "set", group: "id3", key: "Title", value: "Old Title" },
      { edit: "set", group: "id3", key: "Album Artist", value: "Various Artists" },
      { edit: "clear", group: "id3", key: "Comment" },
    ]);
  });

  it("preserves a key containing spaces (Album Artist regression) round-trip through the composite key", () => {
    const edited = stageFieldEdits(fields, (f) => f.value);
    expect(splitFieldKey(joinFieldKey("id3", "Album Artist"))).toEqual({ group: "id3", key: "Album Artist" });
    expect(edited[joinFieldKey("id3", "Album Artist")]).toBe("Various Artists");
  });

  it("returns an empty record for an empty field list", () => {
    expect(stageFieldEdits([], () => "")).toEqual({});
  });
});

describe("isFieldDirty / revertFieldEdit (CPE-1327 per-field revert)", () => {
  const title: MetaField = { group: "id3", key: "Title", value: "Old Title", editable: true };
  const artist: MetaField = { group: "id3", key: "Album Artist", value: "Various Artists", editable: true };

  it("isFieldDirty is false when the field has no pending edit", () => {
    expect(isFieldDirty(title, {})).toBe(false);
    expect(isFieldDirty(title, { [joinFieldKey("id3", "Album Artist")]: "Someone Else" })).toBe(false);
  });

  it("isFieldDirty is true once the field's composite key is staged", () => {
    const edited = { [joinFieldKey("id3", "Title")]: "New Title" };
    expect(isFieldDirty(title, edited)).toBe(true);
  });

  it("revertFieldEdit drops only the target field's edit, leaving other fields' edits untouched", () => {
    const edited = {
      [joinFieldKey("id3", "Title")]: "New Title",
      [joinFieldKey("id3", "Album Artist")]: "New Artist",
    };
    const result = revertFieldEdit(title, edited);
    expect(result).toEqual({ [joinFieldKey("id3", "Album Artist")]: "New Artist" });
    // The other field is still dirty; only Title was reverted.
    expect(isFieldDirty(artist, result)).toBe(true);
    expect(isFieldDirty(title, result)).toBe(false);
    // Original record is untouched (no in-place mutation) — Svelte reactivity needs a fresh reference.
    expect(edited).toEqual({
      [joinFieldKey("id3", "Title")]: "New Title",
      [joinFieldKey("id3", "Album Artist")]: "New Artist",
    });
  });

  it("revertFieldEdit is a no-op (same reference) when the field wasn't dirty", () => {
    const edited = { [joinFieldKey("id3", "Album Artist")]: "New Artist" };
    expect(revertFieldEdit(title, edited)).toBe(edited);
  });

  it("preserves a key containing spaces (Album Artist regression) through revert", () => {
    const edited = {
      [joinFieldKey("id3", "Title")]: "New Title",
      [joinFieldKey("id3", "Album Artist")]: "New Artist",
    };
    const result = revertFieldEdit(artist, edited);
    expect(result).toEqual({ [joinFieldKey("id3", "Title")]: "New Title" });
  });
});
