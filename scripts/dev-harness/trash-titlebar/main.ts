// CPE-1882 layout-guard case script. Registers a canned `list_trash_stream` fixture on the shared
// pluggable mock (../layout-guard/shared-mocks/invoke.ts — see its header for why this is the whole
// wiring a new backend-talking case needs, no bespoke mock file), sets the theme the same way
// src/lib/theme.ts does in the real app, then mounts the REAL TrashView.svelte.
import { registerRawInvoke } from "../layout-guard/shared-mocks/invoke";
import type { TrashEntry, TrashStreamSummary } from "../../../src/lib/bindings.gen";
import TrashView from "../../../src/lib/components/TrashView.svelte";

const params = new URLSearchParams(location.search);
const theme = params.get("theme") === "dark" ? "dark" : "light";
document.documentElement.dataset.theme = theme;

const now = Math.floor(Date.now() / 1000);
const FIXTURE_ENTRIES: TrashEntry[] = [
  { id: "1", name: "quarterly-report.docx", original_path: "/work/proj/quarterly-report.docx", time_deleted: now - 120, size: 245_760 },
  { id: "2", name: "old-screenshots", original_path: "/work/proj/old-screenshots", time_deleted: now - 3_600, size: null },
  { id: "3", name: "build-cache.tmp", original_path: "/work/proj/build/build-cache.tmp", time_deleted: now - 86_400, size: 10_485_760 },
];

// CPE-1882: cases.mjs's "trash-titlebar" case runs a `selfPaint` check on `.tv-x` (the Close button) —
// it must stay hit-testable at every tested width, the exact CPE-1827 invariant. A handful of fixture
// rows is enough for that; the titlebar's own geometry does not depend on entry COUNT once the CPE-1827
// fix is in place (that was the bug: the old markup's toolbar width *did* vary with what fit, which is
// what made it break unpredictably).
registerRawInvoke<{ onEntry: { onmessage: ((batch: TrashEntry[]) => void) | null } }, TrashStreamSummary>(
  "list_trash_stream",
  (args) => {
    args.onEntry.onmessage?.(FIXTURE_ENTRIES);
    return { count: FIXTURE_ENTRIES.length, degraded: false, skipped: 0 } as unknown as TrashStreamSummary;
  },
);

const app = new TrashView({ target: document.getElementById("mount")! });
(window as unknown as { __trashView?: unknown }).__trashView = app;
