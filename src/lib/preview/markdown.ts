/**
 * Render markdown to sanitized HTML for the preview pane.
 *
 * A previewed `.md` file is untrusted content, so its rendered HTML MUST be
 * sanitized: `marked` turns markdown into HTML, then `DOMPurify` strips
 * `<script>`, inline event handlers, `javascript:` URLs, etc. The result is safe
 * to inject with `{@html}`. Everything is bundled — no network requests.
 */
import { marked } from "marked";
import DOMPurify from "dompurify";

export function renderMarkdown(src: string): string {
  const html = marked.parse(src, { async: false }) as string;
  return DOMPurify.sanitize(html);
}
