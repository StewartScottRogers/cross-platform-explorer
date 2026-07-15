/**
 * AgentMenu render test (CPE-457) — the right-click "close the AI Console" menu shown on an Agents
 * leaf / the AI Console button. Verifies the label is shown and confirming dispatches `confirm`.
 */
import { render, screen, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import AgentMenu from "./AgentMenu.svelte";

describe("AgentMenu", () => {
  it("shows the given label and dispatches confirm + close when clicked", async () => {
    const { component } = render(AgentMenu, { props: { x: 10, y: 10, label: "Close all consoles" } });
    const confirm = vi.fn();
    const close = vi.fn();
    component.$on("confirm", confirm);
    component.$on("close", close);

    const item = screen.getByRole("menuitem", { name: /close all consoles/i });
    await fireEvent.click(item);
    expect(confirm).toHaveBeenCalledOnce();
    expect(close).toHaveBeenCalledOnce();
  });

  it("uses a per-leaf label when given one", () => {
    render(AgentMenu, { props: { label: "Close AI Console" } });
    expect(screen.getByRole("menuitem", { name: /close ai console/i })).toBeTruthy();
  });
});
