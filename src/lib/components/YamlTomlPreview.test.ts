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

  it("switching to a new path re-loads and replaces the previous file's tree", async () => {
    readFileTextMock.mockResolvedValueOnce(ok("a = 1"));
    const { container, rerender } = render(YamlTomlPreview, { path: "/x/a.toml", format: "toml" });
    await waitFor(() => expect(container.textContent).toContain("a"));

    readFileTextMock.mockResolvedValueOnce(ok("totally_different_key = 2"));
    await rerender({ path: "/x/b.toml", format: "toml" });
    await waitFor(() => expect(container.textContent).toContain("totally_different_key"));
  });
});
