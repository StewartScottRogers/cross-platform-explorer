import { describe, it, expect } from "vitest";
import {
  SECTION_IDS,
  defaultOrder,
  mergeOrder,
  parseOrder,
  moveBy,
  moveToEnd,
  moveNextTo,
  orderStyleMap,
} from "./sidebarOrder";
import { toggled, isOpen } from "./sidebarSections";

describe("sidebarOrder (CPE-1520)", () => {
  it("defaultOrder matches the app's original hardcoded top-to-bottom order", () => {
    expect(defaultOrder()).toEqual([
      "agents",
      "favorites",
      "tags",
      "smart",
      "savedSearch",
      "explore",
      "places",
      "drives",
      "network",
    ]);
  });

  it("defaultOrder returns a fresh array each call (callers can't mutate the shared default)", () => {
    const a = defaultOrder();
    a.push("bogus");
    expect(defaultOrder()).toEqual([...SECTION_IDS]);
  });

  describe("parseOrder", () => {
    it("falls back to the default order when unset", () => {
      expect(parseOrder(null)).toEqual(defaultOrder());
    });

    it("falls back to the default order on malformed JSON", () => {
      expect(parseOrder("not json")).toEqual(defaultOrder());
    });

    it("falls back to the default order on legacy/unexpected shapes (object, not an array)", () => {
      expect(parseOrder('{"agents":true}')).toEqual(defaultOrder());
      expect(parseOrder("[1,2,3]")).toEqual(defaultOrder()); // array, but not of strings
    });

    it("accepts a valid full permutation as-is", () => {
      const custom = ["network", "drives", "places", "explore", "savedSearch", "smart", "tags", "favorites", "agents"];
      expect(parseOrder(JSON.stringify(custom))).toEqual(custom);
    });

    it("appends a section id missing from a persisted order, rather than stranding it off-list", () => {
      const legacy = ["favorites", "tags", "smart", "savedSearch", "explore", "places", "drives", "network"]; // no "agents"
      const merged = parseOrder(JSON.stringify(legacy));
      expect(merged).toContain("agents");
      expect(merged).toHaveLength(SECTION_IDS.length);
      expect(new Set(merged)).toEqual(new Set(SECTION_IDS));
    });

    it("drops unknown ids from a persisted order", () => {
      const stale = ["bogus", ...defaultOrder(), "alsoBogus"];
      expect(parseOrder(JSON.stringify(stale))).toEqual(defaultOrder());
    });
  });

  describe("mergeOrder", () => {
    it("dedupes a repeated id, keeping its first position", () => {
      const dup = ["tags", "tags", "agents", "favorites", "smart", "savedSearch", "explore", "places", "drives", "network"];
      const out = mergeOrder(dup);
      expect(out.filter((x) => x === "tags")).toHaveLength(1);
      expect(new Set(out)).toEqual(new Set(SECTION_IDS));
    });
  });

  describe("moveBy (up/down)", () => {
    it("moves a section up one slot", () => {
      const out = moveBy(defaultOrder(), "tags", -1);
      expect(out).toEqual(["agents", "tags", "favorites", "smart", "savedSearch", "explore", "places", "drives", "network"]);
    });

    it("moves a section down one slot", () => {
      const out = moveBy(defaultOrder(), "tags", 1);
      expect(out).toEqual(["agents", "favorites", "smart", "tags", "savedSearch", "explore", "places", "drives", "network"]);
    });

    it("is a no-op moving the first section up (top end)", () => {
      expect(moveBy(defaultOrder(), "agents", -1)).toEqual(defaultOrder());
    });

    it("is a no-op moving the last section down (bottom end)", () => {
      expect(moveBy(defaultOrder(), "network", 1)).toEqual(defaultOrder());
    });

    it("is a no-op for an unknown id", () => {
      expect(moveBy(defaultOrder(), "bogus", 1)).toEqual(defaultOrder());
    });

    it("does not mutate the input array", () => {
      const order = defaultOrder();
      moveBy(order, "tags", 1);
      expect(order).toEqual(defaultOrder());
    });
  });

  describe("moveToEnd", () => {
    it("moves a section all the way to the top", () => {
      const out = moveToEnd(defaultOrder(), "network", "start");
      expect(out[0]).toBe("network");
      expect(out).toHaveLength(SECTION_IDS.length);
    });

    it("moves a section all the way to the bottom", () => {
      const out = moveToEnd(defaultOrder(), "agents", "end");
      expect(out[out.length - 1]).toBe("agents");
      expect(out).toHaveLength(SECTION_IDS.length);
    });

    it("is a no-op for an unknown id", () => {
      expect(moveToEnd(defaultOrder(), "bogus", "start")).toEqual(defaultOrder());
    });
  });

  describe("moveNextTo (drag-and-drop reorder)", () => {
    it("drops a section immediately before a target", () => {
      const out = moveNextTo(defaultOrder(), "network", "agents", true);
      expect(out).toEqual(["network", "agents", "favorites", "tags", "smart", "savedSearch", "explore", "places", "drives"]);
    });

    it("drops a section immediately after a target", () => {
      const out = moveNextTo(defaultOrder(), "network", "agents", false);
      expect(out).toEqual(["agents", "network", "favorites", "tags", "smart", "savedSearch", "explore", "places", "drives"]);
    });

    it("is a no-op when dragging a section onto itself", () => {
      expect(moveNextTo(defaultOrder(), "tags", "tags", true)).toEqual(defaultOrder());
    });

    it("is a no-op for an unknown drag or drop id", () => {
      expect(moveNextTo(defaultOrder(), "bogus", "agents", true)).toEqual(defaultOrder());
      expect(moveNextTo(defaultOrder(), "agents", "bogus", true)).toEqual(defaultOrder());
    });
  });

  describe("orderStyleMap", () => {
    it("assigns each id a CSS order index spaced by 10, in array position", () => {
      const map = orderStyleMap(["network", "agents", "tags"]);
      expect(map).toEqual({ network: 0, agents: 10, tags: 20 });
    });
  });

  it("reordering is orthogonal to collapse state — moving sections never touches sidebarSections", () => {
    // Start from an arbitrary collapse map, apply a bunch of order reducers, and confirm the collapse
    // map is untouched (they're two separate stores/keys entirely, so this is really a documentation
    // test, but it pins the invariant the ticket calls out explicitly).
    let collapse = toggled({}, "tags"); // tags now collapsed
    expect(isOpen(collapse, "tags")).toBe(false);

    let order = defaultOrder();
    order = moveBy(order, "tags", -1);
    order = moveNextTo(order, "network", "agents", true);
    order = moveToEnd(order, "favorites", "start");

    // Collapse state, re-checked after a flurry of reorder ops, is unchanged.
    expect(isOpen(collapse, "tags")).toBe(false);
    expect(isOpen(collapse, "agents")).toBe(true); // untouched, still default-open
  });
});
