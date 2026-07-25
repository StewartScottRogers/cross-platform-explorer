/**
 * Component tests for the StatusBar item count, including the "X of Y items" filtered readout
 * (CPE-407) and the free-space readout (CPE-403).
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import StatusBar from "./StatusBar.svelte";

describe("StatusBar item count (CPE-407)", () => {
  it("shows a plain count when nothing is filtered", () => {
    render(StatusBar, { itemCount: 5, totalCount: 5 });
    expect(screen.getByText("5 items")).toBeTruthy();
  });

  it("shows 'X of Y items' when a filter is narrowing the list", () => {
    render(StatusBar, { itemCount: 3, totalCount: 12 });
    expect(screen.getByText("3 of 12 items")).toBeTruthy();
  });

  it("uses the singular for a single item", () => {
    render(StatusBar, { itemCount: 1, totalCount: 1 });
    expect(screen.getByText("1 item")).toBeTruthy();
  });
});

describe("StatusBar free space (CPE-403)", () => {
  it("renders free/total when known", () => {
    render(StatusBar, { itemCount: 1, totalCount: 1, diskFree: 50 * 1024 ** 3, diskTotal: 200 * 1024 ** 3 });
    expect(screen.getByText(/50.0 GB free of 200.0 GB/)).toBeTruthy();
  });
  it("omits free space when unknown", () => {
    render(StatusBar, { itemCount: 1, totalCount: 1, diskFree: null, diskTotal: null });
    expect(screen.queryByText(/free of/)).toBeNull();
  });
});

describe("StatusBar git sync (CPE-462)", () => {
  it("shows branch + ahead/behind and offers Pull/Push, dispatching them", async () => {
    const { component } = render(StatusBar, {
      itemCount: 1, totalCount: 1,
      git: { is_repo: true, branch: "main", upstream: "origin/main", ahead: 2, behind: 1, dirty: true },
    });
    expect(screen.getByText(/⎇ main/)).toBeTruthy();
    expect(screen.getByText("↑2")).toBeTruthy();
    expect(screen.getByText("↓1")).toBeTruthy();

    const pull = vi.fn(); const push = vi.fn();
    component.$on("pull", pull); component.$on("push", push);
    await fireEvent.click(screen.getByRole("button", { name: /pull/i }));
    await fireEvent.click(screen.getByRole("button", { name: /push/i }));
    expect(pull).toHaveBeenCalledOnce();
    expect(push).toHaveBeenCalledOnce();
  });

  it("shows nothing git-related when the folder isn't a repo", () => {
    render(StatusBar, { itemCount: 1, totalCount: 1, git: null });
    expect(screen.queryByText(/⎇/)).toBeNull();
    expect(screen.queryByRole("button", { name: /pull|push/i })).toBeNull();
  });

  it("offers only Push when ahead-only (nothing to pull)", () => {
    render(StatusBar, { itemCount: 1, totalCount: 1, git: { is_repo: true, branch: "main", ahead: 1, behind: 0 } });
    expect(screen.getByRole("button", { name: /push/i })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /pull/i })).toBeNull();
  });
});
