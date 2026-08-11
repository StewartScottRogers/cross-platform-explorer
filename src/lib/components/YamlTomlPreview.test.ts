import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import YamlTomlPreview from "./YamlTomlPreview.svelte";

// CPE-1617 (epic CPE-1568 slice 7): jsdom render-spec for the YAML/TOML structured preview, wiring the
// generic `read_file_text` backend command into a standalone component (same mocking recipe as
// NotebookPreview.test.ts/LogPreview.test.ts: mock `../bindings.gen`'s `commands` object). jsdom can't
// see layout, so these assert text/DOM content and state-transition robustness only — never a visual
// verdict.

const { readFileTextMock } = vi.hoisted(() => ({ readFileTextMock: vi.fn() }));

vi.mock("../bindings.gen", () => ({
  commands: { readFileText: readFileTextMock },
}));

function ok(text: string) {
  return { status: "ok" as const, data: text };
}

beforeEach(() => {
  readFileTextMock.mockReset();
});

describe("YamlTomlPreview — TOML", () => {
  it("renders a nested table/array structure as a tree, not flat text", async () => {
    readFileTextMock.mockResolvedValueOnce(
      ok(['[server]', 'host = "localhost"', "port = 8080", "", "tags = [1, 2, 3]"].join("\n")),
    );

    const { container } = render(YamlTomlPreview, { path: "/x/config.toml", format: "toml" });

    await waitFor(() => expect(container.querySelector('[data-testid="yamltoml-tree"]')).toBeTruthy());
    expect(readFileTextMock).toHaveBeenCalledWith("/x/config.toml", expect.any(Number));
    expect(container.querySelector('[data-testid="yamltoml-parse-error"]')).toBeFalsy();
    expect(container.textContent).toContain("server");
    expect(container.textContent).toContain("localhost");
    expect(container.textContent).toContain("tags");
  });

  it("shows a specific, real parser error on a malformed table header, and degrades to raw text", async () => {
    readFileTextMock.mockResolvedValueOnce(ok("[server\nname = 1"));

    const { container } = render(YamlTomlPreview, { path: "/x/bad.toml", format: "toml" });

    await waitFor(() => expect(container.querySelector('[data-testid="yamltoml-parse-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="yamltoml-parse-error"]')!.textContent).toMatch(
      /malformed table header/i,
    );
    // The raw fallback keeps the original content visible rather than a blank pane.
    expect(container.querySelector('[data-testid="yamltoml-raw-fallback"]')!.textContent).toContain("[server");
    expect(container.querySelector('[data-testid="yamltoml-tree"]')).toBeFalsy();
  });
});

describe("YamlTomlPreview — YAML", () => {
  it("renders a nested mapping/sequence structure as a tree, not flat text", async () => {
    readFileTextMock.mockResolvedValueOnce(
      ok(["server:", "  host: localhost", "  port: 8080", "fruits:", "  - apple", "  - banana"].join("\n")),
    );

    const { container } = render(YamlTomlPreview, { path: "/x/config.yaml", format: "yaml" });

    await waitFor(() => expect(container.querySelector('[data-testid="yamltoml-tree"]')).toBeTruthy());
    expect(container.textContent).toContain("localhost");
    expect(container.textContent).toContain("fruits");
    expect(container.textContent).toContain("apple");
  });

  it("shows a specific, real parser error on bad indentation (a genuine syntax error, not 'unsupported')", async () => {
    readFileTextMock.mockResolvedValueOnce(ok(["a: 1", "  b: 2"].join("\n")));

    const { container } = render(YamlTomlPreview, { path: "/x/bad.yaml", format: "yaml" });

    await waitFor(() => expect(container.querySelector('[data-testid="yamltoml-parse-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="yamltoml-parse-error"]')!.textContent).toMatch(
      /doesn't look like valid YAML/i,
    );
    expect(container.querySelector('[data-testid="yamltoml-unsupported"]')).toBeFalsy();
  });

  it("degrades EXPLICITLY with a stated reason on an anchor (deliberately unsupported), distinct from a real error", async () => {
    readFileTextMock.mockResolvedValueOnce(ok("base: &anchor\n  a: 1"));

    const { container } = render(YamlTomlPreview, { path: "/x/anchor.yaml", format: "yaml" });

    await waitFor(() => expect(container.querySelector('[data-testid="yamltoml-unsupported"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="yamltoml-unsupported"]')!.textContent).toMatch(/anchor/i);
    expect(container.querySelector('[data-testid="yamltoml-parse-error"]')).toBeFalsy();
    // Raw content is still shown — never a blank pane.
    expect(container.querySelector('[data-testid="yamltoml-raw-fallback"]')!.textContent).toContain("&anchor");
  });

  it("renders a real block scalar (GH Actions 'run: |' shape) as a structured tree", async () => {
    // A top-level key (not nested past JsonTree's auto-collapse depth) so the block scalar's own
    // resolved text is visible without simulating an expand-click — jsdom can't judge layout, but the
    // tree/error/unsupported STATE and the resolved value both belong in this component-level test.
    readFileTextMock.mockResolvedValueOnce(ok(["run: |", "  npm ci", "  npm test"].join("\n")));

    const { container } = render(YamlTomlPreview, { path: "/x/workflow.yaml", format: "yaml" });

    await waitFor(() => expect(container.querySelector('[data-testid="yamltoml-tree"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="yamltoml-parse-error"]')).toBeFalsy();
    expect(container.querySelector('[data-testid="yamltoml-unsupported"]')).toBeFalsy();
    expect(container.textContent).toContain("npm ci");
  });
});

// CPE-1617 PR #833 review finding #9: the degrade/error banners read as run-on sentences (no
// punctuation between the reason and "Showing the raw file content instead.").
describe("YamlTomlPreview — finding #9: banner punctuation", () => {
  it("the unsupported-degrade banner has a period between the reason and the follow-up sentence", async () => {
    readFileTextMock.mockResolvedValueOnce(ok("base: &anchor\n  a: 1"));
    const { container } = render(YamlTomlPreview, { path: "/x/anchor.yaml", format: "yaml" });
    await waitFor(() => expect(container.querySelector('[data-testid="yamltoml-unsupported"]')).toBeTruthy());
    // Pre-fix text ran the reason straight into "Showing" with no punctuation at all. Whitespace
    // between words is matched loosely (`\s+`) since the template's source line-wrapping puts extra
    // whitespace/newlines into raw `textContent` that a real rendered layout would collapse visually.
    expect(container.querySelector('[data-testid="yamltoml-unsupported"]')!.textContent).toMatch(
      /this preview\.\s+Showing\s+the raw file content instead\./,
    );
  });

  it("the parse-error banner has a period between the reason and the follow-up sentence", async () => {
    readFileTextMock.mockResolvedValueOnce(ok("[server\nname = 1"));
    const { container } = render(YamlTomlPreview, { path: "/x/bad.toml", format: "toml" });
    await waitFor(() => expect(container.querySelector('[data-testid="yamltoml-parse-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="yamltoml-parse-error"]')!.textContent).toMatch(
      /expected '\]'\.\s+Showing\s+the raw file content instead\./,
    );
  });
});

describe("YamlTomlPreview — shared robustness (empty file / load error / structural distinctness)", () => {
  it("an empty file shows an explicit 'empty' state, never confused with a parse error or a blank tree", async () => {
    readFileTextMock.mockResolvedValueOnce(ok(""));

    const { container } = render(YamlTomlPreview, { path: "/x/empty.yaml", format: "yaml" });

    await waitFor(() => expect(container.querySelector('[data-testid="yamltoml-empty"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="yamltoml-tree"]')).toBeFalsy();
    expect(container.querySelector('[data-testid="yamltoml-parse-error"]')).toBeFalsy();
    expect(container.querySelector('[data-testid="yamltoml-unsupported"]')).toBeFalsy();
  });

  it("a load error (e.g. file too large) shows a load-error state without crashing", async () => {
    readFileTextMock.mockRejectedValueOnce(new Error("File is too large to preview"));

    const { container } = render(YamlTomlPreview, { path: "/x/huge.toml", format: "toml" });

    await waitFor(() => expect(container.querySelector('[data-testid="yamltoml-load-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="yamltoml-load-error"]')!.textContent).toContain("too large");
  });

  // CPE-1617 PR #833 review finding #10: a whitespace/comment-only file (non-zero bytes) parses
  // SUCCESSFULLY (YAML -> null, TOML -> {}) but pre-fix rendered a bare degenerate tree node ("null" /
  // an empty object) rather than the same explicit empty state a truly empty file gets.
  it("a comment-only YAML file shows the empty state, not a bare 'null' tree node", async () => {
    readFileTextMock.mockResolvedValueOnce(ok("# just a comment\n# another comment\n"));
    const { container } = render(YamlTomlPreview, { path: "/x/comments.yaml", format: "yaml" });
    await waitFor(() => expect(container.querySelector('[data-testid="yamltoml-empty"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="yamltoml-tree"]')).toBeFalsy();
  });

  it("a comment-only TOML file shows the empty state, not a bare empty-object tree node", async () => {
    readFileTextMock.mockResolvedValueOnce(ok("# just a comment\n\n# another comment\n"));
    const { container } = render(YamlTomlPreview, { path: "/x/comments.toml", format: "toml" });
    await waitFor(() => expect(container.querySelector('[data-testid="yamltoml-empty"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="yamltoml-tree"]')).toBeFalsy();
  });

  it("switching to a new path re-loads and replaces the previous file's tree", async () => {
    readFileTextMock.mockResolvedValueOnce(ok("a = 1"));
    const { container, rerender } = render(YamlTomlPreview, { path: "/x/a.toml", format: "toml" });
    await waitFor(() => expect(container.textContent).toContain("a"));

    readFileTextMock.mockResolvedValueOnce(ok("totally_different_key = 2"));
    await rerender({ path: "/x/b.toml", format: "toml" });
    await waitFor(() => expect(container.textContent).toContain("totally_different_key"));
  });
});
