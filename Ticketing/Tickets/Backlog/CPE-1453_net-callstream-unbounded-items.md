---
id: CPE-1453
title: "net Client::call_stream accumulates unbounded StreamItems from a hostile server → client OOM"
type: Bug
status: Backlog
priority: Low
component: Backend
tags: [ready, security]
epic: CPE-810
created: 2026-08-08
---
## Vector (found in the net/webdav deep audit, 2026-08-08)
`crates/net/src/client.rs:~176-181`: `items: Vec` grows one push per `StreamItem` with NO count or aggregate-byte
cap. `read_envelope` caps each FRAME at 16 MiB (CPE-1416) but NOT the number of frames. A hostile server answers a
stream request with an endless `StreamItem` sequence (never sending `StreamEnd`) → client OOM.

## Reachability
LOW — the `cpe-net` client isn't wired into the shipped app today (same latent posture as the remote providers).
Real code bug; fix while hardening the network stack.

## Fix direction
Cap total items AND/OR aggregate bytes in `call_stream`; error out past the cap (surface a truncation/limit error).
Pick caps consistent with the 16 MiB per-frame cap and realistic listing sizes.

## Effort / blast radius
S / client.rs. Epic CPE-810. Parallel-safe with the transfer.rs and sidecar work (different crate). Batch with
CPE-1454 (same net crate, different file server.rs).
