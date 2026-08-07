import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import EmailPreview from "./EmailPreview.svelte";

// CPE-1434 (epic CPE-1433): jsdom render-spec for the .eml email preview view, wiring the CPE-1434
// `email_preview` backend command into a standalone component (mirrors JwtPreview.test.ts's recipe of
// mocking `../bindings.gen`'s `commands` object directly, since that's the module EmailPreview imports).

const { emailPreviewMock } = vi.hoisted(() => ({ emailPreviewMock: vi.fn() }));

vi.mock("../bindings.gen", () => ({
  commands: { emailPreview: emailPreviewMock },
}));

interface MimePart { content_type: string; filename: string | null; size: number; is_attachment: boolean }
interface Attachment { filename: string; size: number; content_type: string }
interface EmailPreviewData {
  from: string | null;
  to: string[];
  cc: string[];
  subject: string | null;
  date: string | null;
  date_rfc3339: string | null;
  parts: MimePart[];
  attachments: Attachment[];
  body: string;
  body_is_html: boolean;
  body_truncated: boolean;
  error: string | null;
}

function ok(data: EmailPreviewData) {
  return { status: "ok" as const, data };
}

const base: EmailPreviewData = {
  from: "Alice <alice@example.com>",
  to: ["Bob <bob@example.com>", "carol@example.com"],
  cc: ["dave@example.com"],
  subject: "Héllo —",
  date: "Mon, 07 Aug 2026 09:30:00 +0000",
  date_rfc3339: "2026-08-07T09:30:00Z",
  parts: [
    { content_type: "text/plain", filename: null, size: 11, is_attachment: false },
    { content_type: "application/pdf", filename: "report.pdf", size: 20480, is_attachment: true },
  ],
  attachments: [{ filename: "report.pdf", size: 20480, content_type: "application/pdf" }],
  body: "Café time.",
  body_is_html: false,
  body_truncated: false,
  error: null,
};

beforeEach(() => {
  emailPreviewMock.mockReset();
});

describe("EmailPreview (CPE-1434)", () => {
  it("renders the header card, attachment pills and body for a normal message", async () => {
    emailPreviewMock.mockResolvedValueOnce(ok(base));

    const { container } = render(EmailPreview, { path: "/x/message.eml" });

    await waitFor(() => expect(screen.getByText("Héllo —")).toBeTruthy());
    expect(emailPreviewMock).toHaveBeenCalledWith("/x/message.eml");
    // Headers.
    expect(screen.getByText("Alice <alice@example.com>")).toBeTruthy();
    expect(container.querySelector('[data-testid="email-to"]')?.textContent).toContain("bob@example.com");
    expect(container.querySelector('[data-testid="email-cc"]')?.textContent).toContain("dave@example.com");
    // Humanized date preferred.
    expect(screen.getByText("2026-08-07T09:30:00Z")).toBeTruthy();
    // Attachment pill.
    const atts = container.querySelector('[data-testid="email-attachments"]');
    expect(atts?.textContent).toContain("report.pdf");
    // Body.
    expect(container.querySelector('[data-testid="email-body"]')?.textContent).toContain("Café time.");
    // The remote-content disclaimer is always shown.
    expect(screen.getByText(/remote content is not loaded/i)).toBeTruthy();
  });

  it("shows the 'shown as text' note when the body came from an HTML part", async () => {
    emailPreviewMock.mockResolvedValueOnce(
      ok({ ...base, body: "Hello world & welcome", body_is_html: true, attachments: [] }),
    );

    const { container } = render(EmailPreview, { path: "/x/html.eml" });

    await waitFor(() => expect(container.querySelector('[data-testid="email-html-note"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="email-html-note"]')!.textContent).toMatch(/shown as text/i);
    // No attachment section renders when there are none.
    expect(container.querySelector('[data-testid="email-attachments"]')).toBeNull();
  });

  it("shows an empty-body note for a message with no text body", async () => {
    emailPreviewMock.mockResolvedValueOnce(ok({ ...base, body: "", attachments: [] }));

    const { container } = render(EmailPreview, { path: "/x/nobody.eml" });

    await waitFor(() => expect(container.querySelector('[data-testid="email-empty-body"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="email-body"]')).toBeNull();
  });

  it("shows a clean decode-error banner for a malformed message, without crashing", async () => {
    emailPreviewMock.mockResolvedValueOnce(
      ok({
        from: null,
        to: [],
        cc: [],
        subject: null,
        date: null,
        date_rfc3339: null,
        parts: [],
        attachments: [],
        body: "",
        body_is_html: false,
        body_truncated: false,
        error: "not a valid email message: no RFC 822 headers found",
      }),
    );

    const { container } = render(EmailPreview, { path: "/x/broken.eml" });

    await waitFor(() => expect(container.querySelector('[data-testid="email-decode-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="email-decode-error"]')!.textContent).toMatch(/no RFC 822 headers/);
  });

  it("shows a load-error state when the invoke call itself fails, without crashing", async () => {
    emailPreviewMock.mockRejectedValueOnce(new Error("permission denied"));

    const { container } = render(EmailPreview, { path: "/x/locked.eml" });

    await waitFor(() => expect(container.querySelector('[data-testid="email-load-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="email-load-error"]')!.textContent).toContain("permission denied");
  });

  it("reloads when the previewed path changes", async () => {
    emailPreviewMock.mockResolvedValueOnce(ok(base));
    emailPreviewMock.mockResolvedValueOnce(ok({ ...base, subject: "Second message" }));

    const { rerender } = render(EmailPreview, { path: "/x/a.eml" });
    await waitFor(() => expect(screen.getByText("Héllo —")).toBeTruthy());

    await rerender({ path: "/x/b.eml" });
    await waitFor(() => expect(screen.getByText("Second message")).toBeTruthy());
    expect(emailPreviewMock).toHaveBeenCalledTimes(2);
    expect(emailPreviewMock).toHaveBeenNthCalledWith(2, "/x/b.eml");
  });
});
