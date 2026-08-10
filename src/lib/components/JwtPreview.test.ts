import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import JwtPreview from "./JwtPreview.svelte";

// CPE-1422 (epic CPE-1417): jsdom render-spec for the JWT preview view, wiring the CPE-1418
// `jwt_preview` backend command into a standalone component (mirrors DataBrowser.test.ts's recipe of
// mocking `../bindings.gen`'s `commands` object directly, since that's the module JwtPreview imports).
// Exercises the shapes documented in `samples/crypto/README.md` (hs256-valid / expired / alg-none).

const { jwtPreviewMock } = vi.hoisted(() => ({ jwtPreviewMock: vi.fn() }));

vi.mock("../bindings.gen", () => ({
  commands: { jwtPreview: jwtPreviewMock },
}));

interface JwtClaimTime { raw: number; rfc3339: string }
interface JwtPreviewData {
  alg: string | null;
  typ: string | null;
  kid: string | null;
  header_json: string | null;
  payload_json: string | null;
  exp: JwtClaimTime | null;
  iat: JwtClaimTime | null;
  nbf: JwtClaimTime | null;
  expired: boolean | null;
  not_yet_valid: boolean | null;
  signature_present: boolean;
  signature_len: number;
  error: string | null;
}

function ok(data: JwtPreviewData) {
  return { status: "ok" as const, data };
}

const validToken: JwtPreviewData = {
  alg: "HS256",
  typ: "JWT",
  kid: null,
  header_json: '{\n  "alg": "HS256",\n  "typ": "JWT"\n}',
  payload_json: '{\n  "sub": "1234567890",\n  "name": "Ada Lovelace"\n}',
  exp: { raw: 2_000_000_000, rfc3339: "2033-05-18T03:33:20Z" },
  iat: { raw: 1_700_000_000, rfc3339: "2023-11-14T22:13:20Z" },
  nbf: null,
  expired: false,
  not_yet_valid: null,
  signature_present: true,
  signature_len: 32,
  error: null,
};

const expiredToken: JwtPreviewData = {
  ...validToken,
  exp: { raw: 1_600_003_600, rfc3339: "2020-09-13T13:26:40Z" },
  expired: true,
};

const algNoneToken: JwtPreviewData = {
  alg: "none",
  typ: "JWT",
  kid: null,
  header_json: '{\n  "alg": "none",\n  "typ": "JWT"\n}',
  payload_json: '{\n  "sub": "0000000000"\n}',
  exp: null,
  iat: null,
  nbf: null,
  expired: null,
  not_yet_valid: null,
  signature_present: false,
  signature_len: 0,
  error: null,
};

beforeEach(() => {
  jwtPreviewMock.mockReset();
});

describe("JwtPreview (CPE-1422)", () => {
  it("renders header + claims + a present-signature indicator for a valid token", async () => {
    jwtPreviewMock.mockResolvedValueOnce(ok(validToken));

    const { container } = render(JwtPreview, { path: "/x/hs256-valid.jwt" });

    await waitFor(() => expect(screen.getByText("HS256")).toBeTruthy());
    expect(jwtPreviewMock).toHaveBeenCalledWith("/x/hs256-valid.jwt");
    expect(screen.getByText("JWT")).toBeTruthy();
    expect(container.querySelector('[data-testid="jwt-signature"]')?.textContent).toContain("Signature present");
    expect(container.querySelector('[data-testid="jwt-signature"]')?.textContent).toContain("32 bytes");
    expect(screen.getByText(/Ada Lovelace/)).toBeTruthy();
    // Not expired — no badge.
    expect(container.querySelector('[data-testid="jwt-expired"]')).toBeNull();
    // The viewer-not-verifier disclaimer is always shown.
    expect(screen.getByText(/does not verify the signature/)).toBeTruthy();
  });

  it("shows the EXPIRED badge when exp is in the past", async () => {
    jwtPreviewMock.mockResolvedValueOnce(ok(expiredToken));

    const { container } = render(JwtPreview, { path: "/x/expired.jwt" });

    await waitFor(() => expect(container.querySelector('[data-testid="jwt-expired"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="jwt-expired"]')!.textContent).toMatch(/EXPIRED/);
  });

  it("shows alg:none as unsigned with no signature-present indicator", async () => {
    jwtPreviewMock.mockResolvedValueOnce(ok(algNoneToken));

    const { container } = render(JwtPreview, { path: "/x/alg-none.jwt" });

    await waitFor(() => expect(screen.getByText("none")).toBeTruthy());
    const sig = container.querySelector('[data-testid="jwt-signature"]');
    expect(sig?.textContent).toContain("No signature");
    expect(sig?.textContent).toContain("alg: none");
  });

  it("shows a clean decode-error banner for a malformed token, without crashing", async () => {
    jwtPreviewMock.mockResolvedValueOnce(
      ok({
        alg: null,
        typ: null,
        kid: null,
        header_json: null,
        payload_json: null,
        exp: null,
        iat: null,
        nbf: null,
        expired: null,
        not_yet_valid: null,
        signature_present: false,
        signature_len: 0,
        error: "malformed JWT: expected 3 dot-separated segments (header.payload.signature), found 2",
      }),
    );

    const { container } = render(JwtPreview, { path: "/x/broken.jwt" });

    await waitFor(() => expect(container.querySelector('[data-testid="jwt-decode-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="jwt-decode-error"]')!.textContent).toMatch(/malformed JWT/);
  });

  it("shows a load-error state when the invoke call itself fails (e.g. unreadable file), without crashing", async () => {
    jwtPreviewMock.mockRejectedValueOnce(new Error("permission denied"));

    const { container } = render(JwtPreview, { path: "/x/locked.jwt" });

    await waitFor(() => expect(container.querySelector('[data-testid="jwt-load-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="jwt-load-error"]')!.textContent).toContain("permission denied");
  });

  it("reports the decoded claims/header up via onValues, keyed for the action bar (CPE-1570)", async () => {
    // The inline copy buttons moved to PreviewPane's generic action bar (CPE-1570, epic CPE-1568) — this
    // component's job now is just to report its two copyable JSON blobs, keyed to match the `jwt`
    // provider's declared action ids (`copy-claims` / `copy-header`) in `preview/provider.ts`.
    jwtPreviewMock.mockResolvedValueOnce(ok(validToken));
    const onValues = vi.fn();

    render(JwtPreview, { path: "/x/hs256-valid.jwt", onValues });
    await waitFor(() => expect(screen.getByText("HS256")).toBeTruthy());

    await waitFor(() =>
      expect(onValues).toHaveBeenCalledWith({
        "copy-claims": validToken.payload_json,
        "copy-header": validToken.header_json,
      }),
    );
    // No inline Copy buttons render anymore — they live in the pane's action bar instead.
    expect(screen.queryAllByRole("button", { name: /Copy/ }).length).toBe(0);
  });

  it("reports empty values while loading/on error, so a stale action never lingers (CPE-1570)", async () => {
    jwtPreviewMock.mockRejectedValueOnce(new Error("nope"));
    const onValues = vi.fn();

    render(JwtPreview, { path: "/x/broken.jwt", onValues });

    await waitFor(() => expect(onValues).toHaveBeenCalledWith({ "copy-claims": "", "copy-header": "" }));
  });

  it("reloads when the previewed path changes", async () => {
    jwtPreviewMock.mockResolvedValueOnce(ok(validToken));
    jwtPreviewMock.mockResolvedValueOnce(ok(algNoneToken));

    const { rerender } = render(JwtPreview, { path: "/x/a.jwt" });
    await waitFor(() => expect(screen.getByText("HS256")).toBeTruthy());

    await rerender({ path: "/x/b.jwt" });
    await waitFor(() => expect(screen.getByText("none")).toBeTruthy());
    expect(jwtPreviewMock).toHaveBeenCalledTimes(2);
    expect(jwtPreviewMock).toHaveBeenNthCalledWith(2, "/x/b.jwt");
  });
});
