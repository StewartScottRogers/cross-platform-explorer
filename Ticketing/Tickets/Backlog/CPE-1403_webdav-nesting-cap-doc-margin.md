---
id: CPE-1403
title: "WebDAV: doc-comment on MAX_XML_NESTING_DEPTH overstates the safety margin (build-profile dependent)"
type: Task
status: Backlog
priority: Low
component: Backend
tags: [ready]
epic: CPE-616
created: 2026-08-07
---

## Problem (CPE-1398 / PR #678 re-review note — non-exploitable)
`crates/webdav/src/lib.rs` `MAX_XML_NESTING_DEPTH` doc comment states the crash threshold is ~150 on a 256KB
stack — but the re-reviewer found that's only true for OPTIMIZED (release) builds. A DEBUG build overflows even
a 256KB thread stack at the allowed cap (64). NOT exploitable against users (the app ships release; real parse
threads get multi-MB Tokio spawn_blocking stacks), but a future debug-mode fuzz/CI harness touching this path
could hit a startling crash that looks like a regression. Also: `xmlparser` (the guard's lexer) and roxmltree's
internal lexer are now INDEPENDENTLY-maintained forks of a common 2023 ancestor (xmlparser is dormant) — a
structural drift risk to note.

## Fix direction
Update the doc comment: state the crash depth is BOTH stack-size AND build-profile dependent (debug overflows
far shallower than release), that 64 is safe for the app's release build + multi-MB threads with large margin,
and add a note that the guard's xmlparser lexer must stay grammar-aligned with roxmltree (re-verify on a
roxmltree major bump). Doc-only; no logic change.
