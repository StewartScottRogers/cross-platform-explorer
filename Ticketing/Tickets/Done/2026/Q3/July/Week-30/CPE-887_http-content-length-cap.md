---
id: CPE-887
title: Cap HTTP request body size so a hostile Content-Length can't abort the sidecar
type: bug
component: Sidecar
priority: high
tags: ready
epic: CPE-259
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
Same DoS class as CPE-886 in the HTTP request path. `ai-console::http::read_request` read the client's
`Content-Length` header and did `vec![0u8; content_length]` with **no upper bound**. A request with
`Content-Length: 999999999999` (no body even required) makes the sidecar attempt a ~1 TB allocation; the
allocation failure runs `handle_alloc_error`, **aborting the whole process** — a trivially reachable DoS on
the loopback UI/API server (the allocation happens before `read_exact`, so just the headers are enough).

Fix: reject a request whose declared `Content-Length` exceeds `MAX_REQUEST_BODY` (16 MiB — the API only
takes small JSON) **before** allocating, returning `None` so the connection is dropped without a response.

## Acceptance Criteria
- [x] A request with an absurd `Content-Length` is refused before allocating its body.
- [x] A normal small body still parses unchanged.
- [x] `ai-console` http tests (9) + `cargo clippy --all-targets -D warnings` green.

## Work Log
- 2026-07-22 (autonomous) — After CPE-886 (WebSocket frame cap) found the sibling unbounded allocation in
  `read_request`'s `Content-Length` handling. Added a `MAX_REQUEST_BODY` cap + a regression test feeding a
  giant Content-Length. 9/9 http tests pass; clippy clean.
