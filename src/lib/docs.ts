// Application → Documents library (CPE-536). The docs are markdown files in `src/docs/` bundled into the
// app at build time via Vite's `import.meta.glob` (eager, raw) — so they ship offline with no runtime
// fetch. The index/search logic is pure + unit-tested; the glob wiring at the bottom feeds it.

export interface Doc {
  slug: string;
  title: string;
  order: number;
  content: string;
}

/** Split a doc's `---` frontmatter (`title`, `order`) from its markdown body. */
function frontmatter(raw: string): { meta: Record<string, string>; body: string } {
  const t = raw.replace(/^﻿/, "");
  if (t.startsWith("---")) {
    const end = t.indexOf("\n---", 3);
    if (end !== -1) {
      const meta: Record<string, string> = {};
      for (const line of t.slice(3, end).split(/\r?\n/)) {
        const i = line.indexOf(":");
        if (i > 0) meta[line.slice(0, i).trim()] = line.slice(i + 1).trim().replace(/^["']|["']$/g, "");
      }
      return { meta, body: t.slice(end + 4).replace(/^\r?\n/, "") };
    }
  }
  return { meta: {}, body: t };
}

/** The doc slug from its file path (`.../06-agent-board.md` → `06-agent-board`). */
export function slugFromPath(path: string): string {
  return (path.split("/").pop() || "").replace(/\.md$/i, "");
}

/** Parse one doc; a missing title falls back to the slug, a missing/NaN order sorts last. */
export function parseDoc(slug: string, raw: string): Doc {
  const { meta, body } = frontmatter(raw);
  const order = Number(meta.order);
  return {
    slug,
    title: meta.title || slug,
    order: Number.isFinite(order) ? order : 999,
    content: body.trim(),
  };
}

/** Build the ordered doc index from a `path → raw` map (by `order`, then title). */
export function buildIndex(rawByPath: Record<string, string>): Doc[] {
  return Object.entries(rawByPath)
    .map(([path, raw]) => parseDoc(slugFromPath(path), raw))
    .sort((a, b) => a.order - b.order || a.title.localeCompare(b.title));
}

/** Filter docs whose title or body contains `query` (case-insensitive); empty query ⇒ all. */
export function searchDocs(docs: Doc[], query: string): Doc[] {
  const q = query.trim().toLowerCase();
  if (!q) return docs;
  return docs.filter((d) => d.title.toLowerCase().includes(q) || d.content.toLowerCase().includes(q));
}

// The bundled library — every `src/docs/*.md`, imported raw at build time.
const glob = import.meta.glob("../docs/*.md", { query: "?raw", import: "default", eager: true }) as Record<
  string,
  string
>;

/** The full, ordered documentation set, built into the app. */
export const DOCS: Doc[] = buildIndex(glob);
