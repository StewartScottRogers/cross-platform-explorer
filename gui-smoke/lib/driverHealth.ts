// CPE-1955 — telling a WebDriver TRANSPORT death apart from an ordinary in-app assertion failure, and
// naming the resulting shard abort in one legible line. Extracted out of `wdio.conf.ts` so the two
// judgements the containment fix rests on are unit-testable (`driverHealth.test.ts`) without needing a
// real `tauri-driver` round trip — same "pure lib module, thin caller" split as `waitForPort.ts` and
// `shard.ts`.
//
// WHAT THIS IS FOR, with the measured evidence. On 2026-08-27 `gui-smoke` shard 2 failed four times on
// four unrelated PRs, always identically: "SUITE DID NOT COMPLETE: expected 14 spec file(s) but only 1
// reported any result", with ZERO new failing cases. Reading the four job logs (jobs 98553879134,
// 98601625108, 98646323315, 98647909000) gave one identical chain every time, not a plausible story:
//
//   1. `archive-browse.smoke.ts` (shard 2's spec #1) runs and passes.
//   2. `handleRunnableStart` runs `resetAppState` before `checkpoint-restore.smoke.ts` (spec #2). It
//      fails with an ORDINARY app-level assertion — `expected the breadcrumb to show
//      "cpe-gui-smoke-xxxxxx" after navigating to /tmp/cpe-gui-smoke-xxxxxx` — i.e. the renderer had not
//      settled in time. That is the CPE-1728 environment signature, a soft failure, and CPE-1866's
//      recovery path is supposed to absorb it.
//   3. That recovery path calls `browser.reloadSession()`, whose first act is `DELETE /session/<id>`.
//   4. Roughly 600 ms later tauri-driver logs, in its OWN voice, "Error serving connection:
//      hyper::Error(User(Service), client error (SendRequest) ... connection closed before message
//      completed)", and from then on every request to it is "client error (Connect) ... Connection
//      refused (os error 111)". The NATIVE driver behind tauri-driver (WebKitWebDriver on
//      NATIVE_DRIVER_PORT) has gone away, and nothing respawns it. The transport is dead for the rest of
//      the shard.
//
// So the recovery mechanism for a soft failure is itself what converts that soft failure into a
// whole-shard wipeout. Every subsequent spec's before-hook then fails instantly against a dead socket.
// (It does not ALWAYS die: job 98697809924 hit the identical step-2 reset failure on the identical spec
// and `reloadSession()` recovered in 35.4 s. That is why this is intermittent rather than constant, and
// why the containment below is a bounded RESPAWN rather than a blanket "assume dead".)
//
// The distinction this module draws is the one the containment turns on: a step-2 failure is the app
// misbehaving and `reloadSession()` is the right answer, whereas a step-4 failure is the PLUMBING being
// gone and no amount of asking the dead driver for a new session can help — that needs tauri-driver
// itself restarted. Conflating them is what made the four failures unreadable.

/** Substrings that only ever appear when the WebDriver TRANSPORT itself is gone — the socket to
 *  tauri-driver (or to the native driver behind it) refused, reset, or closed mid-message — as opposed
 *  to the driver answering normally with a WebDriver-protocol error, or an in-app assertion failing.
 *
 *  Every entry is copied from real CI output, not guessed:
 *  - `UND_ERR_SOCKET` — undici's code, and the `code` field on the `WebDriverRequestError` WDIO actually
 *    threw into `handleRunnableStart`'s catch in all four failures.
 *  - `ECONNREFUSED` / `Connection refused (os error 111)` — the Node-side and the tauri-driver-side
 *    spelling of the same refused connect, both present in the logs (the second is tauri-driver's own
 *    hyper error text, echoed onto our stdout because we inherit its stdio).
 *  - `connection closed before message completed` — the FIRST symptom in the chain, logged the instant
 *    the native driver disappeared mid-response to the `DELETE /session`.
 *  - `socket hang up` / `ECONNRESET` / `EPIPE` — the other shapes the same "peer went away" event takes
 *    depending on exactly when it happens. Not observed in these four logs; included because they are
 *    the same class and excluding them would just mean the next variant reads as a mystery again. */
const TRANSPORT_DEATH_MARKERS: readonly string[] = [
  "UND_ERR_SOCKET",
  "ECONNREFUSED",
  "Connection refused (os error 111)",
  "connection closed before message completed",
  "socket hang up",
  "ECONNRESET",
  "EPIPE",
];

/** Flattens whatever was thrown into the text this module matches against. Errors carry their marker in
 *  different places depending on the layer that produced them — `code` for undici's
 *  `WebDriverRequestError`, `message` for a rethrown WebDriver error, `cause` for anything wrapped — so
 *  all of them are searched rather than betting on one. Bounded to one level of `cause` unwrapping per
 *  step so a self-referential cause chain cannot spin. */
function errorText(err: unknown): string {
  const parts: string[] = [];
  const seen = new Set<unknown>();
  let current: unknown = err;
  for (let depth = 0; depth < 8 && current !== undefined && current !== null; depth += 1) {
    if (seen.has(current)) break;
    seen.add(current);
    if (typeof current === "string") {
      parts.push(current);
      break;
    }
    if (typeof current !== "object") {
      parts.push(String(current));
      break;
    }
    const rec = current as { message?: unknown; code?: unknown; cause?: unknown };
    if (typeof rec.message === "string") parts.push(rec.message);
    if (typeof rec.code === "string") parts.push(rec.code);
    current = rec.cause;
  }
  return parts.join("\n");
}

/** True when `err` is the WebDriver transport being GONE rather than the app under test misbehaving.
 *
 *  Deliberately conservative: it matches only the socket-level markers above, so an ordinary in-app
 *  assertion failure (`expected the breadcrumb to show ...`) — the step-2 failure that legitimately wants
 *  a plain `reloadSession()` — returns false and keeps the cheaper recovery. A false NEGATIVE here costs
 *  one wasted `reloadSession()` attempt; a false POSITIVE would cost a needless ~30 s driver respawn, so
 *  the bias is towards not matching. */
export function isTransportDead(err: unknown): boolean {
  const text = errorText(err);
  if (text === "") return false;
  return TRANSPORT_DEATH_MARKERS.some((marker) => text.includes(marker));
}

/** One-line-per-clause summary of an unrecoverable shard abort, printed once by `wdio.conf.ts` at the
 *  moment the transport is confirmed gone AND the bounded respawn has been used up.
 *
 *  WHY THIS EXISTS AT ALL — this is the acceptance criterion the CPE-1955 ticket calls "most of the
 *  value". Before it, the only thing a reader got was the ratchet's "only 1 of 14 reported", which names
 *  neither the spec that died nor the reason, and reads as a mystery infrastructure flake — which is
 *  what trained the crew to reach for `gh run rerun`, the habit that eventually lets a real regression
 *  through. After it, the job says which spec it died in, what killed the transport, and — crucially —
 *  that the N failures that follow are ONE death and not N regressions, so nobody is tempted to file
 *  N `known-failing.json` entries for them.
 *
 *  `remainingSpecs` is how many spec files after `diedIn` are still to come; they will each fail their
 *  own before-hook against the dead socket, which is the honest record of what happened to them and is
 *  what makes them show up in the ratchet by NAME instead of vanishing (see `wdio.conf.ts`'s
 *  `currentSpecFile` handling — the attribution half of this ticket). */
export function formatShardAbort(diedIn: string, remainingSpecs: number, err: unknown): string {
  const detail = errorText(err).split("\n")[0] ?? String(err);
  const following =
    remainingSpecs > 0
      ? `The ${remainingSpecs} spec file(s) after it will each fail their own before-hook against the dead socket and are reported BY NAME — that is ONE driver death, not ${remainingSpecs} separate regressions, so do NOT add known-failing entries for them.`
      : "It was the last spec file in this shard, so nothing follows it.";
  return [
    `[gui-smoke] SHARD ABORTED in ${diedIn}: the WebDriver transport is gone and a bounded respawn of tauri-driver did not bring it back.`,
    `[gui-smoke] SHARD ABORTED — cause: ${detail}`,
    `[gui-smoke] SHARD ABORTED — ${following}`,
    "[gui-smoke] SHARD ABORTED — this is CPE-1955's failure mode. Re-running turns it green without fixing anything; read gui-smoke/lib/driverHealth.ts's header for the measured chain.",
  ].join("\n");
}
