---
id: CPE-1433
title: "EPIC: Structured file previews (.eml / .ics / .vcf)"
type: Epic
status: In Progress
priority: Medium
component: Full-stack
tags: [epic]
created: 2026-08-07
---
## Goal (workshift, PM-activated 2026-08-07)
Turn three common plain-text-mapped file types into **structured preview cards**, following the shipped
crypto-viewer pattern (CPE-1417: `jwt_preview.rs` → `jwt_preview` command → `JwtPreview.svelte` + jsdom test,
provider entry ordered before `text`). Today `.eml/.ics/.vcf` all fall through to the plain-text/"code"
provider; the closed CPE-079/092/093 shipped only that text mapping and **explicitly deferred** structured
parsing to "a future backend enhancement" — this epic is that enhancement.

## Why
`.eml` (mail export), `.ics` (calendar invite), `.vcf` (contact card) are mainstream files a general explorer
should summarize, not dump as raw MIME/`BEGIN:VCALENDAR` text. A headers/attendees/contact card is a real
capability, in the exact direction the crypto-viewer epic already took. Pure-Rust parsers, cargo + jsdom
verifiable, no heavy dep → respects PURPOSE.md fast/small/predictable.

## Children
- **CPE-1434** — `.eml` structured email preview (From/To/Cc/Subject/Date + MIME-part tree + attachment
  list + sanitized text body). Backend `email_preview.rs` + command + provider + `EmailPreview.svelte` + tests.
  *Build first (highest value; establishes the epic's shared wiring).*
- **CPE-1435** — `.ics` iCalendar preview (VEVENT/VTODO: SUMMARY/DTSTART/DTEND/LOCATION/ATTENDEE/RRULE →
  what/when/where/who card). Zero-dep RFC 5545 line-unfolding parser. *After 1434 merges (shared provider.ts /
  PreviewPane.svelte / lib.rs handler wiring).*
- **CPE-1436** — `.vcf` vCard preview (N/FN/ORG/TITLE/TEL/EMAIL/ADR + photo-presence → contact card). Zero-dep
  parser. *After 1434 merges.*

## Decisions
- Private/sensitive content: render as data only; never execute embedded HTML/scripts in an `.eml` body
  (sanitize to text — no remote resource loads, consistent with the CSP posture).
- Prefer zero-dep hand-rolled parsers for `.ics`/`.vcf` (simple line grammars). For `.eml`, a small pure-Rust
  MIME parse (hand-rolled or the MIT `mailparse` crate — Dependency Steward vets if proposed).
- Each child is independently shippable; 1435/1436 serialize after 1434 because they edit the SAME shared
  wiring files (provider ordering + PreviewPane import block + the `generate_handler!` list).

## Definition of Done
- `.eml/.ics/.vcf` render structured summary cards in the preview pane; malformed files degrade to the plain
  text/hex fallback (no panic). Parsers are pure + cargo-tested; provider selection + render specs jsdom-tested;
  docs + sectionDocs entries added (CPE-579). Unused ⇒ no cost.
