/**
 * CPE-1573 (epic CPE-1568) — the `json` provider's declared `actions` (`preview/provider.ts`), rendered
 * by PreviewPane's generic action bar (CPE-1570) and run end to end: Copy value/path, Format, Collapse/
 * Expand all, and Validate. Mirrors `PreviewPane.jwtActions.test.ts`'s recipe.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import PreviewPane from "./PreviewPane.svelte";
import type { DirEntry } from "../types";

const entry = (over: Partial<DirEntry>): DirEntry => ({
  name: "x",
  path: "/x",
  is_dir: false,
  size: 1,
  modified: 0,
  extension: "",
  hidden: false,
  is_symlink: false,
  ...over,
});

function jsonEntry(path = "C:\\d\\d.json") {
  return entry({ name: "d.json", path, extension: "json" });
}

function setClipboard() {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  return writeText;
}

describe("PreviewPane — json provider action bar (CPE-1573)", () => {
  it("renders the json provider's declared actions (Copy path excluded until a node is focused)", async () => {
    const { container } = render(PreviewPane, {
      entry: jsonEntry(),
      loadText: async () => '{"a":1}',
    });

    await waitFor(() => expect(container.querySelector('[data-testid="preview-action-bar"]')).toBeTruthy());
    const bar = container.querySelector('[data-testid="preview-action-bar"]')!;
    const labels = [...bar.querySelectorAll("button")].map((b) => b.textContent?.trim());
    expect(labels).toEqual(["Copy value", "Format", "Collapse all", "Expand all", "Validate"]);
  });

  it("Copy path is disabled (absent) until a tree node is focused", async () => {
    const { container } = render(PreviewPane, {
      entry: jsonEntry(),
      loadText: async () => '{"a":{"b":1}}',
    });
    await waitFor(() => expect(container.querySelector('[data-testid="preview-action-bar"]')).toBeTruthy());
    let bar = container.querySelector('[data-testid="preview-action-bar"]')!;
    expect([...bar.querySelectorAll("button")].map((b) => b.textContent?.trim())).not.toContain("Copy path");

    await waitFor(() => expect(container.querySelector('[data-json-path="$.a.b"] > .jt-row')).toBeTruthy());
    await fireEvent.click(container.querySelector('[data-json-path="$.a.b"] > .jt-row')!);

    bar = container.querySelector('[data-testid="preview-action-bar"]')!;
    expect([...bar.querySelectorAll("button")].map((b) => b.textContent?.trim())).toContain("Copy path");
  });

  it("Copy value copies the focused node's value; falls back to the whole doc when nothing's focused", async () => {
    const writeText = setClipboard();
    render(PreviewPane, { entry: jsonEntry(), loadText: async () => '{"a":"hello"}' });

    const copyValueBtn = await waitFor(() => screen.getByRole("button", { name: /Copy value/ }));
    await fireEvent.click(copyValueBtn);
    expect(writeText).toHaveBeenCalledWith('{"a":"hello"}'); // no node focused yet -> raw whole doc

    await waitFor(() => expect(document.querySelector('[data-json-path="$.a"] > .jt-row')).toBeTruthy());
    await fireEvent.click(document.querySelector('[data-json-path="$.a"] > .jt-row')!);
    await fireEvent.click(copyValueBtn);
    expect(writeText).toHaveBeenLastCalledWith("hello");
  });

  it("Copy path copies the focused node's path", async () => {
    const writeText = setClipboard();
    render(PreviewPane, { entry: jsonEntry(), loadText: async () => '{"a":{"b":1}}' });

    await waitFor(() => expect(document.querySelector('[data-json-path="$.a.b"] > .jt-row')).toBeTruthy());
    const bRow = document.querySelector('[data-json-path="$.a.b"] > .jt-row')!;
    await fireEvent.click(bRow);

    const copyPathBtn = await waitFor(() => screen.getByRole("button", { name: /Copy path/ }));
    await fireEvent.click(copyPathBtn);
    expect(writeText).toHaveBeenCalledWith("$.a.b");
  });

  it("Format copies normalized JSON and shows a confirmation message", async () => {
    const writeText = setClipboard();
    render(PreviewPane, { entry: jsonEntry(), loadText: async () => '{"a":1,"b":2}' });

    const formatBtn = await waitFor(() => screen.getByRole("button", { name: /Format/ }));
    await fireEvent.click(formatBtn);

    expect(writeText).toHaveBeenCalledWith('{\n  "a": 1,\n  "b": 2\n}');
    await waitFor(() => expect(screen.getByTestId("preview-action-message").textContent).toContain("Copied formatted JSON"));
  });

  it("Format on malformed JSON reports the parse error instead of copying", async () => {
    const writeText = setClipboard();
    render(PreviewPane, { entry: jsonEntry(), loadText: async () => "{not json}" });

    const formatBtn = await waitFor(() => screen.getByRole("button", { name: /Format/ }));
    await fireEvent.click(formatBtn);

    expect(writeText).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByTestId("preview-action-message").textContent).toContain("Can't format"));
  });

  it("Validate reports 'Valid JSON' for a well-formed file", async () => {
    render(PreviewPane, { entry: jsonEntry(), loadText: async () => '{"a":1}' });

    const validateBtn = await waitFor(() => screen.getByRole("button", { name: /Validate/ }));
    await fireEvent.click(validateBtn);

    await waitFor(() => expect(screen.getByTestId("preview-action-message").textContent).toContain("Valid JSON"));
  });

  it("Validate surfaces a clear message for malformed JSON", async () => {
    render(PreviewPane, { entry: jsonEntry(), loadText: async () => "{oops" });

    const validateBtn = await waitFor(() => screen.getByRole("button", { name: /Validate/ }));
    await fireEvent.click(validateBtn);

    const msg = await waitFor(() => screen.getByTestId("preview-action-message"));
    expect(msg.textContent).toContain("Invalid JSON");
    expect(msg.className).toContain("err");
  });

  it("Collapse all / Expand all reach into the mounted tree", async () => {
    const { container } = render(PreviewPane, { entry: jsonEntry(), loadText: async () => '{"a":{"b":1}}' });
    await waitFor(() => expect(container.querySelector('[data-json-path="$.a.b"]')).toBeTruthy());

    const collapseBtn = await waitFor(() => screen.getByRole("button", { name: /Collapse all/ }));
    await fireEvent.click(collapseBtn);
    await waitFor(() => expect(container.querySelector('[data-json-path="$.a.b"]')).toBeNull());

    const expandBtn = await waitFor(() => screen.getByRole("button", { name: /Expand all/ }));
    await fireEvent.click(expandBtn);
    await waitFor(() => expect(container.querySelector('[data-json-path="$.a.b"]')).toBeTruthy());
  });
});
