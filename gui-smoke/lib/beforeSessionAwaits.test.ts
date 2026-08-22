// CPE-1843 — the guard that keeps CPE-1772's + CPE-1832's two-port startup fix from silently rotting.
//
// `wdio.conf.ts`'s `beforeSession` spawns `tauri-driver` and then waits for BOTH ports before returning:
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
// Red-proofed by deleting each `await` in turn (both the 4444 wait and the 4445 wait, separately) and
// confirming this file fails each time; see CPE-1843's work log for the exact lines.
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
  const source = fs.readFileSync(WDIO_CONF_PATH, "utf-8");
  return ts.createSourceFile(WDIO_CONF_PATH, source, ts.ScriptTarget.Latest, /* setParentNodes */ true, ts.ScriptKind.TS);
}

/** The `beforeSession` hook's function body, however it is spelled (`beforeSession: async () => {}`,
 *  `beforeSession: async function () {}`, or the `async beforeSession() {}` method shorthand). */
function findBeforeSession(sourceFile: ts.SourceFile): {
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
          `wdio.conf.ts:${line} — \`beforeSession\` is no longer a function literal this guard can read. ` +
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

describe("wdio.conf.ts beforeSession — both port waits stay awaited (CPE-1843)", () => {
  const sourceFile = parseWdioConf();
  const { fn: beforeSession, line: beforeSessionLine } = findBeforeSession(sourceFile);
  const calls = collectWaitForPortCalls(sourceFile, beforeSession);

  it("declares beforeSession `async`, so an `await` inside it is even possible", () => {
    const isAsync = (ts.getModifiers(beforeSession) ?? []).some((m) => m.kind === ts.SyntaxKind.AsyncKeyword);
    assert.ok(
      isAsync,
      `wdio.conf.ts:${beforeSessionLine} — \`beforeSession\` lost its \`async\` modifier, so it cannot ` +
        `await either port wait and WDIO will not wait for the hook to settle. ` + FIX_HINT,
    );
  });

  it("waits on BOTH tauri-driver's own port and the native WebDriver's port", () => {
    assert.equal(
      calls.length,
      REQUIRED_PORT_ARGS.length,
      `Expected ${REQUIRED_PORT_ARGS.length} \`waitForPort\` calls inside \`beforeSession\` (one per port ` +
        `in the proxy chain), found ${calls.length} at line(s) ${calls.map((c) => c.line).join(", ") || "none"}. ` +
        FIX_HINT,
    );
    assert.deepEqual(
      calls.map((c) => c.portArg),
      [...REQUIRED_PORT_ARGS],
      "The two `waitForPort` calls in `beforeSession` no longer wait on tauri-driver's own port and then " +
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
      const match = calls.find((c) => c.portArg === portArg);
      assert.ok(
        match,
        `No \`waitForPort\` call for ${portArg} found inside \`beforeSession\`. ` + FIX_HINT,
      );
      assert.ok(
        ts.isAwaitExpression(match.call.parent),
        `wdio.conf.ts:${match.line} — the \`waitForPort\` call for ${portArg} is NOT awaited (its parent ` +
          `expression is \`${ts.SyntaxKind[match.call.parent.kind]}\`, not \`AwaitExpression\`). ` + FIX_HINT,
      );
    });
  }
});
