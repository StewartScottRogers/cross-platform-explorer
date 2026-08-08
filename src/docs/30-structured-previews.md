---
title: Structured File Previews
order: 30
category: Explorer
categoryOrder: 2
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
