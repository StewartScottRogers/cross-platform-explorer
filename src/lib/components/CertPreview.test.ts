import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import CertPreview from "./CertPreview.svelte";

// CPE-1422 (epic CPE-1417): jsdom render-spec for the certificate/CSR/key preview view, wiring the
// CPE-1419 `cert_decode` backend command into a standalone component (same mocking recipe as
// JwtPreview.test.ts / DataBrowser.test.ts: mock `../bindings.gen`'s `commands` object). Exercises the
// shapes documented in `samples/crypto/README.md` (self-signed-rsa.pem, expired.pem, request.csr,
// public-key.pem, self-signed-rsa.key).

const { certDecodeMock } = vi.hoisted(() => ({ certDecodeMock: vi.fn() }));

vi.mock("../bindings.gen", () => ({
  commands: { certDecode: certDecodeMock },
}));

interface KeyInfo { algorithm: string; size_bits: number | null; curve: string | null }
interface CertificateInfo {
  subject: string; issuer: string; serial: string; version: string;
  not_before: string; not_after: string; expired: boolean; not_yet_valid: boolean;
  signature_algorithm: string; public_key: KeyInfo; subject_alt_names: string[];
  is_ca: boolean; key_usage: string[]; extended_key_usage: string[];
  sha256_fingerprint: string; sha1_fingerprint: string;
}
interface CsrInfo { subject: string; requested_sans: string[]; public_key: KeyInfo }
interface CertPreviewData {
  kind: string | null; encoding: string | null;
  certificate: CertificateInfo | null; csr: CsrInfo | null;
  public_key: KeyInfo | null; private_key: KeyInfo | null; error: string | null;
}

function ok(data: CertPreviewData) {
  return { status: "ok" as const, data };
}

const rsaCert: CertificateInfo = {
  subject: "CN=sample-rsa.cpe.local",
  issuer: "CN=sample-rsa.cpe.local",
  serial: "a1b2c3d4",
  version: "v3",
  not_before: "2026-01-01T00:00:00Z",
  not_after: "2027-01-01T00:00:00Z",
  expired: false,
  not_yet_valid: false,
  signature_algorithm: "sha256WithRSAEncryption",
  public_key: { algorithm: "RSA", size_bits: 2048, curve: null },
  subject_alt_names: ["sample-rsa.cpe.local", "www.sample-rsa.cpe.local"],
  is_ca: false,
  key_usage: ["digitalSignature", "keyEncipherment"],
  extended_key_usage: ["serverAuth"],
  sha256_fingerprint: "deadbeef".repeat(8),
  sha1_fingerprint: "cafebabe".repeat(5),
};

const expiredCert: CertificateInfo = {
  ...rsaCert,
  subject: "CN=expired.cpe.local",
  not_before: "2020-01-01T00:00:00Z",
  not_after: "2021-01-01T00:00:00Z",
  expired: true,
  subject_alt_names: ["expired.cpe.local"],
  public_key: { algorithm: "EC", size_bits: 256, curve: "prime256v1" },
};

const emptyPreview = (): CertPreviewData => ({
  kind: null, encoding: null, certificate: null, csr: null, public_key: null, private_key: null, error: null,
});

beforeEach(() => {
  certDecodeMock.mockReset();
});

describe("CertPreview (CPE-1422)", () => {
  it("renders certificate fields, SANs, key usage, and fingerprints for a valid RSA cert", async () => {
    certDecodeMock.mockResolvedValueOnce(
      ok({ ...emptyPreview(), kind: "certificate", encoding: "PEM", certificate: rsaCert }),
    );

    const { container } = render(CertPreview, { path: "/x/self-signed-rsa.pem" });

    // Self-signed: subject and issuer are the same DN, so both appear.
    await waitFor(() => expect(screen.getAllByText("CN=sample-rsa.cpe.local").length).toBe(2));
    expect(certDecodeMock).toHaveBeenCalledWith("/x/self-signed-rsa.pem");
    expect(screen.getByText(/RSA — 2048-bit/)).toBeTruthy(); // public-key algo+size row
    expect(screen.getByText("sha256WithRSAEncryption")).toBeTruthy(); // signature-algorithm row
    expect(screen.getByText("sample-rsa.cpe.local")).toBeTruthy();
    expect(screen.getByText("www.sample-rsa.cpe.local")).toBeTruthy();
    expect(screen.getByText("digitalSignature")).toBeTruthy();
    expect(screen.getByText("serverAuth")).toBeTruthy();
    expect(screen.getByText(rsaCert.sha256_fingerprint)).toBeTruthy();
    expect(screen.getByText(rsaCert.sha1_fingerprint)).toBeTruthy();
    // Not expired — no badge.
    expect(container.querySelector('[data-testid="cert-expired"]')).toBeNull();
    expect(screen.getByText(/does not verify trust or a chain/)).toBeTruthy();
  });

  it("shows the EXPIRED badge for a certificate whose validity window is in the past", async () => {
    certDecodeMock.mockResolvedValueOnce(
      ok({ ...emptyPreview(), kind: "certificate", encoding: "PEM", certificate: expiredCert }),
    );

    const { container } = render(CertPreview, { path: "/x/expired.pem" });

    await waitFor(() => expect(container.querySelector('[data-testid="cert-expired"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="cert-expired"]')!.textContent).toMatch(/EXPIRED/);
    // Non-RSA algorithm decode path (CPE-1419's EC coverage) — curve rendered.
    expect(screen.getByText(/prime256v1/)).toBeTruthy();
  });

  it("renders a CSR's requested subject and SANs", async () => {
    certDecodeMock.mockResolvedValueOnce(
      ok({
        ...emptyPreview(),
        kind: "csr",
        encoding: "PEM",
        csr: {
          subject: "CN=csr.sample.cpe.local",
          requested_sans: ["csr.sample.cpe.local"],
          public_key: { algorithm: "EC", size_bits: 256, curve: "prime256v1" },
        },
      }),
    );

    render(CertPreview, { path: "/x/request.csr" });

    await waitFor(() => expect(screen.getByText("CN=csr.sample.cpe.local")).toBeTruthy());
    expect(screen.getByText("csr.sample.cpe.local")).toBeTruthy();
  });

  it("renders a standalone public key's algorithm + curve", async () => {
    certDecodeMock.mockResolvedValueOnce(
      ok({
        ...emptyPreview(),
        kind: "public_key",
        encoding: "PEM",
        public_key: { algorithm: "EC", size_bits: 256, curve: "prime256v1" },
      }),
    );

    render(CertPreview, { path: "/x/public-key.pem" });
    await waitFor(() => expect(screen.getByText(/EC \(prime256v1\)/)).toBeTruthy());
  });

  it("shows ONLY algorithm + size for a private key file — never key material", async () => {
    certDecodeMock.mockResolvedValueOnce(
      ok({
        ...emptyPreview(),
        kind: "private_key",
        encoding: "PEM",
        private_key: { algorithm: "RSA", size_bits: 2048, curve: null },
      }),
    );

    const { container } = render(CertPreview, { path: "/x/self-signed-rsa.key" });

    await waitFor(() => expect(screen.getByText(/RSA — 2048-bit/)).toBeTruthy());
    expect(screen.getByText(/only the algorithm and size/i)).toBeTruthy();
    // No hex blob / PEM body anywhere near key material — the algorithm row is the whole story.
    expect(container.textContent).not.toMatch(/BEGIN (RSA )?PRIVATE KEY/);
  });

  it("shows a clean decode-error banner for a malformed file, without crashing", async () => {
    certDecodeMock.mockResolvedValueOnce(
      ok({ ...emptyPreview(), encoding: "DER", error: "not a recognizable certificate, CSR, public key, or private key (DER)" }),
    );

    const { container } = render(CertPreview, { path: "/x/garbage.der" });

    await waitFor(() => expect(container.querySelector('[data-testid="cert-decode-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="cert-decode-error"]')!.textContent).toMatch(/not a recognizable/);
  });

  it("shows a load-error state when the invoke call itself fails, without crashing", async () => {
    certDecodeMock.mockRejectedValueOnce(new Error("file too large"));

    const { container } = render(CertPreview, { path: "/x/huge.pem" });

    await waitFor(() => expect(container.querySelector('[data-testid="cert-load-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="cert-load-error"]')!.textContent).toContain("file too large");
  });

  it("copies a fingerprint to the clipboard on click", async () => {
    certDecodeMock.mockResolvedValueOnce(
      ok({ ...emptyPreview(), kind: "certificate", encoding: "PEM", certificate: rsaCert }),
    );
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });

    render(CertPreview, { path: "/x/self-signed-rsa.pem" });
    await waitFor(() => expect(screen.getByText(rsaCert.sha256_fingerprint)).toBeTruthy());

    await fireEvent.click(screen.getAllByRole("button", { name: /Copy/ })[0]);
    expect(writeText).toHaveBeenCalledWith(rsaCert.sha256_fingerprint);
    await waitFor(() => expect(screen.getByText("Copied")).toBeTruthy());
  });
});
