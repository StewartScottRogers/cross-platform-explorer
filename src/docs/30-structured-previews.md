---
title: Structured File Previews
order: 30
category: Previews & Media
categoryOrder: 6
---

# Structured File Previews

Some file types are far more useful shown as a **structured card** than as raw source or a hex dump.
When you select one of these files, the preview pane decodes it automatically — no dialog to open, no
tool to launch. Every viewer here is a **read-only decoder**: it shows you what a file contains and
never modifies it, executes it, or reaches out to the network on its behalf.

This page covers the structured previews under the "Structured previews" epic. (JWTs and certificates
have their own [JWT & Certificate Preview](26-crypto-preview) page.)

## Email preview (`.eml`)

Opens automatically for `.eml` files (RFC 822 / MIME email messages). The pane shows:

- **Header card** — **From**, **To**, **Cc**, **Subject**, and **Date**. International text in the
  headers (encoded with the MIME `=?utf-8?…?=` "encoded-word" scheme) is decoded to readable text, and
  the date is normalised to a clear `YYYY-MM-DDTHH:MM:SSZ` UTC timestamp.
- **Attachments** — a row of pills, one per attached part, each showing the filename and size. The row
  reflows onto more lines as needed.
- **Body** — the message's plain-text body, shown in a scrollable region. Content encoded for transport
  (base64 or quoted-printable) is decoded first.

### Safety: no HTML, no remote content

The email viewer **never renders HTML and never loads remote resources.** When a message has only an
HTML body, the viewer reduces it to plain text — dropping `<script>` and `<style>` blocks and stripping
all tags — before showing it. Because the result is plain text, there is nothing for the pane to fetch:
no tracking pixels, no remote images, no stylesheets, no scripts. A short note above the body reminds
you that an HTML message is being shown as sanitized text.

A malformed or truncated `.eml` file degrades gracefully — you still get whatever headers and body
could be parsed, with a small note explaining what couldn't — rather than an error or a crash.

## Calendar preview (`.ics`)

Opens automatically for `.ics` files (RFC 5545 iCalendar — invitations, subscriptions, and calendar
exports). The pane shows one **event card** per component (`VEVENT` meetings, `VTODO` tasks, `VJOURNAL`
notes) in the file:

- **Summary heading** — the event title, with a small badge for tasks and journal entries and an
  "All day" badge for date-only events.
- **When** — the start and (where present) end, humanised to a clear `YYYY-MM-DD` for all-day events or
  `YYYY-MM-DDTHH:MM:SSZ` for timed events. A floating local time that names a time zone (`TZID`) shows
  that zone alongside it.
- **Where** — the location.
- **Who** — the organizer, plus a row of attendee pills that reflows onto more lines as needed. Each
  attendee shows their display name where the file provides one, otherwise their address.
- **Repeats** — a plain-language summary of a recurrence rule (for example, "Weekly on Mon, Wed, 10
  times"). When a rule is too unusual to summarise, the raw rule text is shown instead.
- **Description** — the event's notes, shown in a scrollable region.

Nested alarms (`VALARM`) and time-zone definitions (`VTIMEZONE`) are handled correctly and never leak
their properties onto an event. A malformed or truncated `.ics` file degrades gracefully — you still get
whatever events could be parsed, with a note when none were found — rather than an error or a crash.

## Contact preview (`.vcf`)

Opens automatically for `.vcf` files (vCard 2.1 / 3.0 / 4.0 contact cards). A file may hold several
contacts; the pane shows one **contact card** each:

- **Name heading** — the formatted name, with the organisation and job title beneath it.
- **Phones, emails, addresses, URLs** — one row each, with reflowing **type pills** (`work`, `home`,
  `cell`, …) beside phones, emails, and postal addresses. Structured names and addresses are assembled
  into readable lines.
- **Birthday** — shown as recorded in the card.

### Safety: the photo is never fetched

When a contact carries an embedded **photo**, the card shows only a short "photo present" note with its
approximate size — the image bytes are **never** read out of the file into the preview or sent over the
app's internal channel. This keeps the contact preview light and consistent with the app's rule against
moving heavy binary blobs through a preview.

A malformed or truncated `.vcf` file degrades gracefully — you still get whatever contacts and fields
could be parsed, with a note when no contact block was found — rather than an error or a crash.
