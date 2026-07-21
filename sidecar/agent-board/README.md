# agent-board

A Cross-Platform Explorer sidecar (see `docs/adr/0001-sidecar-platform.md`).

- `src/main.rs` — the sidecar process. Depends only on `sidecar-contract`.
- `sidecar.json` — the manifest the host's registry loads.

Validate it with the conformance kit and grow it from the template TODOs.
