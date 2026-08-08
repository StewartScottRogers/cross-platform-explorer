import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import VcardPreview from "./VcardPreview.svelte";

// CPE-1436 (epic CPE-1433): jsdom render-spec for the .vcf contact preview view, wiring the CPE-1436
// `vcard_preview` backend command into a standalone component (mirrors EmailPreview.test.ts's recipe of
// mocking `../bindings.gen`'s `commands` object directly, since that's the module VcardPreview imports).

const { vcardPreviewMock } = vi.hoisted(() => ({ vcardPreviewMock: vi.fn() }));

vi.mock("../bindings.gen", () => ({
  commands: { vcardPreview: vcardPreviewMock },
}));

interface VcardPhone { number: string; types: string[] }
interface VcardEmail { address: string; types: string[] }
interface VcardAddress { label: string; types: string[] }
interface VcardEntry {
  formatted_name: string | null;
  name: string | null;
  org: string | null;
  title: string | null;
  phones: VcardPhone[];
  emails: VcardEmail[];
  addresses: VcardAddress[];
  urls: string[];
  birthday: string | null;
  has_photo: boolean;
  photo_size: number;
}
interface VcardPreviewData {
  cards: VcardEntry[];
  error: string | null;
}

function ok(data: VcardPreviewData) {
  return { status: "ok" as const, data };
}

function card(over: Partial<VcardEntry> = {}): VcardEntry {
  return {
    formatted_name: "Dr. Alice P. Example",
    name: "Dr. Alice Pat Example PhD",
    org: "Acme Corporation, Research Division",
    title: "Principal Engineer",
    phones: [
      { number: "+1-555-0100", types: ["work", "voice"] },
      { number: "+1-555-0199", types: ["cell"] },
    ],
    emails: [{ address: "alice@example.com", types: ["internet", "work"] }],
    addresses: [{ label: "123 Main Street, Suite 400, Springfield, IL, 62704, USA", types: ["work"] }],
    urls: ["https://example.com/~alice"],
    birthday: "1985-04-07",
    has_photo: true,
    photo_size: 24,
    ...over,
  };
}

const base: VcardPreviewData = { cards: [card()], error: null };

beforeEach(() => {
  vcardPreviewMock.mockReset();
});

describe("VcardPreview (CPE-1436)", () => {
  it("renders a contact card with name/org/title, phone/email/address rows and TYPE pills", async () => {
    vcardPreviewMock.mockResolvedValueOnce(ok(base));

    const { container } = render(VcardPreview, { path: "/x/contact.vcf" });

    await waitFor(() => expect(screen.getByText("Dr. Alice P. Example")).toBeTruthy());
    expect(vcardPreviewMock).toHaveBeenCalledWith("/x/contact.vcf");
    // Org/title sub-heading.
    expect(container.querySelector('[data-testid="vcard-sub"]')?.textContent).toContain("Principal Engineer");
    expect(container.querySelector('[data-testid="vcard-sub"]')?.textContent).toContain("Acme Corporation");
    // Phone rows + TYPE pills.
    const tels = container.querySelectorAll('[data-testid="vcard-tel"]');
    expect(tels.length).toBe(2);
    expect(tels[0].textContent).toContain("+1-555-0100");
    expect(tels[0].textContent).toContain("work");
    expect(tels[0].textContent).toContain("voice");
    // Email + address + url + birthday.
    expect(container.querySelector('[data-testid="vcard-email"]')?.textContent).toContain("alice@example.com");
    expect(container.querySelector('[data-testid="vcard-adr"]')?.textContent).toContain("Springfield");
    expect(container.querySelector('[data-testid="vcard-url"]')?.textContent).toContain("example.com/~alice");
    expect(container.querySelector('[data-testid="vcard-bday"]')?.textContent).toContain("1985-04-07");
    // The read-only disclaimer.
    expect(screen.getByText(/Contact viewer — read-only/i)).toBeTruthy();
  });

  it("shows a presence-only photo note (never image bytes)", async () => {
    vcardPreviewMock.mockResolvedValueOnce(ok(base));

    const { container } = render(VcardPreview, { path: "/x/photo.vcf" });

    await waitFor(() => expect(container.querySelector('[data-testid="vcard-photo"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="vcard-photo"]')!.textContent).toMatch(/photo present/i);
  });

  it("renders multiple cards and a count in the banner", async () => {
    vcardPreviewMock.mockResolvedValueOnce(
      ok({ cards: [card({ formatted_name: "First" }), card({ formatted_name: "Second" })], error: null }),
    );

    const { container } = render(VcardPreview, { path: "/x/multi.vcf" });

    await waitFor(() => expect(container.querySelectorAll('[data-testid="vcard-card"]').length).toBe(2));
    expect(container.querySelector('[data-testid="vcard-count"]')?.textContent).toContain("2 contacts");
  });

  it("falls back to a placeholder heading when a card has no name, without crashing", async () => {
    vcardPreviewMock.mockResolvedValueOnce(
      ok({ cards: [card({ formatted_name: null, name: null, has_photo: false })], error: null }),
    );

    const { container } = render(VcardPreview, { path: "/x/noname.vcf" });

    await waitFor(() => expect(container.querySelector('[data-testid="vcard-name"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="vcard-name"]')!.textContent).toMatch(/unnamed contact/i);
    // No photo note when the card has no photo.
    expect(container.querySelector('[data-testid="vcard-photo"]')).toBeNull();
  });

  it("shows a clean decode-error note for a non-vcard file, without crashing", async () => {
    vcardPreviewMock.mockResolvedValueOnce(
      ok({ cards: [], error: "not a valid vCard file: no BEGIN:VCARD block found" }),
    );

    const { container } = render(VcardPreview, { path: "/x/broken.vcf" });

    await waitFor(() => expect(container.querySelector('[data-testid="vcard-decode-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="vcard-decode-error"]')!.textContent).toMatch(/no BEGIN:VCARD/);
  });

  it("shows a load-error state when the invoke call itself fails, without crashing", async () => {
    vcardPreviewMock.mockRejectedValueOnce(new Error("permission denied"));

    const { container } = render(VcardPreview, { path: "/x/locked.vcf" });

    await waitFor(() => expect(container.querySelector('[data-testid="vcard-load-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="vcard-load-error"]')!.textContent).toContain("permission denied");
  });

  it("reloads when the previewed path changes", async () => {
    vcardPreviewMock.mockResolvedValueOnce(ok(base));
    vcardPreviewMock.mockResolvedValueOnce(ok({ cards: [card({ formatted_name: "Second Person" })], error: null }));

    const { rerender } = render(VcardPreview, { path: "/x/a.vcf" });
    await waitFor(() => expect(screen.getByText("Dr. Alice P. Example")).toBeTruthy());

    await rerender({ path: "/x/b.vcf" });
    await waitFor(() => expect(screen.getByText("Second Person")).toBeTruthy());
    expect(vcardPreviewMock).toHaveBeenCalledTimes(2);
    expect(vcardPreviewMock).toHaveBeenNthCalledWith(2, "/x/b.vcf");
  });
});
