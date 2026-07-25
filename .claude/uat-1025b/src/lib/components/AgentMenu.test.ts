/**
 * AgentMenu render test (CPE-457) — the right-click "close the Agent Deck" menu shown on an Agents
 * leaf / the Agent Deck button. Verifies the label is shown and confirming dispatches `confirm`.
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
    render(AgentMenu, { props: { label: "Close Agent Deck" } });
    expect(screen.getByRole("menuitem", { name: /close agent deck/i })).toBeTruthy();
  });

  it("offers a per-session close that dispatches closeOne with the id (CPE-489)", async () => {
    const { component, container } = render(AgentMenu, {
      props: { label: "Close all consoles", sessionId: "s2", sessionLabel: "claude · sonnet-4.5" },
    });
    const closeOne = vi.fn();
    component.$on("closeOne", closeOne);

    // Both items are present: the specific session AND close-all.
    const one = screen.getByRole("menuitem", { name: /close claude · sonnet-4\.5/i });
    expect(screen.getByRole("menuitem", { name: /close all consoles/i })).toBeTruthy();

    // The per-session item carries the same chip as the leaf (CPE-493): number from the id.
    const chip = container.querySelector(".menu-chip") as HTMLElement;
    expect(chip).toBeTruthy();
    expect(chip.textContent).toBe("2");
    expect(chip.style.background).not.toBe("");

    await fireEvent.click(one);
    expect(closeOne).toHaveBeenCalledOnce();
    expect(closeOne.mock.calls[0][0].detail).toBe("s2");
  });

  it("shows only close-all when no session id is given (toolbar button)", () => {
    render(AgentMenu, { props: { label: "Close all consoles" } });
    expect(screen.queryAllByRole("menuitem")).toHaveLength(1);
  });
});
