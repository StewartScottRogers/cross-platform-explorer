import { describe, it, expect } from "vitest";
import {
  setBinding,
  removeBinding,
  bindingFor,
  bindingsForSurface,
  matchHotkey,
  normalizeHotkey,
  hotkeyFromEvent,
  serializeBindings,
  parseBindings,
  type MacroBinding,
} from "./macroBindings";

describe("macroBindings normalizeHotkey / hotkeyFromEvent (CPE-1191)", () => {
  it("canonicalizes modifier order and uppercases a single-char key", () => {
    expect(normalizeHotkey("alt+ctrl+k")).toBe("Ctrl+Alt+K");
    expect(normalizeHotkey("shift+ctrl+alt+1")).toBe("Ctrl+Alt+Shift+1");
  });

  it("accepts cmd/meta/control/option synonyms", () => {
    expect(normalizeHotkey("cmd+k")).toBe("Ctrl+K");
    expect(normalizeHotkey("Control+K")).toBe("Ctrl+K");
    expect(normalizeHotkey("option+K")).toBe("Alt+K");
  });

  it("rejects a combo with no key, or with only Shift (no Ctrl/Alt) — returns \"\"", () => {
    expect(normalizeHotkey("ctrl+alt")).toBe("");
    expect(normalizeHotkey("shift+k")).toBe("");
    expect(normalizeHotkey("k")).toBe("");
    expect(normalizeHotkey("")).toBe("");
  });

  it("hotkeyFromEvent builds the same canonical form from a KeyboardEvent-shaped object", () => {
    expect(
      hotkeyFromEvent({ ctrlKey: true, metaKey: false, altKey: true, shiftKey: false, key: "k" }),
    ).toBe("Ctrl+Alt+K");
    // metaKey (Cmd) counts as Ctrl, matching App.svelte's existing `ctrl = ctrlKey || metaKey`.
    expect(
      hotkeyFromEvent({ ctrlKey: false, metaKey: true, altKey: false, shiftKey: false, key: "1" }),
    ).toBe("Ctrl+1");
    // Shift-only, no Ctrl/Alt: rejected (would collide with plain typing / type-ahead find).
    expect(
      hotkeyFromEvent({ ctrlKey: false, metaKey: false, altKey: false, shiftKey: true, key: "a" }),
    ).toBe("");
  });
});

describe("macroBindings setBinding/removeBinding/bindingFor (CPE-1191)", () => {
  it("inserts a new binding with defaults when the name is unknown", () => {
    const out = setBinding([], "Tidy Downloads", { surfaces: ["context"] });
    expect(out).toEqual([{ name: "Tidy Downloads", surfaces: ["context"], hotkey: "" }]);
  });

  it("patches an existing binding by name without touching siblings", () => {
    let list = setBinding([], "A", { surfaces: ["context"] });
    list = setBinding(list, "B", { surfaces: ["palette"] });
    list = setBinding(list, "A", { hotkey: "ctrl+alt+1" });
    expect(bindingFor(list, "A")).toEqual({ name: "A", surfaces: ["context"], hotkey: "Ctrl+Alt+1" });
    expect(bindingFor(list, "B")).toEqual({ name: "B", surfaces: ["palette"], hotkey: "" });
  });

  it("removes a binding by name; a missing name is a no-op", () => {
    const list = setBinding(setBinding([], "A", {}), "B", {});
    expect(removeBinding(list, "A").map((b) => b.name)).toEqual(["B"]);
    expect(removeBinding(list, "nope")).toEqual(list);
  });
});

describe("macroBindings bindingsForSurface / matchHotkey (CPE-1191)", () => {
  it("filters by surface in list order", () => {
    let list: MacroBinding[] = [];
    list = setBinding(list, "tool", { surfaces: ["context", "palette"] });
    list = setBinding(list, "ctx-only", { surfaces: ["context"] });
    list = setBinding(list, "pal-only", { surfaces: ["palette"] });
    expect(bindingsForSurface(list, "context").map((b) => b.name)).toEqual(["tool", "ctx-only"]);
    expect(bindingsForSurface(list, "palette").map((b) => b.name)).toEqual(["tool", "pal-only"]);
  });

  it("matches a binding by its canonical hotkey", () => {
    let list: MacroBinding[] = [];
    list = setBinding(list, "Tidy", { hotkey: "ctrl+alt+1" });
    expect(matchHotkey(list, "Ctrl+Alt+1")?.name).toBe("Tidy");
    expect(matchHotkey(list, "Ctrl+Alt+2")).toBeUndefined();
    expect(matchHotkey(list, "")).toBeUndefined();
  });

  it("never matches a binding with no hotkey set", () => {
    const list = setBinding([], "Tidy", { surfaces: ["context"] });
    expect(matchHotkey(list, "Ctrl+K")).toBeUndefined();
  });
});

describe("macroBindings persistence (CPE-1191)", () => {
  it("round-trips through serialize/parse", () => {
    const list = setBinding(setBinding([], "A", { surfaces: ["context"] }), "B", {
      surfaces: ["palette"],
      hotkey: "ctrl+1",
    });
    expect(parseBindings(serializeBindings(list))).toEqual(list);
  });

  it("tolerates null/garbage and drops malformed bindings", () => {
    expect(parseBindings(null)).toEqual([]);
    expect(parseBindings("nope")).toEqual([]);
    expect(parseBindings(JSON.stringify({ not: "array" }))).toEqual([]);
    const good: MacroBinding = { name: "A", surfaces: ["context"], hotkey: "" };
    const mixed = JSON.stringify([
      good,
      { name: "", surfaces: ["context"], hotkey: "" }, // empty name
      { name: "B", hotkey: "" }, // no surfaces
      { name: "C", surfaces: ["nowhere"], hotkey: "" }, // bad surface
      { name: "D", surfaces: ["context"], hotkey: 5 }, // bad hotkey type
    ]);
    expect(parseBindings(mixed)).toEqual([good]);
  });
});
