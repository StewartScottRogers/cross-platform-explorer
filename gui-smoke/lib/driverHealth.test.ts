// CPE-1955 — headless unit tests for `driverHealth.ts`, the transport-death predicate the gui-smoke
// containment fix turns on. Runs under Node's built-in test runner via `tsx` (same convention as
// `waitForPort.test.ts`/`shard.test.ts`):
//   npm run test:unit          (from gui-smoke/)
//
// The load-bearing property is the DISTINCTION, in both directions: the exact error text CI produced
// when the app misbehaved must NOT be called a transport death (it wants a cheap `reloadSession()`), and
// the exact error text CI produced when the native driver disappeared MUST be. Every input below is
// copied verbatim from the four failing job logs named in `driverHealth.ts`'s header, so these tests
// falsify the fix against the real evidence rather than against a paraphrase of it.
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { formatShardAbort, isTransportDead } from "./driverHealth.js";

/** The `WebDriverRequestError` shape WDIO actually threw into `handleRunnableStart`'s catch once the
 *  native driver behind tauri-driver had gone (job 98646323315, 19:44:19Z): an Error whose `message`
 *  names the undici code and whose `code` field carries it too. */
function transportError(): Error & { code: string } {
  const err = new Error(
    'WebDriverError: Request failed with error code UND_ERR_SOCKET when running "execute/sync" with method "POST"',
  ) as Error & { code: string };
  err.code = "UND_ERR_SOCKET";
  return err;
}

describe("isTransportDead — app misbehaving vs plumbing gone", () => {
  it("does NOT match the in-app assertion failure that opens every observed CPE-1955 chain", () => {
    // Verbatim from all four logs, step 2. This one legitimately wants `reloadSession()`; calling it a
    // transport death would respawn tauri-driver on every ordinary slow-renderer blip, which is both
    // slower and would have masked the real bug.
    const err = new Error(
      'expected the breadcrumb to show "cpe-gui-smoke-D4eCFu" after navigating to /tmp/cpe-gui-smoke-D4eCFu',
    );
    assert.equal(isTransportDead(err), false);
  });

  it("matches the WebDriverRequestError CI threw once the native driver was gone", () => {
    assert.equal(isTransportDead(transportError()), true);
  });

  it("matches on the `code` field alone, even with an unhelpful message", () => {
    const err = new Error("Request failed") as Error & { code: string };
    err.code = "UND_ERR_SOCKET";
    assert.equal(isTransportDead(err), true);
  });

  it("matches tauri-driver's own hyper wording for the first symptom in the chain", () => {
    // The FIRST thing that appears, ~600 ms after the DELETE /session, before the port starts refusing.
    assert.equal(
      isTransportDead(
        new Error(
          "Error serving connection: hyper::Error(User(Service), client error (SendRequest)\n\nCaused by:\n    connection closed before message completed)",
        ),
      ),
      true,
    );
  });

  it("matches tauri-driver's refused-connect wording (the os error 111 spelling)", () => {
    assert.equal(
      isTransportDead(
        new Error(
          "Error serving connection: hyper::Error(User(Service), client error (Connect)\n\nCaused by:\n    0: tcp connect error\n    1: Connection refused (os error 111))",
        ),
      ),
      true,
    );
  });

  it("finds the marker through a `cause` chain rather than only on the top-level error", () => {
    const err = new Error("resetAppState failed", { cause: transportError() });
    assert.equal(isTransportDead(err), true);
  });

  it("tolerates non-Error throwables without crashing", () => {
    assert.equal(isTransportDead("ECONNREFUSED 127.0.0.1:4444"), true);
    assert.equal(isTransportDead("something else entirely"), false);
    assert.equal(isTransportDead(undefined), false);
    assert.equal(isTransportDead(null), false);
    assert.equal(isTransportDead(42), false);
  });

  it("does not spin on a self-referential cause chain", () => {
    const a = new Error("a") as Error & { cause?: unknown };
    const b = new Error("b") as Error & { cause?: unknown };
    a.cause = b;
    b.cause = a;
    assert.equal(isTransportDead(a), false);
  });
});

describe("formatShardAbort — the diagnosis that replaces 'only 1 of 14 reported'", () => {
  it("names the spec it died in and the cause", () => {
    const out = formatShardAbort("checkpoint-restore.smoke.ts", 12, transportError());
    assert.match(out, /checkpoint-restore\.smoke\.ts/);
    assert.match(out, /UND_ERR_SOCKET/);
  });

  it("says the following failures are ONE death, not N regressions", () => {
    const out = formatShardAbort("checkpoint-restore.smoke.ts", 12, transportError());
    assert.match(out, /12 spec file\(s\) after it/);
    assert.match(out, /not 12 separate regressions/);
    assert.match(out, /do NOT add known-failing entries/);
  });

  it("does not claim following specs exist when it died in the last one", () => {
    const out = formatShardAbort("trash.smoke.ts", 0, transportError());
    assert.match(out, /last spec file in this shard/);
    assert.doesNotMatch(out, /separate regressions/);
  });

  it("cites the owning ticket so a reader lands on the measured writeup", () => {
    assert.match(formatShardAbort("x.smoke.ts", 1, transportError()), /CPE-1955/);
  });
});
