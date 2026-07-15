/**
 * Component tests for the StatusBar item count, including the "X of Y items" filtered readout
 * (CPE-407) and the free-space readout (CPE-403).
 */
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/svelte";
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
