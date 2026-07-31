# agent-board

A Cross-Platform Explorer sidecar (see `docs/adr/0001-sidecar-platform.md`).

- `src/main.rs` — the sidecar process. Depends only on `sidecar-contract`.
- `src/ui.rs` — the loopback HTTP Kanban UI it serves (Board / Epics / Sprints views over `Ticketing/`).
- `sidecar.json` — the manifest the host's registry loads.

Validate it with the conformance kit and grow it from the template TODOs.

## Headless UI click-through — `clickthrough.mjs` (CPE-1168)

A zero-dependency Node harness that drives the served UI end-to-end, retiring
`MANUAL-TEST-BURNDOWN.md` row #9 (the standalone-board view-switcher click-through). It launches the
built sidecar, speaks just enough of the ADR-0001 stdio contract to reach `Ready` (the sidecar emits
`Hello`; the harness replies `Welcome`; the sidecar announces its loopback UI URL as a `Status` event),
then points **headless Edge** (`msedgedriver`, raw WebDriver HTTP — no WebdriverIO/tauri-driver, no
`npm install`) at that URL, clicks each top-level view button, and asserts the rendered list swaps —
checking **real computed visibility** (`getComputedStyle().display`), not just the `hidden` DOM
property (the distinction that caught CPE-1168's swap bug). A screenshot per view lands in
`.screenshots/` (gitignored). The browser + sidecar are torn down in a `finally`.

```
# from the repo root, with msedgedriver on PATH (this repo keeps it in ~/.cargo/bin):
cargo build --release --manifest-path sidecar/agent-board/Cargo.toml
node sidecar/agent-board/clickthrough.mjs
```

Exit 0 = all three views drove and swapped; non-zero prints the failing assertion. Loopback-only, no
user, no creds. Not yet wired into CI (would need Edge + `msedgedriver` on the runner, like
`gui-smoke/`); a follow-up can add it to `.github/workflows/gui-smoke.yml`.
