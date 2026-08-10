---
title: Certificate Management
order: 28
category: Power Tools
categoryOrder: 7
---

# Certificate Management

Create self-signed certificates and issue certificates signed by an existing CA, right from the
explorer — no external `openssl`/`certutil` invocation needed. This is the write side of
[JWT & Certificate Preview](26-crypto-preview): that page covers the read-only decoder; this page covers
generating and signing.

## Creating a certificate

Right-click a **folder** (or empty space in a folder) and choose **Create certificate here…**, or run
**Create certificate…** from the command palette. The dialog generates a fresh keypair and a
self-signed X.509 certificate, then writes both PEM files to the chosen folder.

- **Common name (CN)** — the certificate's subject, e.g. `my-service.local`. Required.
- **Subject alternative names** — DNS names and IP addresses, entered as reflowing pill tags (type a
  value, press Enter or `,` to add it; Backspace on an empty field removes the last one).
- **Validity (days)** — how long the certificate is valid for, starting a few minutes in the past (a
  small clock-skew allowance).
- **Key type** — EC-P256 (default, fast, small), EC-P384, RSA-2048, or RSA-4096 for interop with
  systems that don't accept EC certificates.
- **This is a CA certificate** — sets the certificate's `BasicConstraints` CA flag so it can later sign
  other certificates (needed if you plan to use it with "Sign / issue from CSR…" below).
- **Output folder + filenames** — a native Browse picker chooses the folder; the certificate and key
  filenames default to `<common name>.pem` / `<common name>.key` and can be edited directly.

On success, both files land in the chosen folder and the listing refreshes to show them.

## Signing / issuing a certificate from a CSR

Right-click a `.csr` file and choose **Issue cert from this CSR…**, or right-click a cert-shaped file
(`.pem`/`.crt`/`.cer`/`.der`) and choose **Sign with this as CA…** — either opens the same dialog,
pre-filled with the clicked file as the CSR or the CA certificate respectively. It's also reachable with
nothing pre-filled via **Sign / issue certificate…** in the command palette.

- **CSR file** — the PKCS#10 certificate signing request to issue a certificate for.
- **CA certificate** and **CA private key** — the existing CA that signs the new certificate. The CA
  key is read only to sign — it's never written anywhere else or sent back over the app's IPC.
- **Validity (days)** — how long the issued certificate is valid for.
- **Output certificate** — where the issued PEM is written; defaults to `<CSR name>.crt` in the target
  folder.

Every path field has its own native Browse picker. On success, the issued certificate lands at the
chosen path and the listing refreshes to show it.

## Inspecting from the menu

Right-clicking a cert/CSR file also offers **Inspect**, and a `.jwt`/`.jws` file offers **Inspect
JWT** — both show the same decoded view described in [JWT & Certificate Preview](26-crypto-preview);
right-clicking already selects the row, so the decoder has already run by the time the menu opens. In
single-pane mode this brings the preview pane's decoded view forward. In dual-pane (commander) mode the
right pane occupies the preview slot, so **Inspect** opens the decode in a centered overlay instead —
Esc or clicking outside closes it — and works the same from either pane.

## Works from either pane

In dual-pane (commander) mode, every action above is **pane-aware**: right-clicking in the right pane
creates or signs into the right pane's own folder, not the left pane's — the same convention every other
pane-routed context-menu action in the app follows.

## Honest limits

Neither dialog performs any trust decision. A self-signed certificate is exactly that — self-signed;
nothing here checks a chain, submits to a public CA, or otherwise vouches for the certificate beyond
what you typed into the form. The CA key you point "Sign / issue from CSR…" at is trusted implicitly,
the same way any signing tool trusts the key you hand it.
