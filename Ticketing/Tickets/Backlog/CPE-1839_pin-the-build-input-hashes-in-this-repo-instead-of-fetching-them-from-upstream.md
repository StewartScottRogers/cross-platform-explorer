---
id: CPE-1839
title: "Security: pin the build-input hashes in this repo instead of fetching them from upstream"
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

The release build downloads five prebuilt binaries — three pdfium archives and two ffmpeg archives —
and CPE-1764 added format, size and (for ffmpeg) checksum verification to them. That work is right and
shipped. This is the layer above it.

**The ffmpeg checksum has no independent trust anchor.** `release-sidecar.yml:322` fetches
`checksums.sha256` from the same host, the same release, over the same TLS channel, with the same
`curl`, as the artifact it verifies. Anyone able to serve a malicious `ffmpeg-….zip` — a compromised
upstream release, a TLS-terminating proxy on the runner, a poisoned CDN edge — serves a matching
checksum in the same breath. So it detects **corruption and truncation**, which is worth having, and
not **substitution**.

**Tag pinning does not pin content.** `PDFIUM_TAG: "chromium/7961"` and
`FFMPEG_BUILD_TAG: "autobuild-2026-07-31-14-10"` are literal and nothing floats — but GitHub release
assets are *mutable within a tag*. The owner can replace both the asset and the checksum file under the
same tag and every pin still matches.

**And the stronger mechanism is the one we are not using.** bblanchon publishes a Sigstore attestation
bundle for pdfium (`pdfium-attestation.json`, Fulcio-issued cert, verified live during the CPE-1764
review) — an independent trust anchor giving real authenticity. BtbN publishes only the same-origin
sha256. CPE-1764's record initially framed pdfium as the *weaker* case for lacking a plain checksum;
it is the opposite, and that framing was corrected there.

## Acceptance criteria

- [ ] The expected sha256 of every pinned asset is **committed to this repo** and compared against the
      downloaded body. That is what converts the check from integrity to authenticity: the anchor moves
      from the server we are downloading from to the repo we control.
- [ ] pdfium's Sigstore attestation is verified — `gh attestation verify` or the equivalent — rather than
      relying on the size/format checks alone. Say what identity it is verified against, not just that a
      bundle exists.
- [ ] Bumping a pin is a deliberate two-part edit: the tag **and** the hash. That is the point, not an
      inconvenience — a tag bump that silently accepts new bytes is the hole this closes. Document the
      bump procedure next to the pins so the next person does not delete the hash to make CI pass.
- [ ] Every guard aborts, and a test proves it. CPE-1764's reviewer found that replacing every `exit 1`
      in that workflow with a warning left all its guard tests green; whatever lands here must not repeat
      that shape.
- [ ] The workflow comments state accurately what each layer defends against — format, size, integrity,
      authenticity — with no layer claiming more than it delivers.

## Notes

Split out of CPE-1764 by the Foreman after the reviewer answered the provenance question. CPE-1764's own
scope — reject a wrong or damaged body — is complete and correct; this is the authenticity layer above it.

The one-sentence summary of what ships today, worth carrying into this ticket so nobody re-derives it:
*the guard defends the release pipeline against a wrong or damaged body — an HTML error page, a
captive-portal redirect, an empty response, an early truncation, and for the two ffmpeg assets any
corruption past the 64KiB floor — but not against a determined attacker, because the checksum comes from
the same host, the same release, and the same connection as the artifact it is checking.*

Related: CPE-1824 (hang-hardening in the same two release workflows, including the new un-timed `curl` at
`release-sidecar.yml:322`).
