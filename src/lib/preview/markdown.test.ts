import { describe, it, expect } from "vitest";
import { renderMarkdown } from "./markdown";

describe("renderMarkdown", () => {
  it("renders headings and inline formatting", () => {
    const html = renderMarkdown("# Title\n\nsome **bold** text");
    expect(html).toContain("<h1");
    expect(html).toContain("<strong>bold</strong>");
  });

  it("renders lists", () => {
    const html = renderMarkdown("- one\n- two");
    expect(html).toContain("<li>one</li>");
    expect(html).toContain("<li>two</li>");
  });

  it("strips script tags (sanitization)", () => {
    const html = renderMarkdown("# Hi\n\n<script>window.evil=1</script>");
    expect(html).toContain("<h1");
    expect(html.toLowerCase()).not.toContain("<script");
  });

  it("strips inline event handlers", () => {
    const html = renderMarkdown('<img src="x" onerror="evil()">');
    expect(html.toLowerCase()).not.toContain("onerror");
  });
});
