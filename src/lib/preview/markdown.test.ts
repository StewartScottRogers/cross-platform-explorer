import { describe, it, expect } from "vitest";
import { renderMarkdown } from "./markdown";

describe("renderMarkdown (lazy)", () => {
  it("renders headings and inline formatting", async () => {
    const html = await renderMarkdown("# Title\n\nsome **bold** text");
    expect(html).toContain("<h1");
    expect(html).toContain("<strong>bold</strong>");
  });

  it("renders lists", async () => {
    const html = await renderMarkdown("- one\n- two");
    expect(html).toContain("<li>one</li>");
    expect(html).toContain("<li>two</li>");
  });

  it("strips script tags (sanitization)", async () => {
    const html = await renderMarkdown("# Hi\n\n<script>window.evil=1</script>");
    expect(html).toContain("<h1");
    expect(html.toLowerCase()).not.toContain("<script");
  });

  it("strips inline event handlers", async () => {
    const html = await renderMarkdown('<img src="x" onerror="evil()">');
    expect(html.toLowerCase()).not.toContain("onerror");
  });
});
