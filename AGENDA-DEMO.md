# Team Agenda — What's Next

Based on recent commits, epics queue, and backlog state (as of 2026-07-23).

## Priority Areas

1. **Complete tauri-specta typed bindings rollout (CPE-953)**  
   Foundation landed in CPE-812 — now roll out `#[specta::specta]` annotations to all `#[tauri::command]` functions and migrate frontend call sites to use the generated `bindings.gen.ts` typed client. This eliminates runtime type mismatches and makes the frontend/backend contract explicit and compile-checked. ~4h medium-priority ticket, ready to work.

2. **Activate high-value Agent Watch epics**  
   The Agent Watch mode has 5 unopened epics (CPE-728 through CPE-732): activity replay, multi-agent conflict radar, cost/resource dashboard, intervene-and-approve, and checkpoint-rollback. Recent commits show strong Mailbox coordination infrastructure (CPE-954/955) — perfect foundation to activate one of these epics and deliver a flagship Agent Watch feature that showcases live visibility into AI activity.

3. **Performance epic — 10x faster (CPE-688)**  
   The "fast, small, predictable" tiebreaker from PURPOSE.md is central to the project. CPE-688 (epic-explorer-performance-10x) is the structured push to measure and optimize hot paths (directory listing, preview generation, search). Recent work on streaming liveness (CPE-662), thumbnail priority queues (CPE-950), and spotlight ranking (CPE-952) all feed this — time to activate the umbrella epic and set quantified targets.

4. **Remote/cloud filesystem support (CPE-616)**  
   Epic CPE-616 (remote-and-cloud-filesystems) and the scheme-based provider seam from CPE-685 set up a plugin model for non-local filesystems. WebDAV, SFTP, S3, and cloud drives are natural next frontiers. The cpe-server architecture (CPE-810) already decouples domain logic — extending it to handle remote mounts is the logical next frontier for "cross-platform" to mean cross-location too.

5. **User command templating — full rollout (CPE-783 follow-up)**  
   CPE-783 landed the GUI for user-defined templated commands with a confirm-before-launch gate. The next step: integrate this with the scriptable actions/macros epic (CPE-739, which delivered the macro library in CPE-951) so users can compose multi-step workflows. This bridges one-off commands and full automation, giving power users a low-friction path to automate repetitive tasks without leaving the explorer.

---

**Common thread:** Recent work shows a maturity inflection — foundations (sidecar platform, typed bindings, streaming liveness, user commands) are landed; now activate the epics that *use* them to deliver user-visible step-changes in capability and polish.
