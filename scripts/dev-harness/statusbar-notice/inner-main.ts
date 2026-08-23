// CPE-1660 notice-overflow verification harness — inner iframe document script. Reads ?notice=<short|
// long|none> from the URL (width comes from the OUTER page sizing the <iframe> itself), mounts the real
// StatusBar.svelte with a notice of the requested length, and publishes a diagnostic object on `window`
// that the outer page reads back via same-origin `iframe.contentWindow` access.
//
// CPE-1859 extends it with `?git=on|off&disk=on|off`, because the right-hand cluster's ANCHORING — not
// just its height — needs a real browser to see: this project's vitest config applies no component CSS,
// so `getComputedStyle`/`getBoundingClientRect` in jsdom cannot observe where in the row an element
// actually lands. The diagnostics below therefore report the measured horizontal POSITION of `.disk`
// (its distance from the bar's right padding edge, and from `.item-count`'s right edge), which is the
// single number that distinguishes "anchored right" from "left-adjacent to the item count".
import StatusBar from "../../../src/lib/components/StatusBar.svelte";

const params = new URLSearchParams(location.search);
const noticeParam = params.get("notice");
const kind = noticeParam === "long" ? "long" : noticeParam === "none" ? "none" : "short";
const showGit = params.get("git") !== "off";
const showDisk = params.get("disk") !== "off";

// Mirrors the ticket's own evidence table: a long, real-world-shaped notice string (German-length
// class) vs. a short one that comfortably fits on one line at the tested widths.
const SHORT_NOTICE = "Saved.";
const LONG_NOTICE =
  "Es gibt nicht behobene Probleme in dieser Basislinie — bitte vor dem Fortfahren prüfen und bestätigen.";

const app = new StatusBar({
  target: document.getElementById("mount")!,
  props: {
    itemCount: 42,
    totalCount: 42,
    notice: kind === "none" ? "" : kind === "long" ? LONG_NOTICE : SHORT_NOTICE,
    // CPE-1859: `{#if git && git.is_repo}` gates the whole chip, so `git: null` is the ordinary
    // NON-REPO folder — the state in which `.disk` has no preceding `margin-left: auto` sibling.
    git: showGit ? { is_repo: true, branch: "main", upstream: "origin/main" } : null,
    diskFree: showDisk ? 123_456_789_012 : null,
    diskTotal: showDisk ? 500_000_000_000 : null,
  },
});
(window as unknown as { __statusBar?: unknown }).__statusBar = app;

function computeDiag() {
  const statusbar = document.querySelector(".statusbar") as HTMLElement | null;
  const diag: Record<string, unknown> = { innerWidth: window.innerWidth, noticeLineCount: 0 };
  if (statusbar) {
    const r = statusbar.getBoundingClientRect();
    diag.statusbarRect = { height: r.height, width: r.width };
    // CPE-1859: the right padding edge of the bar's content box — what a right-anchored child's own
    // right edge should sit flush against (modulo `.disk`'s own zero right margin). Read from the
    // live computed style rather than hard-coding app.css's 14px, so a padding change can't quietly
    // turn a real regression into a passing number.
    const padRight = parseFloat(getComputedStyle(statusbar).paddingRight) || 0;
    diag.contentRight = r.right - padRight;
    const rect = (sel: string) => {
      const el = statusbar.querySelector(sel) as HTMLElement | null;
      if (!el) return null;
      const b = el.getBoundingClientRect();
      return { left: b.left, right: b.right, width: b.width };
    };
    diag.itemCount = rect(".item-count");
    diag.git = rect(".git");
    diag.disk = rect(".disk");
  }
  (window as unknown as { __sbDiag?: unknown }).__sbDiag = diag;
  (window as unknown as { __sbDiagReady?: boolean }).__sbDiagReady = true;
  renderInnerReadout(diag);
}

/** CPE-1859: mirror the diagnostics into this document's own DOM (see inner.html for why). Same numbers
 *  the outer shell prints; deliberately duplicated as TEXT rather than shared, so `--dump-dom` on THIS
 *  url captures a complete measurement with no cross-frame access and no polling. */
function renderInnerReadout(diag: Record<string, unknown>) {
  const el = document.getElementById("inner-readout");
  if (!el) return;
  type R = { left: number; right: number; width: number } | null;
  const fmt = (r: R) =>
    r ? `left=${r.left.toFixed(1)} right=${r.right.toFixed(1)} w=${r.width.toFixed(1)}` : "ABSENT";
  const contentRight = diag.contentRight as number | undefined;
  const disk = diag.disk as R;
  const itemCount = diag.itemCount as R;
  const lines = [
    `innerWidth=${diag.innerWidth} notice=${kind} git=${showGit ? "on" : "off"} disk=${showDisk ? "on" : "off"}`,
    `.item-count ${fmt(itemCount)}`,
    `.git        ${fmt(diag.git as R)}`,
    `.disk       ${fmt(disk)}`,
  ];
  if (contentRight !== undefined && disk) {
    lines.push(`diskRightGap=${(contentRight - disk.right).toFixed(1)}px  (0 ⇒ anchored right)`);
    if (itemCount) {
      lines.push(`diskFromItemCount=${(disk.left - itemCount.right).toFixed(1)}px  (≈14 ⇒ glued to the count)`);
    }
  }
  el.textContent = lines.join("\n");
}

// The double-rAF is the interactive path: it publishes after the browser has laid the component out.
// CPE-1859 adds two idempotent backstops, because under `--headless=new --virtual-time-budget=N` the
// rAF callbacks are not guaranteed to run before the DOM is dumped — without them roughly one driven
// run in three snapshotted the readout still saying "booting…". `computeDiag` only reads and reassigns,
// so running it three times costs a measurement, not a state change.
requestAnimationFrame(() => requestAnimationFrame(computeDiag));
window.addEventListener("load", computeDiag);
setTimeout(computeDiag, 0);
