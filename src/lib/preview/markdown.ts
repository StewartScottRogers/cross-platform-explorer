/**
 * Render markdown to sanitized HTML for the preview pane — LAZY.
 *
 * `marked` and `DOMPurify` are dynamically imported the first time a markdown
 * file is previewed, so they are code-split out of the initial bundle. A
 * previewed .md file is untrusted, so its HTML is always sanitized (script,
 * inline handlers, javascript: URLs stripped) before it is injected.
 */
export async function renderMarkdown(src: string): Promise<string> {
  const [{ marked }, DOMPurify] = await Promise.all([
    import("marked"),
    import("dompurify"),
  ]);
  const html = marked.parse(src, { async: false }) as string;
  return DOMPurify.default.sanitize(html);
}
