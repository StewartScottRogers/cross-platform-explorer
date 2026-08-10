import { describe, it, expect } from "vitest";
import {
  ACTIONS,
  defaultKeymap,
  chordFor,
  actionForChord,
  setChord,
  resetChord,
  resetAll,
  listActions,
  findConflicts,
  serializeKeymap,
  parseKeymap,
  normalizeChord,
  chordFromEvent,
  formatChord,
  isActionId,
  exportKeymap,
  importKeymap,
  type Keymap,
  type ActionId,
} from "./keymap";

/** A minimal `KeyboardEvent`-shaped object, matching what `chordFromEvent`/`hotkeyFromEvent`
 *  expect — only the fields they read. */
function keyEvent(
  key: string,
  mods: { ctrl?: boolean; alt?: boolean; shift?: boolean } = {},
): { ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean; key: string } {
  return { ctrlKey: !!mods.ctrl, metaKey: false, altKey: !!mods.alt, shiftKey: !!mods.shift, key };
}

describe("keymap ACTIONS / defaultKeymap (CPE-1547)", () => {
  it("has no duplicate action ids", () => {
    const ids = ACTIONS.map((a) => a.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("defaultKeymap covers every ActionId exactly once", () => {
    const km = defaultKeymap();
    const keys = Object.keys(km);
    expect(keys.length).toBe(ACTIONS.length);
    for (const a of ACTIONS) {
      expect(km).toHaveProperty(a.id);
      expect(km[a.id]).toBe(a.defaultChord);
    }
  });

  it("returns a fresh, independently mutable object each call", () => {
    const a = defaultKeymap();
    const b = defaultKeymap();
    expect(a).toEqual(b);
    expect(a).not.toBe(b);
    a.back = "Ctrl+Alt+B";
    expect(b.back).not.toBe("Ctrl+Alt+B");
  });

  it("every default chord is either '' or normalizes to itself (idempotent canonical form)", () => {
    for (const a of ACTIONS) {
      if (a.defaultChord === "") continue;
      expect(normalizeChord(a.defaultChord)).toBe(a.defaultChord);
    }
  });

  it("isActionId recognizes every registered id and rejects unknown strings", () => {
    for (const a of ACTIONS) expect(isActionId(a.id)).toBe(true);
    expect(isActionId("notAnAction")).toBe(false);
    expect(isActionId(42)).toBe(false);
  });
});

describe("keymap normalizeChord (CPE-1547)", () => {
  it("canonicalizes a Ctrl/Alt-qualified chord identically to normalizeHotkey", () => {
    expect(normalizeChord("ctrl+shift+f")).toBe("Ctrl+Shift+F");
    expect(normalizeChord("Alt+Enter")).toBe("Alt+Enter");
  });

  it("preserves a bare function/navigation key that normalizeHotkey would reject", () => {
    expect(normalizeChord("F5")).toBe("F5");
    expect(normalizeChord("Enter")).toBe("Enter");
    expect(normalizeChord("Esc")).toBe("Esc");
    expect(normalizeChord("Delete")).toBe("Delete");
    expect(normalizeChord("?")).toBe("?");
  });

  it("preserves a Shift-only chord (e.g. Shift+Delete)", () => {
    expect(normalizeChord("Shift+Delete")).toBe("Shift+Delete");
  });

  it("returns '' for a modifier-only or empty input", () => {
    expect(normalizeChord("Ctrl+Alt")).toBe("");
    expect(normalizeChord("")).toBe("");
  });
});

// Regression coverage for a review-caught bug: 4 of 34 defaultChord values were transcribed from
// shortcuts.ts's DISPLAY glyphs ("←"/"→"/"↑"/"Esc") instead of the real `KeyboardEvent.key` form a
// live keypress produces ("ArrowLeft"/"ArrowRight"/"ArrowUp"/"Escape"). Round-tripping the STORED
// string through normalizeChord (as the tests above do) can never catch that class of bug — it
// never involves a live event. This simulates the actual live-capture path (chordFromEvent, the
// permissive analog of macroBindings' hotkeyFromEvent that the future remap UI will use) for a
// representative sample spanning every shape of default: Alt+Arrow-key, a bare Escape, a bare
// function key, Shift+bare-key, and ordinary Ctrl-qualified chords — and asserts the captured
// chord equals the registered default AND resolves back to the right action.
describe("keymap chordFromEvent — live-keypress capture matches registered defaults (CPE-1547)", () => {
  const cases: { id: ActionId; event: ReturnType<typeof keyEvent> }[] = [
    { id: "back", event: keyEvent("ArrowLeft", { alt: true }) },
    { id: "forward", event: keyEvent("ArrowRight", { alt: true }) },
    { id: "up", event: keyEvent("ArrowUp", { alt: true }) },
    { id: "clearSelection", event: keyEvent("Escape") },
    { id: "refresh", event: keyEvent("F5") },
    { id: "openItem", event: keyEvent("Enter") },
    { id: "rename", event: keyEvent("F2") },
    { id: "deleteToTrash", event: keyEvent("Delete") },
    { id: "deletePermanent", event: keyEvent("Delete", { shift: true }) },
    { id: "docsHelp", event: keyEvent("F1") },
    { id: "shortcutsCheatSheet", event: keyEvent("?") },
    { id: "copy", event: keyEvent("c", { ctrl: true }) },
    { id: "contentSearch", event: keyEvent("f", { ctrl: true, shift: true }) },
    { id: "properties", event: keyEvent("Enter", { alt: true }) },
    { id: "toggleDetails", event: keyEvent("p", { alt: true }) },
  ];

  it.each(cases)("$id: a real keypress captures the registered defaultChord", ({ id, event }) => {
    const def = ACTIONS.find((a) => a.id === id)!;
    expect(chordFromEvent(event)).toBe(def.defaultChord);
  });

  it.each(cases)("$id: actionForChord resolves the captured chord back to the action", ({ id, event }) => {
    const km = defaultKeymap();
    expect(actionForChord(km, chordFromEvent(event))).toBe(id);
  });

  it("the 4 review-flagged actions specifically: captured chord is the event.key form, not the display glyph", () => {
    expect(chordFromEvent(keyEvent("ArrowLeft", { alt: true }))).toBe("Alt+ArrowLeft");
    expect(chordFromEvent(keyEvent("ArrowRight", { alt: true }))).toBe("Alt+ArrowRight");
    expect(chordFromEvent(keyEvent("ArrowUp", { alt: true }))).toBe("Alt+ArrowUp");
    expect(chordFromEvent(keyEvent("Escape"))).toBe("Escape");
  });
});

describe("keymap chordFor / actionForChord (CPE-1547)", () => {
  it("chordFor returns the effective (default) chord for every action", () => {
    const km = defaultKeymap();
    for (const a of ACTIONS) expect(chordFor(km, a.id)).toBe(a.defaultChord);
  });

  it("chordFor returns an override when present", () => {
    const km = setChord(defaultKeymap(), "copy", "Ctrl+Alt+C");
    expect(chordFor(km, "copy")).toBe("Ctrl+Alt+C");
  });

  it("actionForChord reverse-looks-up an already-normalized chord", () => {
    const km = defaultKeymap();
    const copyDef = ACTIONS.find((a) => a.id === "copy")!;
    expect(actionForChord(km, copyDef.defaultChord)).toBe("copy");
    expect(actionForChord(km, "Ctrl+Alt+Shift+Q")).toBeUndefined();
  });

  it("actionForChord never matches an empty chord, even though multiple actions may sit at ''", () => {
    const km = defaultKeymap();
    expect(actionForChord(km, "")).toBeUndefined();
  });
});

describe("keymap setChord / resetChord / resetAll (CPE-1547)", () => {
  it("setChord is immutable and normalizes via the strict Ctrl/Alt-required rule", () => {
    const before = defaultKeymap();
    const snapshot = { ...before };
    const after = setChord(before, "paste", "ctrl+alt+v");
    expect(before).toEqual(snapshot); // input untouched
    expect(after).not.toBe(before);
    expect(after.paste).toBe("Ctrl+Alt+V");
  });

  it("setChord stores '' when the raw chord fails strict normalization (e.g. Shift-only)", () => {
    const km = setChord(defaultKeymap(), "paste", "shift+v");
    expect(km.paste).toBe("");
  });

  it("resetChord restores a single action's default, immutably, leaving siblings untouched", () => {
    const overridden = setChord(defaultKeymap(), "copy", "Ctrl+Alt+C");
    const snapshot = { ...overridden };
    const reset = resetChord(overridden, "copy");
    expect(overridden).toEqual(snapshot); // input untouched
    expect(reset).not.toBe(overridden);
    const copyDef = ACTIONS.find((a) => a.id === "copy")!;
    expect(reset.copy).toBe(copyDef.defaultChord);
    // sibling untouched
    const cutDef = ACTIONS.find((a) => a.id === "cut")!;
    expect(reset.cut).toBe(cutDef.defaultChord);
  });

  it("resetAll returns a fresh default keymap", () => {
    expect(resetAll()).toEqual(defaultKeymap());
  });
});

describe("keymap listActions (CPE-1547)", () => {
  it("lists every action in registry order, paired with its effective chord", () => {
    const km = setChord(defaultKeymap(), "up", "Ctrl+Alt+U");
    const listed = listActions(km);
    expect(listed.map((x) => x.action.id)).toEqual(ACTIONS.map((a) => a.id));
    const upEntry = listed.find((x) => x.action.id === "up")!;
    expect(upEntry.chord).toBe("Ctrl+Alt+U");
  });
});

describe("keymap findConflicts (CPE-1547)", () => {
  it("reports no conflicts on a fresh default keymap (every default is unique)", () => {
    expect(findConflicts(defaultKeymap())).toEqual([]);
  });

  it("detects a 2-way collision", () => {
    let km = defaultKeymap();
    km = setChord(km, "copy", "Ctrl+Alt+X");
    km = setChord(km, "cut", "Ctrl+Alt+X");
    const conflicts = findConflicts(km);
    expect(conflicts).toHaveLength(1);
    expect(conflicts[0].chord).toBe("Ctrl+Alt+X");
    expect(conflicts[0].ids.sort()).toEqual(["copy", "cut"].sort());
  });

  it("detects a 3-way collision", () => {
    let km = defaultKeymap();
    km = setChord(km, "copy", "Ctrl+Alt+X");
    km = setChord(km, "cut", "Ctrl+Alt+X");
    km = setChord(km, "paste", "Ctrl+Alt+X");
    const conflicts = findConflicts(km);
    expect(conflicts).toHaveLength(1);
    expect(conflicts[0].ids.sort()).toEqual(["copy", "cut", "paste"].sort());
  });

  it("never treats unbound ('') chords as conflicting, however many actions sit there", () => {
    let km = defaultKeymap();
    km = setChord(km, "copy", "shift+x"); // normalizes to ""
    km = setChord(km, "cut", "shift+y"); // normalizes to ""
    expect(km.copy).toBe("");
    expect(km.cut).toBe("");
    expect(findConflicts(km)).toEqual([]);
  });
});

describe("keymap serializeKeymap / parseKeymap round-trip (CPE-1547)", () => {
  it("round-trips a full default keymap", () => {
    const km = defaultKeymap();
    expect(parseKeymap(serializeKeymap(km))).toEqual(km);
  });

  it("round-trips an overridden keymap", () => {
    let km = defaultKeymap();
    km = setChord(km, "copy", "Ctrl+Alt+C");
    km = setChord(km, "rename", "Ctrl+Alt+R");
    expect(parseKeymap(serializeKeymap(km))).toEqual(km);
  });

  it("tolerates null/undefined/malformed JSON, degrading to defaults", () => {
    expect(parseKeymap(null)).toEqual(defaultKeymap());
    expect(parseKeymap(undefined)).toEqual(defaultKeymap());
    expect(parseKeymap("not json")).toEqual(defaultKeymap());
    expect(parseKeymap("42")).toEqual(defaultKeymap());
    expect(parseKeymap(JSON.stringify(["array", "not", "object"]))).toEqual(defaultKeymap());
  });

  it("drops an unknown/renamed action id, keeping the rest and backfilling the default", () => {
    const partial = { copy: "Ctrl+Alt+C", notARealAction: "Ctrl+Alt+Z" };
    const parsed = parseKeymap(JSON.stringify(partial));
    expect(parsed.copy).toBe("Ctrl+Alt+C");
    expect("notARealAction" in parsed).toBe(false);
    const cutDef = ACTIONS.find((a) => a.id === "cut")!;
    expect(parsed.cut).toBe(cutDef.defaultChord); // backfilled
  });

  it("drops an invalid chord string, backfilling that action's default", () => {
    const partial = { copy: "Ctrl+Alt" }; // modifier-only, invalid
    const parsed = parseKeymap(JSON.stringify(partial));
    const copyDef = ACTIONS.find((a) => a.id === "copy")!;
    expect(parsed.copy).toBe(copyDef.defaultChord);
  });

  it("drops a non-string value, backfilling that action's default", () => {
    const partial = { copy: 5 };
    const parsed = parseKeymap(JSON.stringify(partial));
    const copyDef = ACTIONS.find((a) => a.id === "copy")!;
    expect(parsed.copy).toBe(copyDef.defaultChord);
  });

  it("preserves an explicit '' (unbound) override rather than backfilling it", () => {
    const partial = { copy: "" };
    const parsed = parseKeymap(JSON.stringify(partial));
    expect(parsed.copy).toBe("");
  });

  it("merges a partial map missing several actions cleanly with defaults", () => {
    const partial: Partial<Record<ActionId, string>> = { back: "Ctrl+Alt+B" };
    const parsed = parseKeymap(JSON.stringify(partial));
    expect(parsed.back).toBe("Ctrl+Alt+B");
    const fullDefaults = defaultKeymap();
    for (const a of ACTIONS) {
      if (a.id === "back") continue;
      expect(parsed[a.id]).toBe(fullDefaults[a.id]);
    }
  });

  it("preserves a grandfathered bare-key default surviving a round-trip", () => {
    const km: Keymap = { ...defaultKeymap(), refresh: "F5" };
    const parsed = parseKeymap(serializeKeymap(km));
    expect(parsed.refresh).toBe("F5");
  });
});

describe("keymap formatChord (CPE-1548)", () => {
  it("renders an unbound chord as 'Unbound'", () => {
    expect(formatChord("")).toBe("Unbound");
  });

  it("substitutes arrow keys with their glyph, keeping modifiers as-is", () => {
    expect(formatChord("Alt+ArrowLeft")).toBe("Alt+←");
    expect(formatChord("Alt+ArrowRight")).toBe("Alt+→");
    expect(formatChord("Alt+ArrowUp")).toBe("Alt+↑");
  });

  it("substitutes Escape with 'Esc' and Delete with 'Del'", () => {
    expect(formatChord("Escape")).toBe("Esc");
    expect(formatChord("Shift+Delete")).toBe("Shift+Del");
  });

  it("passes through a chord with no display substitution unchanged", () => {
    expect(formatChord("Ctrl+Shift+F")).toBe("Ctrl+Shift+F");
    expect(formatChord("F5")).toBe("F5");
    expect(formatChord("Enter")).toBe("Enter");
  });

  it("is purely cosmetic — the source ActionDef.defaultChord is untouched by formatting it", () => {
    const before = ACTIONS.map((a) => a.defaultChord);
    for (const a of ACTIONS) formatChord(a.defaultChord);
    const after = ACTIONS.map((a) => a.defaultChord);
    expect(after).toEqual(before);
  });
});

describe("keymap exportKeymap / importKeymap (CPE-1550)", () => {
  it("exportKeymap wraps the full keymap in a versioned envelope", () => {
    const km = defaultKeymap();
    const json = exportKeymap(km);
    const parsed = JSON.parse(json);
    expect(parsed.version).toBe(1);
    expect(parsed.bindings).toEqual(km);
  });

  it("round-trips a default keymap: import(export(km)).keymap === km, every action applied", () => {
    const km = defaultKeymap();
    const result = importKeymap(exportKeymap(km));
    expect(result.keymap).toEqual(km);
    expect(result.rejected).toEqual([]);
    expect(result.applied.sort()).toEqual(ACTIONS.map((a) => a.id).sort());
  });

  it("round-trips an overridden keymap", () => {
    let km = defaultKeymap();
    km = setChord(km, "copy", "Ctrl+Alt+C");
    km = setChord(km, "rename", "Ctrl+Alt+R");
    const result = importKeymap(exportKeymap(km));
    expect(result.keymap).toEqual(km);
    expect(result.rejected).toEqual([]);
  });

  it("also accepts a bare { actionId: chord } map, not just the exportKeymap envelope", () => {
    const bare = JSON.stringify({ copy: "Ctrl+Alt+C" });
    const result = importKeymap(bare, defaultKeymap());
    expect(result.applied).toEqual(["copy"]);
    expect(result.rejected).toEqual([]);
    expect(result.keymap.copy).toBe("Ctrl+Alt+C");
  });

  it("reports malformed JSON as rejected, not thrown, and leaves base untouched", () => {
    const base = defaultKeymap();
    const result = importKeymap("not json at all {{{", base);
    expect(result.applied).toEqual([]);
    expect(result.rejected.length).toBeGreaterThan(0);
    expect(result.keymap).toEqual(base);
  });

  it("reports non-object JSON (array/number) as rejected", () => {
    const base = defaultKeymap();
    expect(importKeymap("42", base).rejected.length).toBeGreaterThan(0);
    expect(importKeymap(JSON.stringify(["a", "b"]), base).rejected.length).toBeGreaterThan(0);
  });

  it("rejects an unknown/renamed action id, keeping it out of applied, without throwing", () => {
    const payload = JSON.stringify({ bindings: { copy: "Ctrl+Alt+C", notARealAction: "Ctrl+Alt+Z" } });
    const result = importKeymap(payload, defaultKeymap());
    expect(result.applied).toContain("copy");
    expect(result.rejected).toContain("notARealAction");
    expect("notARealAction" in result.keymap).toBe(false);
  });

  it("rejects an un-normalizable chord, keeping the base default for that action", () => {
    const payload = JSON.stringify({ bindings: { copy: "Ctrl+Alt" } }); // modifier-only, invalid
    const base = defaultKeymap();
    const result = importKeymap(payload, base);
    expect(result.rejected).toContain("copy");
    expect(result.keymap.copy).toBe(base.copy);
  });

  it("applies an explicit '' (unbind) entry rather than rejecting it", () => {
    const payload = JSON.stringify({ bindings: { copy: "" } });
    const result = importKeymap(payload, defaultKeymap());
    expect(result.applied).toContain("copy");
    expect(result.keymap.copy).toBe("");
  });

  it("a partial import leaves untouched actions at the supplied base's value", () => {
    let base = defaultKeymap();
    base = setChord(base, "cut", "Ctrl+Alt+X");
    const payload = JSON.stringify({ bindings: { copy: "Ctrl+Alt+C" } });
    const result = importKeymap(payload, base);
    expect(result.keymap.copy).toBe("Ctrl+Alt+C");
    expect(result.keymap.cut).toBe("Ctrl+Alt+X"); // preserved from base, not reset to default
  });
});
