// CPE-1660 notice-overflow verification harness — inner iframe document script. Reads ?notice=<short|
// long> from the URL (width comes from the OUTER page sizing the <iframe> itself), mounts the real
// StatusBar.svelte with a notice of the requested length, and publishes a diagnostic object on `window`
// that the outer page reads back via same-origin `iframe.contentWindow` access.
import StatusBar from "../../../src/lib/components/StatusBar.svelte";

const params = new URLSearchParams(location.search);
const kind = params.get("notice") === "long" ? "long" : "short";

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
    notice: kind === "long" ? LONG_NOTICE : SHORT_NOTICE,
  },
});
(window as unknown as { __statusBar?: unknown }).__statusBar = app;

function computeDiag() {
  const statusbar = document.querySelector(".statusbar") as HTMLElement | null;
  const diag: Record<string, unknown> = { innerWidth: window.innerWidth, noticeLineCount: 0 };
  if (statusbar) {
    const r = statusbar.getBoundingClientRect();
    diag.statusbarRect = { height: r.height, width: r.width };
  }
  (window as unknown as { __sbDiag?: unknown }).__sbDiag = diag;
  (window as unknown as { __sbDiagReady?: boolean }).__sbDiagReady = true;
}

requestAnimationFrame(() => requestAnimationFrame(computeDiag));
