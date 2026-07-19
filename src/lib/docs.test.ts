// CPE-536: docs library — frontmatter parse, ordered index, search, and the real bundled set.
import { describe, it, expect } from "vitest";
import { parseDoc, buildIndex, searchDocs, groupDocs, slugFromPath, DOCS } from "./docs";

describe("docs library (CPE-536)", () => {
  it("parses frontmatter title/order/category + body; falls back sensibly", () => {
    const d = parseDoc(
      "06-board",
      "---\ntitle: Agent Board\norder: 6\ncategory: AI Console\ncategoryOrder: 3\n---\n\n# Board\nDrag cards.",
    );
    expect(d).toEqual({
      slug: "06-board",
      title: "Agent Board",
      order: 6,
      category: "AI Console",
      categoryOrder: 3,
      content: "# Board\nDrag cards.",
    });
    // No frontmatter → title = slug, order last, category General/last.
    const d2 = parseDoc("loose", "just text");
    expect(d2.title).toBe("loose");
    expect(d2.order).toBe(999);
    expect(d2.category).toBe("General");
    expect(d2.categoryOrder).toBe(999);
  });

  it("slugFromPath strips the folder + extension", () => {
    expect(slugFromPath("../docs/01-overview.md")).toBe("01-overview");
  });

  it("buildIndex orders by `order` then title", () => {
    const idx = buildIndex({
      "/x/b.md": "---\ntitle: B\norder: 2\n---\nbee",
      "/x/a.md": "---\ntitle: A\norder: 1\n---\nay",
      "/x/z.md": "---\ntitle: Z\n---\nno order (last)",
    });
    expect(idx.map((d) => d.title)).toEqual(["A", "B", "Z"]);
  });

  it("searchDocs matches title or body, case-insensitively; empty = all", () => {
    const docs = buildIndex({
      "/x/a.md": "---\ntitle: Explorer\norder: 1\n---\nbrowse folders",
      "/x/b.md": "---\ntitle: Console\norder: 2\n---\nrun a coding agent",
    });
    expect(searchDocs(docs, "AGENT").map((d) => d.title)).toEqual(["Console"]);
    expect(searchDocs(docs, "explorer").map((d) => d.title)).toEqual(["Explorer"]);
    expect(searchDocs(docs, "  ").length).toBe(2);
  });

  it("groupDocs buckets docs into ordered categories, docs keep their order within a group", () => {
    const docs = buildIndex({
      "/x/a.md": "---\ntitle: Overview\norder: 1\ncategory: Getting started\ncategoryOrder: 1\n---\nintro",
      "/x/b.md": "---\ntitle: Explorer\norder: 3\ncategory: Explorer\ncategoryOrder: 2\n---\nbrowse",
      "/x/c.md": "---\ntitle: Launch\norder: 2\ncategory: Getting started\ncategoryOrder: 1\n---\nflags",
    });
    const groups = groupDocs(docs);
    expect(groups.map((g) => g.name)).toEqual(["Getting started", "Explorer"]);
    // Within a group the incoming (order-sorted) sequence is preserved: Overview(1) before Launch(2).
    expect(groups[0].docs.map((d) => d.title)).toEqual(["Overview", "Launch"]);
    expect(groups[1].docs.map((d) => d.title)).toEqual(["Explorer"]);
  });

  it("the real bundled library groups cleanly (every doc lands in a category)", () => {
    const groups = groupDocs(DOCS);
    expect(groups.length).toBeGreaterThanOrEqual(2);
    expect(groups.reduce((n, g) => n + g.docs.length, 0)).toBe(DOCS.length);
    // No doc falls back to the "General" bucket — every bundled page declares its category.
    expect(DOCS.every((d) => d.category !== "General")).toBe(true);
  });

  it("the real bundled library is present + ordered (Overview first)", () => {
    expect(DOCS.length).toBeGreaterThanOrEqual(8);
    expect(DOCS[0].title).toBe("Overview");
    // Every doc has a title + non-empty content.
    expect(DOCS.every((d) => d.title && d.content.length > 40)).toBe(true);
    // Orders are non-decreasing.
    for (let i = 1; i < DOCS.length; i++) expect(DOCS[i].order).toBeGreaterThanOrEqual(DOCS[i - 1].order);
  });
});
