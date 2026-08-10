/**
 * NavCommandLine tests (CPE-1554) — the mini ":" command-line bar for Navigation Mode. Verified
 * against a mock `Command[]` prop only; the component is not mounted anywhere in the running app
 * yet (that's CPE-1556's job), so these tests are its only current caller.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import NavCommandLine from "./NavCommandLine.svelte";
import type { Command } from "../commandPalette";

const cmd = (id: string, label: string, extra: Partial<Command> = {}): Command => ({ id, label, run: () => {}, ...extra });

const commands: Command[] = [
  cmd("home", "Go Home"),
  cmd("newFolder", "New folder", { keywords: "create directory mkdir" }),
  cmd("settings", "Settings"),
  cmd("locked", "Locked Action", { enabled: () => false }),
];

describe("NavCommandLine (CPE-1554)", () => {
  it("renders a ':' prefixed input and lists every enabled command when empty", () => {
    render(NavCommandLine, { commands });
    expect(screen.getByText(":")).toBeTruthy();
    const rows = screen.getAllByTestId("ncl-row");
    expect(rows).toHaveLength(3); // "Locked Action" excluded (disabled)
    expect(screen.getByText("Go Home")).toBeTruthy();
    expect(screen.getByText("New folder")).toBeTruthy();
    expect(screen.getByText("Settings")).toBeTruthy();
  });

  it("typing filters the visible list", async () => {
    render(NavCommandLine, { commands });
    const input = screen.getByTestId("ncl-input");
    await fireEvent.input(input, { target: { value: "fol" } });
    const rows = screen.getAllByTestId("ncl-row");
    expect(rows).toHaveLength(1);
    expect(screen.getByText("New folder")).toBeTruthy();
    expect(screen.queryByText("Go Home")).toBeNull();
  });

  it("Enter on the highlighted item fires a run event with that Command", async () => {
    const { component } = render(NavCommandLine, { commands });
    const runHandler = vi.fn();
    component.$on("run", (e) => runHandler((e as CustomEvent<Command>).detail));

    const input = screen.getByTestId("ncl-input");
    await fireEvent.input(input, { target: { value: "home" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    expect(runHandler).toHaveBeenCalledTimes(1);
    expect(runHandler.mock.calls[0][0]).toMatchObject({ id: "home", label: "Go Home" });
  });

  it("clicking a result row fires a run event with that Command", async () => {
    const { component } = render(NavCommandLine, { commands });
    const runHandler = vi.fn();
    component.$on("run", (e) => runHandler((e as CustomEvent<Command>).detail));

    await fireEvent.click(screen.getByText("Settings"));

    expect(runHandler).toHaveBeenCalledTimes(1);
    expect(runHandler.mock.calls[0][0]).toMatchObject({ id: "settings" });
  });

  it("Escape fires cancel without a run event", async () => {
    const { component } = render(NavCommandLine, { commands });
    const runHandler = vi.fn();
    const cancelHandler = vi.fn();
    component.$on("run", runHandler);
    component.$on("cancel", cancelHandler);

    const input = screen.getByTestId("ncl-input");
    await fireEvent.keyDown(input, { key: "Escape" });

    expect(cancelHandler).toHaveBeenCalledTimes(1);
    expect(runHandler).not.toHaveBeenCalled();
  });

  it("shows not-found feedback for an unknown command and Enter does not fire run", async () => {
    const { component } = render(NavCommandLine, { commands });
    const runHandler = vi.fn();
    component.$on("run", runHandler);

    const input = screen.getByTestId("ncl-input");
    await fireEvent.input(input, { target: { value: "zzzznope" } });
    expect(screen.getByTestId("ncl-empty")).toBeTruthy();
    expect(screen.queryAllByTestId("ncl-row")).toHaveLength(0);

    await fireEvent.keyDown(input, { key: "Enter" });
    expect(runHandler).not.toHaveBeenCalled();
  });
});
