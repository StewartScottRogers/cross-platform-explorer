// CPE-1843 — the guard that keeps CPE-1772's + CPE-1832's two-port startup fix from silently rotting.
//
// --- CPE-1955 RETARGET, done deliberately and in the same PR as the restructure ----------------------
// The spawn + both port waits moved out of the `beforeSession` hook body into a named `startTauriDriver`
// helper, so that CPE-1955's mid-shard driver respawn (see `wdio.conf.ts`'s `respawnTauriDriver`) starts
// the driver by the SAME path — readiness waits included — instead of a second hand-rolled copy that
// could drift. That is exactly the restructure FIX_HINT below anticipates ("If you genuinely restructured
// this wait (e.g. into a helper...), update THIS guard in the same PR, deliberately — do not delete it"),
// so this file follows its own instruction rather than being deleted or watered down. The guard is
// STRICTLY STRONGER afterwards, not weaker: it still proves both waits exist and are awaited (now inside
// the helper), and it additionally proves (a) `beforeSession` still AWAITS that helper, so the hook
// cannot resolve before the driver chain is up, (b) the CPE-1955 respawn goes through the same helper,
// and (c) `tauri-driver` is spawned in exactly ONE place in the file, which is what stops a future
// "quick" second spawn from reopening the CPE-1772/CPE-1832 race behind the guard's back.
//
// `wdio.conf.ts`'s driver startup spawns `tauri-driver` and then waits for BOTH ports before returning:
// tauri-driver's own front door (`--port`, 4444) and the native WebDriver's back door (`--native-port`,
// 4445) that every request is proxied to. Both waits are `await`ed today. NOTHING PINNED THAT until this
// file: `waitForPort.test.ts` exercises the poll in isolation, and the 121-case wdio suite exercises the
// app — neither one ever looks at whether `beforeSession` actually *waits*. A future edit dropping one
// `await` — the classic form of exactly this bug — would leave every existing suite green and quietly
// reopen the race on Linux CI, the platform where it is hardest to reproduce.
//
// WHY A SOURCE-AST GUARD RATHER THAN CALLING `beforeSession` FOR REAL:
// importing `wdio.conf.ts` is not free. Its module top level `throw`s unless a REAL Tauri-CLI release
// binary already exists at `src-tauri/target/release/cross-platform-explorer[.exe]` (the CPE-1044 guard),
// reads `src-tauri/tauri.conf.json`, and computes shard assignments from `process.env` — so a behavioural
// test of `beforeSession` would need a full app build to even reach the assertion, which is precisely the
// cost `waitForPort.ts` was extracted to avoid. Refactoring the config to be importable would put more
// new, untested machinery between the harness and the thing being guarded than the guard is worth.
// Parsing the file with the TypeScript compiler API (a devDependency already here, used by
// `npm run typecheck`) costs milliseconds, needs no build, and asserts the exact property that matters:
// every `waitForPort` call inside `beforeSession` is the operand of an `await`. It is a *syntactic*
// guard and it says so — it cannot prove the ports are reachable at runtime (that is `waitForPort.test.ts`
// plus the real CI run), only that the harness still waits for them.
//
// --- EVERY ASSERTION LIVES INSIDE AN `it()`, DELIBERATELY (CPE-1843 round-2 review) ------------------
// The first version of this file parsed `wdio.conf.ts` and located `beforeSession` in the `describe`
// BODY, so the two checks that prove this guard reached its target at all — the file being readable, and
// exactly one `beforeSession` existing — ran outside any test. Measured consequence on **node 22.22.3**:
// a throw from a `describe` callback prints `not ok 1 - ...` but still reports `# fail 0` and **exits 0**,
// while the suite total silently drops by this file's cases. Under the two mutations that matter most —
// renaming/moving `wdio.conf.ts` (ENOENT), and extracting the hook to a named helper
// (`beforeSession: startTauriDriver,`, the very restructure FIX_HINT below anticipates) — the guard would
// have VANISHED with CI still green. Node 20 (what `gui-smoke.yml` pins today) exits 1 on the same
// mutation, so the old shape was safe only by accident of an unrelated version pin that nobody bumping
// node would think to re-check. Keep the parse/locate inside `it()` bodies — same discipline as
// `libLayout.test.ts`. A guard that can disappear quietly is worse than no guard.
//
// Red-proofed by deleting each `await` in turn (both the 4444 wait and the 4445 wait, separately), and
// again by both vanish-mutations above; see CPE-1843's work log for the exact lines and exit codes.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const LIB_DIR = path.dirname(fileURLToPath(import.meta.url));
const WDIO_CONF_PATH = path.resolve(LIB_DIR, "../wdio.conf.ts");

/** The two port constants `beforeSession` must wait on — tauri-driver's own front door and the native
 *  WebDriver's back door. Named as identifiers because that is how the calls are written; asserting on
 *  the NAMES (rather than the literal numbers) keeps this guard honest if the numbers ever move. */
const REQUIRED_PORT_ARGS = ["TAURI_DRIVER_PORT", "NATIVE_DRIVER_PORT"] as const;

const FIX_HINT =
  "This is not a style nit: an un-awaited `waitForPort` returns a floating promise, `beforeSession` " +
  "resolves immediately, and WDIO's first `POST /session` goes out before the driver chain is listening " +
  "— the exact ECONNREFUSED / \"Connection refused (os error 111)\" race CPE-1772 and CPE-1832 fixed, " +
  "which reproduces only under CI contention and would leave every other suite green. If you genuinely " +
  "restructured this wait (e.g. into a helper, or a Promise.all), update THIS guard in the same PR, " +
  "deliberately — do not delete it.";

function parseWdioConf(): ts.SourceFile {
  assert.ok(
    fs.existsSync(WDIO_CONF_PATH),
    `Could not find the file this guard exists to protect: ${WDIO_CONF_PATH}. If wdio.conf.ts was ` +
      "renamed or moved, update WDIO_CONF_PATH here in the same PR — do not leave this guard pointing at " +
      "nothing. " +
      FIX_HINT,
  );
  const source = fs.readFileSync(WDIO_CONF_PATH, "utf-8");
  return ts.createSourceFile(WDIO_CONF_PATH, source, ts.ScriptTarget.Latest, /* setParentNodes */ true, ts.ScriptKind.TS);
}

/** The `beforeSession` hook's function body, however it is spelled (`beforeSession: async () => {}`,
 *  `beforeSession: async function () {}`, or the `async beforeSession() {}` method shorthand). */
function locateBeforeSession(sourceFile: ts.SourceFile): {
  fn: ts.ArrowFunction | ts.FunctionExpression | ts.MethodDeclaration;
  line: number;
} {
  const found: Array<{ fn: ts.ArrowFunction | ts.FunctionExpression | ts.MethodDeclaration; line: number }> = [];

  const visit = (node: ts.Node): void => {
    const isNamedBeforeSession =
      (ts.isPropertyAssignment(node) || ts.isMethodDeclaration(node)) &&
      ts.isIdentifier(node.name) &&
      node.name.text === "beforeSession";
    if (isNamedBeforeSession) {
      const line = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
      if (ts.isMethodDeclaration(node)) {
        found.push({ fn: node, line });
      } else if (ts.isArrowFunction(node.initializer) || ts.isFunctionExpression(node.initializer)) {
        found.push({ fn: node.initializer, line });
      } else {
        assert.fail(
          `wdio.conf.ts:${line} — \`beforeSession\` is no longer a function literal this guard can read ` +
            `(it is now \`${ts.SyntaxKind[node.initializer.kind]}\`, e.g. the hook extracted to a named ` +
            "helper). This guard can then no longer see the port waits at all, so it must be pointed at " +
            "wherever they moved, in the same PR. " +
            FIX_HINT,
        );
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);

  assert.equal(
    found.length,
    1,
    `Expected exactly one \`beforeSession\` hook in wdio.conf.ts, found ${found.length}. ` + FIX_HINT,
  );
  return found[0];
}

/** Every `waitForPort(...)` call inside `fn`, with its 1-based source line and its port argument text. */
function collectWaitForPortCalls(
  sourceFile: ts.SourceFile,
  fn: ts.Node,
): Array<{ call: ts.CallExpression; line: number; portArg: string }> {
  const calls: Array<{ call: ts.CallExpression; line: number; portArg: string }> = [];
  const visit = (node: ts.Node): void => {
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === "waitForPort") {
      calls.push({
        call: node,
        line: sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1,
        portArg: node.arguments[1] ? node.arguments[1].getText(sourceFile) : "<missing>",
      });
    }
    ts.forEachChild(node, visit);
  };
  visit(fn);
  return calls;
}

/** CPE-1955: the named `function startTauriDriver(...)` declaration the spawn + both port waits now live
 *  in. Located by name, asserted to exist exactly once — the same reach discipline as
 *  `locateBeforeSession`, for the same reason: if this guard cannot find its target it must FAIL, never
 *  quietly pass over an empty search. */
function locateFunctionDeclaration(
  sourceFile: ts.SourceFile,
  name: string,
): { fn: ts.FunctionDeclaration; line: number } {
  const found: Array<{ fn: ts.FunctionDeclaration; line: number }> = [];
  const visit = (node: ts.Node): void => {
    if (ts.isFunctionDeclaration(node) && node.name?.text === name) {
      found.push({ fn: node, line: sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1 });
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);
  assert.equal(
    found.length,
    1,
    `Expected exactly one \`function ${name}\` declaration in wdio.conf.ts, found ${found.length}. ` + FIX_HINT,
  );
  return found[0];
}

/** Every `await <name>(...)` call inside `fn`, by callee name — used to prove that `beforeSession` and
 *  CPE-1955's `respawnTauriDriver` both reach the driver through `startTauriDriver` rather than around it. */
function awaitsCallTo(sourceFile: ts.SourceFile, fn: ts.Node, calleeName: string): boolean {
  let found = false;
  const visit = (node: ts.Node): void => {
    if (
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      node.expression.text === calleeName &&
      ts.isAwaitExpression(node.parent)
    ) {
      found = true;
    }
    ts.forEachChild(node, visit);
  };
  visit(fn);
  return found;
}

/** Every `spawn(TAURI_DRIVER_BIN, ...)` call anywhere in the file, with its 1-based line. CPE-1955: the
 *  readiness waits are only worth anything if EVERY route to a running tauri-driver goes through the one
 *  helper that performs them, so a second spawn site is itself the regression. */
function collectDriverSpawnSites(sourceFile: ts.SourceFile): number[] {
  const lines: number[] = [];
  const visit = (node: ts.Node): void => {
    if (
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      node.expression.text === "spawn" &&
      node.arguments[0] &&
      node.arguments[0].getText(sourceFile) === "TAURI_DRIVER_BIN"
    ) {
      lines.push(sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1);
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);
  return lines;
}

/** Parse + locate + collect, run fresh inside EVERY `it()` (see the header note on why none of this may
 *  live in the `describe` body). Parsing a ~1500-line file a handful of times costs a few milliseconds. */
function analyzeDriverStartup(): {
  sourceFile: ts.SourceFile;
  beforeSession: ts.ArrowFunction | ts.FunctionExpression | ts.MethodDeclaration;
  beforeSessionLine: number;
  startTauriDriver: ts.FunctionDeclaration;
  startTauriDriverLine: number;
  calls: Array<{ call: ts.CallExpression; line: number; portArg: string }>;
} {
  const sourceFile = parseWdioConf();
  const { fn: beforeSession, line: beforeSessionLine } = locateBeforeSession(sourceFile);
  const { fn: startTauriDriver, line: startTauriDriverLine } = locateFunctionDeclaration(
    sourceFile,
    "startTauriDriver",
  );
  return {
    sourceFile,
    beforeSession,
    beforeSessionLine,
    startTauriDriver,
    startTauriDriverLine,
    calls: collectWaitForPortCalls(sourceFile, startTauriDriver),
  };
}

describe("wdio.conf.ts driver startup — both port waits stay awaited (CPE-1843, retargeted by CPE-1955)", () => {
  // THE REACH CHECK. Every other case below depends on this one having been possible; asserting it as its
  // own named test means "the guard could not find its target" fails as a FAILING TEST rather than as a
  // suite that quietly shrinks. See the header note — this is the case that stops the guard from being
  // hollow-in-waiting under a node upgrade.
  it("locates the startTauriDriver helper the port waits live in", () => {
    const { startTauriDriverLine, calls } = analyzeDriverStartup();
    assert.ok(
      startTauriDriverLine > 0,
      "Located `startTauriDriver` but could not resolve its source line — the parse did not reach the file.",
    );
    assert.ok(
      calls.length > 0,
      `Found \`startTauriDriver\` at wdio.conf.ts:${startTauriDriverLine} but NO \`waitForPort\` calls ` +
        "inside it. The whole two-port startup wait is gone, not just an `await`. " +
        FIX_HINT,
    );
  });

  it("declares startTauriDriver `async`, so an `await` inside it is even possible", () => {
    const { startTauriDriver, startTauriDriverLine } = analyzeDriverStartup();
    const isAsync = (ts.getModifiers(startTauriDriver) ?? []).some((m) => m.kind === ts.SyntaxKind.AsyncKeyword);
    assert.ok(
      isAsync,
      `wdio.conf.ts:${startTauriDriverLine} — \`startTauriDriver\` lost its \`async\` modifier, so it ` +
        "cannot await either port wait. " + FIX_HINT,
    );
  });

  it("declares beforeSession `async`, so an `await` inside it is even possible", () => {
    const { beforeSession, beforeSessionLine } = analyzeDriverStartup();
    const isAsync = (ts.getModifiers(beforeSession) ?? []).some((m) => m.kind === ts.SyntaxKind.AsyncKeyword);
    assert.ok(
      isAsync,
      `wdio.conf.ts:${beforeSessionLine} — \`beforeSession\` lost its \`async\` modifier, so it cannot ` +
        `await the driver startup and WDIO will not wait for the hook to settle. ` + FIX_HINT,
    );
  });

  // CPE-1955: the helper doing the waits is worth nothing if the hook does not WAIT for the helper.
  // Dropping this one `await` restores the exact CPE-1772 race with both inner `await`s still in place.
  it("beforeSession awaits startTauriDriver, so the hook cannot resolve before the chain is up", () => {
    const { sourceFile, beforeSession, beforeSessionLine } = analyzeDriverStartup();
    assert.ok(
      awaitsCallTo(sourceFile, beforeSession, "startTauriDriver"),
      `wdio.conf.ts:${beforeSessionLine} — \`beforeSession\` no longer contains an AWAITED call to ` +
        "`startTauriDriver`. Both port waits can be perfectly intact inside the helper and the race is " +
        "still wide open, because the hook resolves before any of them have run. " + FIX_HINT,
    );
  });

  // CPE-1955: the mid-shard respawn must come up through the same door, or a restarted driver gets no
  // readiness wait at all — the CPE-1832 race, reintroduced on the one code path taken when the driver
  // has ALREADY proved flaky.
  it("the CPE-1955 mid-shard respawn starts the driver through the same helper", () => {
    const { sourceFile } = analyzeDriverStartup();
    const { fn, line } = locateFunctionDeclaration(sourceFile, "respawnTauriDriver");
    assert.ok(
      awaitsCallTo(sourceFile, fn, "startTauriDriver"),
      `wdio.conf.ts:${line} — \`respawnTauriDriver\` no longer awaits \`startTauriDriver\`. A respawned ` +
        "driver would then be used without waiting for either port, on exactly the code path that only " +
        "runs when the transport has already died once. " + FIX_HINT,
    );
  });

  it("spawns tauri-driver in exactly one place, so every route pays the readiness wait", () => {
    const { sourceFile, startTauriDriver } = analyzeDriverStartup();
    const sites = collectDriverSpawnSites(sourceFile);
    assert.equal(
      sites.length,
      1,
      `Expected exactly one \`spawn(TAURI_DRIVER_BIN, ...)\` in wdio.conf.ts, found ${sites.length} at ` +
        `line(s) ${sites.join(", ") || "none"}. A second spawn site is a second startup path that does ` +
        "not necessarily wait for either port. " + FIX_HINT,
    );
    const start = sourceFile.getLineAndCharacterOfPosition(startTauriDriver.getStart(sourceFile)).line + 1;
    const end = sourceFile.getLineAndCharacterOfPosition(startTauriDriver.getEnd()).line + 1;
    assert.ok(
      sites[0] >= start && sites[0] <= end,
      `wdio.conf.ts:${sites[0]} — tauri-driver is spawned OUTSIDE \`startTauriDriver\` ` +
        `(lines ${start}-${end}), so that spawn does not get the two-port readiness wait. ` + FIX_HINT,
    );
  });

  it("waits on BOTH tauri-driver's own port and the native WebDriver's port", () => {
    const { calls } = analyzeDriverStartup();
    assert.equal(
      calls.length,
      REQUIRED_PORT_ARGS.length,
      `Expected ${REQUIRED_PORT_ARGS.length} \`waitForPort\` calls inside \`startTauriDriver\` (one per ` +
        `port in the proxy chain), found ${calls.length} at line(s) ${calls.map((c) => c.line).join(", ") || "none"}. ` +
        FIX_HINT,
    );
    assert.deepEqual(
      calls.map((c) => c.portArg),
      [...REQUIRED_PORT_ARGS],
      "The two `waitForPort` calls no longer wait on tauri-driver's own port and then " +
        "the native WebDriver's port, in that order. Waiting on the same port twice, or only on the front " +
        "door, is the CPE-1832 bug restated: tauri-driver accepts connections on 4444 the instant it " +
        "starts, while the native driver it proxies EVERY request to is still coming up on 4445. " +
        FIX_HINT,
    );
  });

  // The actual sabotage this file exists to catch. Each call is asserted individually so the failure
  // names WHICH wait lost its `await`, not just that one of them did.
  for (const portArg of REQUIRED_PORT_ARGS) {
    it(`awaits the waitForPort call for ${portArg}`, () => {
      const { calls } = analyzeDriverStartup();
      const match = calls.find((c) => c.portArg === portArg);
      assert.ok(
        match,
        `No \`waitForPort\` call for ${portArg} found inside \`startTauriDriver\`. ` + FIX_HINT,
      );
      assert.ok(
        ts.isAwaitExpression(match.call.parent),
        `wdio.conf.ts:${match.line} — the \`waitForPort\` call for ${portArg} is NOT awaited (its parent ` +
          `expression is \`${ts.SyntaxKind[match.call.parent.kind]}\`, not \`AwaitExpression\`). ` + FIX_HINT,
      );
    });
  }
});
