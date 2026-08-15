// CPE-1728 — headless unit tests for the log-signature classifier. Runs under Node's built-in test
// runner via `tsx` (same convention as `ratchet.test.ts`/`compare.test.ts`), so it's verifiable WITHOUT
// a `tauri build` or `tauri-driver` session:
//   npm run test:unit          (from gui-smoke/)
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { classifyLog, formatSignatureReport, ENVIRONMENT_MARKERS } from "./logSignature.js";

describe("classifyLog", () => {
  it("reports no-signal on an empty/clean log", () => {
    const result = classifyLog("");
    assert.equal(result.assertionErrorCount, 0);
    assert.equal(result.environmentMarkerCount, 0);
    assert.deepEqual(result.environmentMarkersHit, []);
    assert.equal(result.verdict, "no-signal");
  });

  it("reports no-signal on a real passing run's log (no failure markers of any kind)", () => {
    const log = [
      "[0-0] RUNNING in wry - ./specs/open-dir.smoke.ts",
      "[0-0] PASSED in wry - ./specs/open-dir.smoke.ts",
      "40 passing (12m 3s)",
    ].join("\n");
    const result = classifyLog(log);
    assert.equal(result.verdict, "no-signal");
  });

  it("classifies the PR #900 (CPE-1723) evidence shape as environment-signature-only", () => {
    // The exact excerpt quoted in CPE-1728: zero AssertionErrors, a flood of WebDriver-level noise.
    const log = [
      "libEGL warning: DRI3 error: Could not get DRI3 device",
      'WARN webdriverio: Failed to execute "scrollIntoView" using WebDriver Actions API:',
      "  WebDriverError: move target out of bounds",
      "INFO webdriver: RESULT { error: 'no such element', message: '', stacktrace: '' }",
      "INFO webdriver: RESULT { error: 'no such element', message: '', stacktrace: '' }",
      "keep-awake: could not inhibit screen lock: org.freedesktop.DBus.Error.ServiceUnknown",
      "dbind-WARNING: AT-SPI: Error retrieving accessibility bus address",
      "3 failing, 37 passing",
    ].join("\n");
    const result = classifyLog(log);
    assert.equal(result.assertionErrorCount, 0);
    assert.ok(result.environmentMarkerCount >= 5, `expected several marker hits, got ${result.environmentMarkerCount}`);
    assert.ok(result.environmentMarkersHit.includes("move target out of bounds"));
    assert.ok(result.environmentMarkersHit.includes("no such element"));
    assert.ok(result.environmentMarkersHit.includes("Could not get DRI3 device"));
    assert.equal(result.verdict, "environment-signature-only");
  });

  it("classifies a real assertion failure (no environment markers) as assertion-failures", () => {
    const log = [
      "1) preview-pane.smoke.ts the data-grid preview renders",
      "AssertionError: expected '.data-browser' to be existing after 10000ms",
      "  at Context.<anonymous> (specs/preview-pane.smoke.ts:88:5)",
      "1 failing, 39 passing",
    ].join("\n");
    const result = classifyLog(log);
    assert.equal(result.assertionErrorCount, 1);
    assert.equal(result.environmentMarkerCount, 0);
    assert.equal(result.verdict, "assertion-failures");
  });

  it("classifies a log with BOTH kinds of evidence as mixed, never silently picking one side", () => {
    const log = [
      "AssertionError: expected the breadcrumb to show the seeded folder",
      "WebDriverError: move target out of bounds",
    ].join("\n");
    const result = classifyLog(log);
    assert.equal(result.verdict, "mixed");
    assert.equal(result.assertionErrorCount, 1);
    assert.ok(result.environmentMarkerCount >= 1);
  });

  it("counts multiple occurrences of the same marker, not just distinct markers", () => {
    const log = Array(5).fill("WebDriverError: no such element").join("\n");
    const result = classifyLog(log);
    assert.equal(result.environmentMarkerCount, 5);
    assert.deepEqual(result.environmentMarkersHit, ["no such element"]);
  });

  it("every declared environment marker is individually recognised", () => {
    for (const marker of ENVIRONMENT_MARKERS) {
      const result = classifyLog(`some noise\n${marker}\nmore noise`);
      assert.ok(
        result.environmentMarkersHit.includes(marker),
        `expected "${marker}" to be recognised as an environment marker`,
      );
      assert.equal(result.assertionErrorCount, 0, `marker "${marker}" must not itself look like an AssertionError`);
    }
  });
});

describe("formatSignatureReport", () => {
  it("never returns an empty report, and never throws, for any verdict shape", () => {
    for (const log of ["", "AssertionError: x", "no such element", "AssertionError: x\nno such element"]) {
      const lines = formatSignatureReport(classifyLog(log));
      assert.ok(lines.length > 0);
      for (const line of lines) assert.equal(typeof line, "string");
    }
  });

  it("names every hit marker in the environment-signature-only report", () => {
    const result = classifyLog("no such element\nmove target out of bounds");
    const lines = formatSignatureReport(result).join("\n");
    assert.ok(lines.includes("no such element"));
    assert.ok(lines.includes("move target out of bounds"));
    assert.ok(lines.includes("ENVIRONMENT SIGNATURE ONLY"));
  });

  it("labels a real assertion failure distinctly from environment noise", () => {
    const lines = formatSignatureReport(classifyLog("AssertionError: boom")).join("\n");
    assert.ok(lines.includes("REAL ASSERTION FAILURE"));
  });

  it("labels a mixed log distinctly, warning against assuming it's all environmental", () => {
    const lines = formatSignatureReport(classifyLog("AssertionError: boom\nno such element")).join("\n");
    assert.ok(lines.includes("MIXED"));
  });
});
