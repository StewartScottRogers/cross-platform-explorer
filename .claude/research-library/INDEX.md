# Research Library — Index

One line per research entry, newest-relevant first. This is the fast-scan surface the Librarian and
Product Manager search **before** dispatching a fresh Researcher — a hit here means research reused, not
re-run. Format: `- [title](entries/slug.md) — date · tags · one-line finding`. Schema + protocol in
[README.md](README.md).

- [Should large/slow producers stream in batches or return one blocking Vec?](entries/streaming-vs-blocking-bulk-payloads.md) — 2026-07-25 · streaming, tauri, ipc-channel, performance · Stream in batches over an `ipc::Channel` backed by a shared walker; codified in `docs/design/STREAMING.md`.
