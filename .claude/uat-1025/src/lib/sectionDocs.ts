// Section → documentation mapping (CPE-595): the ONE source of truth for "which doc page documents this
// section", driving both the contextual Help open (CPE-596) and the exhaustiveness guard test. Don't
// scatter doc slugs across components — add the section here and everything else follows.
//
// Self-maintaining (CPE-597): the guard test asserts every `Section` has a mapping and every mapped slug
// exists in `DOCS`, so a new section without its doc — or a typo'd/renamed slug — fails CI.

import { DOCS } from "./docs";

/** Every user-facing surface that has contextual help. Add a section id here when it earns a doc page. */
export type Section =
  | "home"
  | "explorer"
  | "disk-usage"
  | "ai-console"
  | "agent-grid"
  | "agent-board"
  | "workbench"
  | "repositories"
  | "swarms";

/** Section id → doc slug (a `src/docs/*.md` filename without `.md`). */
const SECTION_DOC: Record<Section, string> = {
  home: "01-overview",
  explorer: "03-explorer",
  "disk-usage": "11-disk-usage",
  "ai-console": "04-ai-console",
  "agent-grid": "05-agent-grid",
  "agent-board": "06-agent-board",
  workbench: "07-workbench",
  repositories: "08-repositories",
  swarms: "09-swarms",
};

/** The default doc when a section has no page (or an unknown id is passed): the Overview. */
export const DEFAULT_DOC_SLUG = "01-overview";

/** All section ids (exhaustive) — used by the guard test and any "which sections exist" logic. */
export const SECTIONS = Object.keys(SECTION_DOC) as Section[];

/**
 * The doc slug for a section. Falls back to the default when the id isn't mapped (graceful in prod);
 * the guard test ensures a mapped slug always resolves in `DOCS`, so this never lands on a blank page.
 */
export function docSlugForSection(section: Section | null | undefined): string {
  return (section && SECTION_DOC[section]) || DEFAULT_DOC_SLUG;
}

/** Every mapped slug (+ the default), for validating against `DOCS` in the guard test. */
export function mappedDocSlugs(): string[] {
  return [...Object.values(SECTION_DOC), DEFAULT_DOC_SLUG];
}

/** True iff a slug exists in the bundled library. */
export function docSlugExists(slug: string): boolean {
  return DOCS.some((d) => d.slug === slug);
}
