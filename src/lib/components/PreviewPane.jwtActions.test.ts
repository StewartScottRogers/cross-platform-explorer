/**
 * CPE-1570 (epic CPE-1568) — the generic provider action bar, worked example: the `jwt` provider's
 * declared `actions` (`preview/provider.ts`) rendered by `PreviewPane`'s generic action bar and run end to
 * end. Mirrors JwtPreview.test.ts's recipe of mocking `../bindings.gen`'s `commands` object directly,
 * since that's the module `JwtPreview` (mounted internally by `PreviewPane` for the `jwt` provider kind)
 * imports.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import PreviewPane from "./PreviewPane.svelte";
import type { DirEntry } from "../types";

const { jwtPreviewMock } = vi.hoisted(() => ({ jwtPreviewMock: vi.fn() }));

vi.mock("../bindings.gen", () => ({
  commands: { jwtPreview: jwtPreviewMock },
}));

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

function ok(data: unknown) {
  return { status: "ok" as const, data };
}

const validToken = {
  alg: "HS256",
  typ: "JWT",
  kid: null,
  header_json: '{\n  "alg": "HS256"\n}',
  payload_json: '{\n  "sub": "1234567890"\n}',
  exp: null,
  iat: null,
  nbf: null,
  expired: null,
  not_yet_valid: null,
  signature_present: true,
  signature_len: 32,
  error: null,
};

beforeEach(() => {
  jwtPreviewMock.mockReset();
});

describe("PreviewPane — generic provider action bar (CPE-1570)", () => {
  it("renders no action bar for a provider with no declared actions (e.g. plain text)", async () => {
    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.txt", path: "/a.txt", extension: "txt" }),
      loadText: async () => "hello",
    });
    await waitFor(() => expect(container.querySelector(".preview-text")).toBeTruthy());
    expect(container.querySelector('[data-testid="preview-action-bar"]')).toBeNull();
  });

  it("renders the jwt provider's declared actions once claims/header are loaded", async () => {
    jwtPreviewMock.mockResolvedValueOnce(ok(validToken));
    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.jwt", path: "/a.jwt", extension: "jwt" }),
    });

    await waitFor(() => expect(container.querySelector('[data-testid="preview-action-bar"]')).toBeTruthy());
    const bar = container.querySelector('[data-testid="preview-action-bar"]')!;
    const labels = [...bar.querySelectorAll("button")].map((b) => b.textContent?.trim());
    expect(labels).toEqual(["Copy claims", "Copy header"]);
  });

  it("does not render an action for an empty value (enabled(ctx) gating)", async () => {
    // header_json null → the "Copy header" action's `enabled(ctx)` guard (`!!ctx.values["copy-header"]`)
    // filters it out, proving `visibleActions` gates on enablement, not just declaration.
    jwtPreviewMock.mockResolvedValueOnce(ok({ ...validToken, header_json: null }));
    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.jwt", path: "/a.jwt", extension: "jwt" }),
    });

    await waitFor(() => expect(container.querySelector('[data-testid="preview-action-bar"]')).toBeTruthy());
    const bar = container.querySelector('[data-testid="preview-action-bar"]')!;
    const labels = [...bar.querySelectorAll("button")].map((b) => b.textContent?.trim());
    expect(labels).toEqual(["Copy claims"]);
  });

  it("clicking a declared action's button runs it — copies the claims JSON to the clipboard", async () => {
    jwtPreviewMock.mockResolvedValueOnce(ok(validToken));
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });

    render(PreviewPane, { entry: entry({ name: "a.jwt", path: "/a.jwt", extension: "jwt" }) });

    const copyClaims = await waitFor(() => screen.getByRole("button", { name: /Copy claims/ }));
    await fireEvent.click(copyClaims);

    expect(writeText).toHaveBeenCalledWith(validToken.payload_json);
  });

  it("clicking the second action copies the header JSON, independently of the first", async () => {
    jwtPreviewMock.mockResolvedValueOnce(ok(validToken));
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });

    render(PreviewPane, { entry: entry({ name: "a.jwt", path: "/a.jwt", extension: "jwt" }) });

    const copyHeader = await waitFor(() => screen.getByRole("button", { name: /Copy header/ }));
    await fireEvent.click(copyHeader);

    expect(writeText).toHaveBeenCalledWith(validToken.header_json);
  });
});
