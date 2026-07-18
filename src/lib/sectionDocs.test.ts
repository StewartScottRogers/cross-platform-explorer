import { describe, it, expect } from "vitest";
import { SECTIONS, docSlugForSection, mappedDocSlugs, docSlugExists, DEFAULT_DOC_SLUG } from "./sectionDocs";
import { DOCS } from "./docs";

describe("section → doc registry (CPE-595)", () => {
  it("maps every section to a slug that exists in DOCS (exhaustive, no dangling slugs)", () => {
    // Self-maintaining discipline: a new section without a mapping, or a mapped-but-missing slug, fails here.
    expect(SECTIONS.length).toBeGreaterThan(0);
    for (const s of SECTIONS) {
      const slug = docSlugForSection(s);
      expect(docSlugExists(slug), `section "${s}" → "${slug}" must exist in DOCS`).toBe(true);
    }
    for (const slug of mappedDocSlugs()) {
      expect(DOCS.some((d) => d.slug === slug), `mapped slug "${slug}" must exist in DOCS`).toBe(true);
    }
  });

  it("falls back to the default doc for an unknown/absent section", () => {
    expect(docSlugForSection(null)).toBe(DEFAULT_DOC_SLUG);
    // @ts-expect-error — an unknown id degrades gracefully rather than blank-paging.
    expect(docSlugForSection("does-not-exist")).toBe(DEFAULT_DOC_SLUG);
    expect(docSlugExists(DEFAULT_DOC_SLUG)).toBe(true);
  });
});
