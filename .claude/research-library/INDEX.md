# Research Library — Index

One line per research entry, newest-relevant first. This is the fast-scan surface the Librarian and
Product Manager search **before** dispatching a fresh Researcher — a hit here means research reused, not
re-run. Format: `- [title](entries/slug.md) — date · tags · one-line finding`. Schema + protocol in
[README.md](README.md).

- [How do we run a headless GUI smoke test that drives the real built Tauri app in CI?](entries/headless-gui-smoke-test-tauri-driver.md) — 2026-07-25 · gui-testing, e2e, tauri-driver, webdriver, webdriverio, ci, headless · Use `tauri-driver` + WebdriverIO against the `tauri build` release binary on Windows; assert `--open <tmpdir>` navigated via the `[aria-current="page"]` breadcrumb (Linux via xvfb later; macOS unsupported).
- [Should large/slow producers stream in batches or return one blocking Vec?](entries/streaming-vs-blocking-bulk-payloads.md) — 2026-07-25 · streaming, tauri, ipc-channel, performance · Stream in batches over an `ipc::Channel` backed by a shared walker; codified in `docs/design/STREAMING.md`.
