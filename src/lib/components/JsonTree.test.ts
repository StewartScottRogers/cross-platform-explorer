/**
 * Component tests for the JSON tree preview (CPE-1573, epic CPE-1568): nested object/array/primitive
 * rendering, collapse/expand (both manual click and the collapse-all/expand-all command prop), the
 * focus-report bridge to `onValues`, and the malformed-JSON fallback message.
 */
import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import JsonTree from "./JsonTree.svelte";

describe("JsonTree", () => {
  it("shows a clear message for malformed JSON instead of a tree", () => {
    const { container, getByTestId } = render(JsonTree, { text: "{not json}" });
    expect(getByTestId("json-tree-invalid").textContent).toMatch(/Invalid JSON/);
    expect(container.querySelector('[data-testid="json-tree-root"]')).toBeNull();
  });

  it("renders nested object/array/primitive nodes with type-appropriate text", () => {
    const text = JSON.stringify({ name: "Ada", tags: ["math", "cs"], active: true, meta: null, age: 30 });
    const { container } = render(JsonTree, { text });

    const nodes = container.querySelectorAll('[data-testid="json-node"]');
    // root + name + tags + tags[0] + tags[1] + active + meta + age = 8
    expect(nodes.length).toBe(8);

    const stringVal = container.querySelector(".jt-val.jt-string");
    expect(stringVal?.textContent).toBe('"Ada"');
    const boolVal = container.querySelector(".jt-val.jt-boolean");
    expect(boolVal?.textContent).toBe("true");
    const nullVal = container.querySelector(".jt-val.jt-null");
    expect(nullVal?.textContent).toBe("null");
  });

  it("starts a top-level array/object node expanded and toggles collapsed on click", async () => {
    const text = JSON.stringify({ a: { b: 1 } });
    const { container } = render(JsonTree, { text });

    // "a" is a child object at depth 1 (< AUTO_COLLAPSE_DEPTH), so it starts expanded — its child "b"
    // should already be in the DOM.
    expect(container.querySelector('[data-json-path="$.a.b"]')).toBeTruthy();

    const aRow = container.querySelector('[data-json-path="$.a"] > .jt-row')!;
    await fireEvent.click(aRow);
    expect(container.querySelector('[data-json-path="$.a.b"]')).toBeNull();

    await fireEvent.click(aRow);
    expect(container.querySelector('[data-json-path="$.a.b"]')).toBeTruthy();
  });

  it("collapse-all / expand-all commands propagate to every node", async () => {
    const text = JSON.stringify({ a: { b: 1 }, c: [1, 2] });
    const { container, rerender } = render(JsonTree, { text, command: null });

    expect(container.querySelector('[data-json-path="$.a.b"]')).toBeTruthy();

    await rerender({ text, command: { id: "collapse-all", token: 1 } });
    expect(container.querySelector('[data-json-path="$.a.b"]')).toBeNull();
    expect(container.querySelector('[data-json-path="$.c[0]"]')).toBeNull();

    await rerender({ text, command: { id: "expand-all", token: 2 } });
    expect(container.querySelector('[data-json-path="$.a.b"]')).toBeTruthy();
    expect(container.querySelector('[data-json-path="$.c[0]"]')).toBeTruthy();
  });

  it("reports the clicked node's path and copyable value via onValues", async () => {
    const text = JSON.stringify({ a: { b: "hello" } });
    const onValues = vi.fn();
    const { container } = render(JsonTree, { text, onValues });

    const bRow = container.querySelector('[data-json-path="$.a.b"] > .jt-row')!;
    await fireEvent.click(bRow);

    expect(onValues).toHaveBeenLastCalledWith({ "copy-path": "$.a.b", "copy-value": "hello" });
  });

  it("caps huge sibling counts behind a 'more' affordance (large-file safety)", () => {
    const arr = Array.from({ length: 250 }, (_, i) => i);
    const text = JSON.stringify(arr);
    const { container, getByText } = render(JsonTree, { text });

    const rendered = container.querySelectorAll('[data-testid="json-node"]');
    // root + 200 visible children (MAX_CHILDREN), not all 251
    expect(rendered.length).toBe(201);
    expect(getByText(/50 more/)).toBeTruthy();
  });
});
