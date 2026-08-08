import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import IcalPreview from "./IcalPreview.svelte";

// CPE-1435 (epic CPE-1433): jsdom render-spec for the .ics calendar preview view, wiring the CPE-1435
// `ical_preview` backend command into a standalone component (mirrors EmailPreview.test.ts's recipe of
// mocking `../bindings.gen`'s `commands` object directly, since that's the module IcalPreview imports).

const { icalPreviewMock } = vi.hoisted(() => ({ icalPreviewMock: vi.fn() }));

vi.mock("../bindings.gen", () => ({
  commands: { icalPreview: icalPreviewMock },
}));

interface IcalEvent {
  component: string;
  summary: string | null;
  dtstart: string | null;
  dtstart_raw: string | null;
  dtend: string | null;
  all_day: boolean;
  location: string | null;
  description: string | null;
  organizer: string | null;
  attendees: string[];
  status: string | null;
  recurrence: string | null;
  rrule_raw: string | null;
  uid: string | null;
}
interface IcalPreviewData {
  calendar_name: string | null;
  method: string | null;
  events: IcalEvent[];
  error: string | null;
}

function ok(data: IcalPreviewData) {
  return { status: "ok" as const, data };
}

function event(over: Partial<IcalEvent> = {}): IcalEvent {
  return {
    component: "VEVENT",
    summary: "Weekly sync",
    dtstart: "2026-08-10T15:00:00Z",
    dtstart_raw: "20260810T150000Z",
    dtend: "2026-08-10T15:30:00Z",
    all_day: false,
    location: "Room 42",
    description: "A recurring sync.",
    organizer: "Alice Example",
    attendees: ["Bob Example", "carol@example.com"],
    status: "CONFIRMED",
    recurrence: "Weekly on Mon, 12 times",
    rrule_raw: "FREQ=WEEKLY;BYDAY=MO;COUNT=12",
    uid: "evt-1@example.com",
    ...over,
  };
}

const base: IcalPreviewData = {
  calendar_name: "CPE Sample Calendar",
  method: "REQUEST",
  events: [event()],
  error: null,
};

beforeEach(() => {
  icalPreviewMock.mockReset();
});

describe("IcalPreview (CPE-1435)", () => {
  it("renders the calendar banner and an event card with when/where/who/recurrence", async () => {
    icalPreviewMock.mockResolvedValueOnce(ok(base));

    const { container } = render(IcalPreview, { path: "/x/invite.ics" });

    await waitFor(() => expect(screen.getByText("Weekly sync")).toBeTruthy());
    expect(icalPreviewMock).toHaveBeenCalledWith("/x/invite.ics");
    // Banner shows the calendar name + method.
    expect(container.querySelector('[data-testid="ical-calname"]')?.textContent).toContain("CPE Sample Calendar");
    expect(container.querySelector('[data-testid="ical-method"]')?.textContent).toContain("REQUEST");
    // When = start – end.
    expect(container.querySelector('[data-testid="ical-when"]')?.textContent).toContain("2026-08-10T15:00:00Z");
    expect(container.querySelector('[data-testid="ical-when"]')?.textContent).toContain("2026-08-10T15:30:00Z");
    // Where + organizer.
    expect(container.querySelector('[data-testid="ical-where"]')?.textContent).toContain("Room 42");
    expect(container.querySelector('[data-testid="ical-organizer"]')?.textContent).toContain("Alice Example");
    // Attendee pills reflow row.
    const atts = container.querySelector('[data-testid="ical-attendees"]');
    expect(atts?.textContent).toContain("Bob Example");
    expect(atts?.textContent).toContain("carol@example.com");
    // Recurrence note.
    expect(container.querySelector('[data-testid="ical-recurrence"]')?.textContent).toContain("Weekly on Mon, 12 times");
    // The read-only disclaimer.
    expect(screen.getByText(/Calendar viewer — read-only/i)).toBeTruthy();
  });

  it("flags an all-day event and renders multiple event cards", async () => {
    icalPreviewMock.mockResolvedValueOnce(
      ok({
        ...base,
        events: [
          event({ summary: "Sync" }),
          event({ summary: "Holiday", all_day: true, dtstart: "2026-12-25", dtend: "2026-12-26", attendees: [], recurrence: null, description: null }),
        ],
      }),
    );

    const { container } = render(IcalPreview, { path: "/x/multi.ics" });

    await waitFor(() => expect(container.querySelectorAll('[data-testid="ical-event"]').length).toBe(2));
    expect(container.querySelector('[data-testid="ical-allday"]')?.textContent).toMatch(/all day/i);
  });

  it("shows a VTODO badge for a task component", async () => {
    icalPreviewMock.mockResolvedValueOnce(
      ok({ ...base, events: [event({ component: "VTODO", summary: "Do the thing" })] }),
    );

    const { container } = render(IcalPreview, { path: "/x/todo.ics" });

    await waitFor(() => expect(container.querySelector('[data-testid="ical-badge"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="ical-badge"]')!.textContent).toMatch(/task/i);
  });

  it("shows a clean decode-error note for a non-calendar file, without crashing", async () => {
    icalPreviewMock.mockResolvedValueOnce(
      ok({ calendar_name: null, method: null, events: [], error: "not a valid iCalendar file: no BEGIN:VCALENDAR found" }),
    );

    const { container } = render(IcalPreview, { path: "/x/broken.ics" });

    await waitFor(() => expect(container.querySelector('[data-testid="ical-decode-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="ical-decode-error"]')!.textContent).toMatch(/no BEGIN:VCALENDAR/);
  });

  it("shows a load-error state when the invoke call itself fails, without crashing", async () => {
    icalPreviewMock.mockRejectedValueOnce(new Error("permission denied"));

    const { container } = render(IcalPreview, { path: "/x/locked.ics" });

    await waitFor(() => expect(container.querySelector('[data-testid="ical-load-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="ical-load-error"]')!.textContent).toContain("permission denied");
  });

  it("reloads when the previewed path changes", async () => {
    icalPreviewMock.mockResolvedValueOnce(ok(base));
    icalPreviewMock.mockResolvedValueOnce(ok({ ...base, events: [event({ summary: "Second event" })] }));

    const { rerender } = render(IcalPreview, { path: "/x/a.ics" });
    await waitFor(() => expect(screen.getByText("Weekly sync")).toBeTruthy());

    await rerender({ path: "/x/b.ics" });
    await waitFor(() => expect(screen.getByText("Second event")).toBeTruthy());
    expect(icalPreviewMock).toHaveBeenCalledTimes(2);
    expect(icalPreviewMock).toHaveBeenNthCalledWith(2, "/x/b.ics");
  });
});
