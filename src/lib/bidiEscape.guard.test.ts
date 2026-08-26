// CPE-1757 round 2 — makes the bidi/format-character escape (CPE-1712's `displaySafeName`/
// `displaySafePath`, src/lib/filename.ts) an ENFORCED invariant instead of a remembered one.
//
// Round 1 of this guard (a named-shape regex zoo: `X.name`, `X.path`, bare `root`/`path`/`name`,
// `baseName(…)`/`basename(…)`) passed 3/3 with a raw `{revertOnePath}` sitting in a file it already
// covered (CheckpointDialog.svelte:322 — UAT), and a follow-up review's 17-shape probe component proved
// it recognized only 3 of those 17 real render shapes. The engine here (`./bidiRenderScan.ts`) is the
// inversion the review prescribed: inside a registered component, EVERY `{…}` in a text / `title=` /
// `aria-label=` / `alt=` / `{@html …}` position must be a literal, a `displaySafeName(…)`/
// `displaySafePath(…)` call (or an `||`/`??`/ternary-branch combination of those), or it's an offender —
// no shape is named, so no shape can be missed by omission. See `bidiRenderScan.ts`'s own header for the
// full account of what this DOES catch (validated against all 17 probe shapes in
// `bidiRenderScan.test.ts`) and what it still can't see.
//
// REGISTRY is exhaustive-by-equality, not exhaustive-by-omission: for every file below, CI recomputes
// `findUnsafeRenderLines` fresh and requires it to equal the recorded array EXACTLY (sorted). That closes
// both directions of drift review round 2 found inert in round 1's design:
//   - A NEW raw render in an already-registered file changes the computed set, so it no longer equals
//     the recorded array → fails. (Round 1's `ALLOWLIST[file] ?? []` was only ever read for files that
//     were ALSO in `COVERED_FILES`, which never overlapped with `ALLOWLIST`'s keys — so the recorded
//     numbers were prose typed as code, never actually compared to anything. Swapping them for `[99999]`
//     stayed green. Equality against a live recompute can't have that hole.)
//   - A STALE recorded line (the render was fixed, or the file was edited around it) also fails, instead
//     of silently sitting there as a lie about what's still raw.
//
// The domain here is COVERED_FILES from round 1 (12 files) PLUS the 19 components CPE-1712 itself
// originally escaped (FileList, Sidebar, TabBar, HomeView, DetailsPane, TrashView, NavToolbar,
// PropertiesDialog, InstantSearch, ArchiveSafetyDialog, PreviewPane, QuickLook, DiskSpaceView,
// DropStackPanel, FolderBrowser, SidebarNode, RunCommandConfirm, ContentSearchDialog, DuplicatesDialog —
// review round 2's B4: these were claimed as covered in the doc but never mechanically checked at all)
// PLUS the ticket's originally-disclosed "not yet covered" dialogs (ContentIndexSearchDialog,
// FileHealthDialog, NearDuplicatesDialog, SimilarImagesDialog, DeclutterDialog, BatchMediaDialog,
// SplitFileDialog, JoinPartsDialog, ExplorerPane, TerminalPanel), registered here too so their remaining
// raw lines are pinned exactly rather than left completely unchecked. That was the CPE-1757 round-2
// baseline: 41 of the 135 `.svelte` files under src/lib/components/, plus App.svelte on its own (see
// below) — NOT `readdir(components)` in full (review round 2's B5, explicitly hedged "Consider"), and the
// "auditing the other ~94 individually is a different, much larger undertaking" paragraph that used to sit
// here was exactly the debt CPE-1768 was filed to close.
//
// CPE-1768 (below, in its own section) IS that follow-up: `isCandidateComponent` in bidiRenderScan.ts
// states the mechanical membership rule (a filesystem-entry-identity shape — property access, a same-
// vocabulary `let`/`export let` declaration, a `baseName`/`basename`/`parentDir` call site, a destructuring
// or bracket-access pattern — see its own doc for the exact list, widened in review round 2's B1 finding
// after the first pass's narrower five-property version missed three already-shipping raw renders), and a
// guard test walks every REAL `.svelte` file and fails when a candidate isn't registered here. REGISTRY
// now carries 92 keys — every file the criterion currently flags, not a hand-picked residual list.
//
// A dry-run of this exact engine across every file in COVERED_FILES surfaced FOUR real, previously-missed
// spoof surfaces beyond the CheckpointDialog:322 UAT bug — proof the inversion earns its keep, not just a
// bigger regex:
//   - AgentTimeline.svelte: the MAIN activity-timeline row's name span (`{baseOf(e.path)}`, a helper this
//     file defines itself — never named "baseName"/"basename", the exact class of miss review predicted)
//     and the "Competing renames" row's name span (`{baseOf(rc.path)}`) — both title-only fixed in round
//     1, leaving the adjacent name half raw, the identical shape of bug as the CheckpointDialog UAT.
//   - AgentTimeline.svelte: the session-history table's row tooltip (`title={row.cwd}`), a real agent
//     working-directory path, entirely undisclosed.
//   - PreviewPane.svelte: five `<img alt={entry.name}>` attributes (image/decoded-image/raw-image/heic/
//     dicom preview kinds) — `alt` is a render position this engine checks and round 1's file-level scope
//     never reached (B4's whole point).
//   - TrashView.svelte: `f.name` passed raw into `$t("trash.restoreFailed", { name: f.name, ... })` — an
//     i18n interpolation parameter, not a template literal or property-access shape, another class round
//     1's regex never considered.
// All four are fixed in this PR (see the diff for each file) before being added to REGISTRY at `[]` (or
// their remaining harmless offenders) below.
//
// CPE-1885: REGISTRY entries used to be `"<line>:<expr>"`, and the LINE half was the entire problem —
// any edit that shifted lines in a guarded component (adding a script line, reordering markup, an
// unrelated component-wide reformat) moved every offender below it to a new address without changing
// what any of them actually say, and the exact-equality check above cannot tell "the same expression,
// now five lines lower" apart from "a genuinely different render." It failed anyway, with a wall of
// "NEW offender"/"STALE recorded" pairs that were the identical expressions restated at new addresses —
// three separate round-trips lost to exactly that in one day (CPE-1833/1836, CPE-1827 twice; see the
// ticket). REGISTRY is now keyed by the matched EXPRESSION TEXT alone (`findUnsafeRenderLines`'s live
// `"<line>:<expr>"` output has its line address stripped before comparing, via `exprMultiset` below) —
// the expression is what the guard actually cares about (does a raw, unescaped value reach the DOM),
// and unlike a line number it is stable under insertion, deletion and reformatting anywhere else in the
// file. The one wrinkle: the SAME expression can legitimately appear more than once in one component
// (TrashView.svelte's two `$t("trash.moreActions")` render positions) — a bare `Set` would silently
// collapse that back down to one, so comparison is by MULTISET (occurrence count), not by deduplicated
// membership: `exprMultiset`/`multisetDiff` below count occurrences on both sides and only pass when
// every expression's count matches exactly, in both directions. A mismatch message now names the
// component and the bare expression text first (no addresses to wade through — most of a 27-entry wall
// used to be noise around a one-word fact); `findUnsafeRenderLines` itself still reports real
// `"<line>:<expr>"` strings (other tests in this file and `bidiRenderScan.test.ts` depend on that shape
// unchanged) — only REGISTRY's own recorded keys and this file's comparison logic dropped the line half.
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { findUnsafeRenderLines, compareOffenders, isCandidateComponent } from "./bidiRenderScan";

const COMPONENTS = join(process.cwd(), "src", "lib", "components");
const APP = join(process.cwd(), "src", "App.svelte");
const DOC = join(process.cwd(), "src", "docs", "03-explorer.md");

/** Strip a `findUnsafeRenderLines` offender's leading `"<line>:"` address, leaving just the matched
 *  expression text. The line number is always digits immediately followed by the FIRST `:` in the
 *  string; the expression itself may contain further colons (e.g. a ternary, `"cond ? a : b"`), so this
 *  must anchor to the start of the string rather than splitting on the first `:` found anywhere. */
function exprOf(offender: string): string {
  return offender.replace(/^\d+:/, "");
}

/** `findUnsafeRenderLines`'s "<line>:<expr>" offenders, reduced to a sorted MULTISET of bare expression
 *  text — duplicates are kept (not deduplicated into a Set), so a component with the same expression
 *  rendered at two different lines still requires two matching recorded entries, not one. This is the
 *  key-by-expression comparison CPE-1885 introduces: two runs that report the identical bag of
 *  expressions are equal regardless of which lines they sat on. */
function exprMultiset(offenders: string[]): string[] {
  return offenders.map(exprOf).sort((a, b) => a.localeCompare(b));
}

/** Multiset difference: every element of `a` that isn't cancelled out by an equal element of `b`,
 *  respecting occurrence counts (so `multisetDiff(["x","x"], ["x"])` is `["x"]`, not `[]`) — a plain
 *  `.filter(x => !b.includes(x))` would over- or under-report duplicates. Used both directions: `a` found
 *  minus `b` recorded is what's newly raw; `a` recorded minus `b` found is what's gone stale. */
function multisetDiff(a: string[], b: string[]): string[] {
  const remaining = new Map<string, number>();
  for (const x of b) remaining.set(x, (remaining.get(x) ?? 0) + 1);
  const diff: string[] = [];
  for (const x of a) {
    const n = remaining.get(x) ?? 0;
    if (n > 0) remaining.set(x, n - 1);
    else diff.push(x);
  }
  return diff;
}

/** file -> the EXACT multiset of expression text `findUnsafeRenderLines` currently reports for it (line
 *  numbers stripped — see CPE-1885 above). Recomputed live every run and checked for multiset equality
 *  (not "offenders minus this array must be empty") — see the header above for why that specific shape
 *  closes round 1's inert-allowlist hole, and CPE-1885's note for why the key is the expression rather
 *  than the line. A non-empty array is NOT necessarily a disclosed spoof risk: most entries here are UI
 *  text this engine can't prove safe (i18n params, counts, labels, diagnostic error/note/reason strings,
 *  diff/metadata CONTENT, macro/workspace/rule/ticket/agent identity strings) — read
 *  `bidiEscape.doc-parity` below for which specific files' entries are an actual disclosed
 *  filesystem-name/path gap vs. harmless-but-unprovable text. */
const REGISTRY: Record<string, string[]> = {
  "ConflictDialog.svelte": ["opLabel ? `— ${opLabel}` : \"\"","unresolved","f.label","opLabel.toLowerCase()","showBase ? \"Hide\" : \"Show\"","versions.base ?? \"— absent —\"","versions.ours ?? \"— absent —\"","versions.theirs ?? \"— absent —\"","error || note || `${opLabel || \"No\"} operation in progress`","unresolved > 0 ? \"Resolve every file first\" : `Continue the ${opLabel.toLowerCase()}`","opLabel.toLowerCase()"],
  "FileNameSearchDialog.svelte": ["$t(\"search.findByNameTitle\")","$t(\"search.docsTitle\")","$t(\"common.close\")","$t(\"search.button\")","$t(\"search.searching\")","error","$t(\"search.noNameMatches\")","hits.length === 1 ? $t(\"search.matchOne\", { count: hits.length }) : $t(\"search.matchMany\", { count: hits.length })","$t(\"search.truncated\")"],
  "RepoBrowser.svelte": ["isGeneric ? \"Git URL\" : \"Repository\"","isGeneric ? \"https only\" : \"private repos\"","loading ? \"Browsing…\" : \"Browse\"","cloning ? \"Cloning…\" : \"Clone\"","provider","statusText","consent.host","repo","fmtSize(e.size)","loaded ? repo : \"No repository open\""],
  "AgentTimeline.svelte": ["agentName", "entries.length", "diff ? `${displaySafePath(e.path)} — hover to see what changed` : displaySafePath(e.path)", "KIND_LABEL[e.kind]", "stats.add", "stats.del", "clock(e.at)", "playing ? \"Pause\" : \"Play\"", "playing ? \"Pause\" : \"Play\"", "s", "cpMarkerTitle(m)", "`Checkpoint ${m.cp.label || cpShortId(m.cp.manifest_id)}`", "Math.round(sliderFraction(range, t) * 100)", "new Date(t).toLocaleTimeString()", "selectedCheckpoint.manifest_id", "selectedCheckpoint.label || cpShortId(selectedCheckpoint.manifest_id)", "cpTime(selectedCheckpoint.ts)", "revertPreviewError", "revertPreview.creates", "revertPreview.overwrites", "revertPreview.deletes", "formatBytes(revertPreview.bytes_written)", "revertPreview.drift_count", "revertPreview.drift_count", "revertPreview.drift_count === 1 ? \"\" : \"s\"", "revertError", "selectedCheckpoint.label || cpShortId(selectedCheckpoint.manifest_id)", "revertPreview.drift_count", "revertPreview.drift_count === 1 ? \"\" : \"s\"", "KIND_LABEL[replayCurrent.kind]", "clock(replayCurrent.at)", "replayKindLabel(re.kind)", "clock(re.ts)", "KIND_LABEL[e.kind]", "clock(e.at)", "c.sessionId", "formatTokens(c.inputTokens)", "formatTokens(c.outputTokens)", "formatTokens(c.totalTokens)", "formatUsd(c.costUsd)", "formatTokens(c.filesTouched)", "formatTokens(c.editCount)", "formatBytes(c.churnBytes)", "formatDuration(c.wallClockMs)", "formatPerMinute(c.tokensPerMinute)", "formatUsd(c.usdPerFile)", "formatBytes(c.churnPer1kTokens)", "relativeLabel(o.lastAt, Date.now())", "friendlyActor(a, sessions)", "rc.kind === \"divergence\" ? \"diverged\" : \"collided\"", "relativeLabel(rc.lastAt, Date.now())", "friendlyActor(a, sessions)", "renameConflictNote(rc.kind)", "historyError", "formatTokens(historyRollup.totals.sessions)", "formatUsd(historyRollup.totals.costUsd)", "formatTokens(historyRollup.totals.totalTokens)", "formatDuration(historyRollup.totals.wallClockMs)", "formatTokens(historyRollup.totals.filesTouched)", "formatBytes(historyRollup.totals.churnBytes)", "historyRollup.totals.sessions", "historyRollup.totals.sessions === 1 ? \"\" : \"s\"", "historyUnclean", "historyUnclean === 1 ? \"its\" : \"their\"", "historyUnclean === 1 ? \"is\" : \"are\"", "historyUnclean === 1 ? \"it\" : \"them\"", "formatPerMinute(historyRollup.ratios.tokensPerMinute)", "formatUsd(historyRollup.ratios.usdPerSession)", "formatUsd(historyRollup.ratios.usdPerFile)", "formatBytes(historyRollup.ratios.churnPer1kTokens)", "row.model", "formatTokens(row.sessions)", "formatTokens(row.totalTokens)", "formatUsd(row.costUsd)", "historyShare(row.costUsd, historyRollup.totals.costUsd)", "row.agentName", "formatTokens(row.sessions)", "formatTokens(row.totalTokens)", "formatUsd(row.costUsd)", "historyShare(row.costUsd, historyRollup.totals.costUsd)", "new Date(row.startedAt).toLocaleString()", "row.agentName || row.agentId", "row.agentName || row.agentId || \"(unknown)\"", "historyDurationLabel(row)", "isSessionEndedCleanly(row) ? \"Clean\" : \"Ended unexpectedly\"", "historyMetric === 'cost' ? 'Cost' : 'Tokens'", "historyBarDate(p.bucketStart)", "historyMetric === \"cost\" ? formatUsd(v) : formatTokens(v)"],
  "ConsultedFiles.svelte": ["$agentConsulted.length","e.count"],
  "SessionHistoryDialog.svelte": ["s","k","error","formatDate(e.ts)","e.kind","filtered.length","filtered.length === 1 ? \"\" : \"s\""],
  "IntegrityDialog.svelte": ["hasBaseline ? `Baseline: ${baseline.length} files` : \"No baseline stored\"","note","error","report.corrupted.length","report.missing.length","report.edited.length","report.new.length","report.intact.length","label","list.length","report.intact.length"],
  "CheckpointDialog.svelte": ["error", "note", "cp.label || shortId(cp.manifest_id)", "fmtTime(cp.ts)", "shortId(cp.manifest_id)", "$t('ckpt.failedTitle')", "cf.reason", "$t(\"ckpt.failedTitle\")", "cf.operation", "fmtTime(cf.ts)", "cf.reason", "preview.creates", "preview.overwrites", "preview.deletes", "fmtBytes(preview.bytes_written)", "preview.drift_count", "diffOpenPath === p ? \"Close diff\" : \"Open diff\"", "diffError", "selected.label || shortId(selected.manifest_id)", "selected.label || shortId(selected.manifest_id)"],
  // CPE-1845 — the shared revert-result panel used by CheckpointDialog, AgentTimeline and
  // CopilotDialog. Every path AND every backend-supplied reason on this page goes through
  // displaySafePath/displaySafeName, including the per-failure `error` strings: `apply_delete` and
  // `apply_write` format those as `"{target}: {os error}"`, so a USER-CONTROLLED FILENAME rides inside
  // them. Review round 2 was right that the earlier note here ("the same class as most other entries")
  // was wrong — most entries are counts and labels, this one provably carries a filename — so the
  // strings are escaped and what is left recorded below is a count and a literal.
  // CPE-1869 adds one more raw line: the copy-full-list button's label. It interpolates only
  // `summary.heldBack` (a count) and the literal "Copied"/`""`/`"s"` — no path or backend-authored
  // prose — so it is the same "count/literal, provably safe" class as `summary.more` beside it, not a
  // new filename/path surface. (CPE-1885: carried across as bare expression text, line prefix dropped,
  // when this PR rebased onto #1026 after it merged.)
  "RevertOutcomePanel.svelte": [
    "headline",
    "summary.reason",
    "summary.nextStep",
    "copied ? \"Copied\" : `Copy all ${summary.heldBack} held-back path${summary.heldBack === 1 ? \"\" : \"s\"}`",
    "summary.more",
  ],
  "DiffSideBySide.svelte": ["r.left ?? \"\"","r.right ?? \"\""],
  "InspectCryptoDialog.svelte": [],
  "BoardView.svelte": ["error","boardQuery.trim()","boardQuery.trim()","col","list.length","showArchived ? \"hide\" : `+${archivedEpicList.length} archived`","\"Open \" + e.id + \" — details\"","e.id","\"Copy \" + e.id","copiedId === e.id ? \"✓\" : \"⧉\"","e.status","e.title","bar.state === \"empty\" ? \"No sub-tickets yet\" : bar.state === \"complete\" && p.total === 0 ? \"Epic complete\" : p.done + \" of \" + p.total + \" tickets done\"","bar.label","col","list.length","showArchived ? \"hide\" : `+${archived.length} archived`","\"Open \" + c.id + \" — details\"","c.id","\"Copy \" + c.id","copiedId === c.id ? \"✓\" : \"⧉\"","c.priority","c.title","c.epic","c.sprint","t","grouped[l].length","l","error || note || \"\""],
  "CopilotDialog.svelte": ["planError", "phase === \"planning\" ? \"Planning…\" : \"Plan\"", "instruction", "v", "planResult.summary.moves", "planResult.summary.moves === 1 ? \"\" : \"s\"", "planResult.summary.renames", "planResult.summary.renames === 1 ? \"\" : \"s\"", "planResult.summary.deletes", "planResult.summary.deletes === 1 ? \"\" : \"s\"", "planResult.summary.mkdirs", "planResult.summary.mkdirs === 1 ? \"\" : \"s\"", "planResult.summary.copies", "opKind(op)", "execError", "v", "execResult.results.filter((r) => r.ok).length", "execResult.results.length", "r.error", "execResult.checkpoint.checkpoint.manifest_id", "undoing ? \"Undoing…\" : \"Undo\"", "undoError"],

  // --- B4: the 19 components CPE-1712 itself originally escaped ---------------------------------
  "FileList.svelte": ["$t(\"fl.columnsButton\")","$t(\"fl.columnsButton\")","$t(\"fl.sortBy\", { col: $t(col.labelKey) })","$t(col.labelKey)","$t(\"fl.sortBy\", { col: ac.col.label })","ac.col.label","$t(\"fl.resizeColumn\", { col: handleLabel(i) })","$t(\"fl.resizeTip\")","error","$t(\"fl.loading\")","searching ? $t(\"fl.noMatch\") : $t(\"fl.empty\")","tagEntry.label","ruleStyle.label","tag","$t(ACTIVITY_LABEL_KEY[act.kind])","$t(\"fl.agentInside\")","formatDate(entry.modified)","typeName(entry)","folderSizes.has(entry.path) ? formatSize(folderSizes.get(entry.path) ?? 0) : \"…\"","formatSize(entry.size)","cell.display","$t(\"fl.agentLegend\")","friendlyActor(a, sessions)"],
  "Sidebar.svelte": ["agentsOpen ? \"Collapse\" : \"Expand\"","$t(\"sidebar.agents\")","`${s.agentName}${s.provider ? \" · \" + s.provider : \"\"}${s.model ? \" · \" + s.model : \"\"} · ${s.cwd} (double-click to open its tab · right-click for more)`","sessionNum(s.sessionId)","model","s.agentName || s.agentId || \"Agent\"","baseName(s.cwd)","favOpen ? \"Collapse\" : \"Expand\"","tagsOpen ? \"Collapse\" : \"Expand\"","`${count} item${count === 1 ? \"\" : \"s\"} tagged “${tag}” — click to filter, right-click to rename/delete`","tag","count","smartOpen ? \"Collapse\" : \"Expand\"","$t(\"smart.section\")","$t(\"smart.itemTip\", { tag: sf.tag })","sf.name","savedSearchOpen ? \"Collapse\" : \"Expand\"","$t(\"smart.searchSection\")","$t(\"smart.searchItemTip\")","ss.name","exploreOpen ? \"Collapse\" : \"Expand\"","$t(\"sidebar.explore\")","$t(\"sidebar.repositories\")","placesOpen ? \"Collapse\" : \"Expand\"","$t(\"sidebar.quickAccess\")","drivesOpen ? \"Collapse\" : \"Expand\"","$t(\"sidebar.drives\")","open ? \"Collapse\" : \"Expand\"","`${formatSize(u.free)} free of ${formatSize(u.total)}`","formatSize(u.free)","networkOpen ? \"Collapse\" : \"Expand\"","`${conn.scheme}://${conn.host} — ${stateTitle(state, connectionErrors[conn.name])} (right-click for more)`","savable ? `${displaySafePath(s.path)} — discovered on your network; click to add it as a connection` : `${displaySafePath(s.path)} — discovered on your network; ${prefill.scheme.toUpperCase()} isn't supported yet`","trashOpen ? \"Collapse\" : \"Expand\"","$t(\"sidebar.trash\")","$t(\"trash.openTip\")","$t(\"trash.open\")","$t(\"trash.macMessage\")","$t(\"trash.macLabel\")"],
  "TabBar.svelte": ["$t(\"app.closeTab\")","$t(\"app.newTab\")"],
  "HomeView.svelte": ["quickOpen ? $t(\"home.collapse\") : $t(\"home.expand\")","$t(\"home.quickAccess\")","$t(\"home.unpinQuick\")","recentOpen ? $t(\"home.collapse\") : $t(\"home.expand\")","tab === \"favorites\" ? $t(\"home.favorites\") : tab === \"folders\" ? $t(\"home.recentFolders\") : tab === \"shared\" ? $t(\"home.shared\") : $t(\"home.recent\")","$t(\"home.clear\")","$t(\"home.addNetworkLocation\")","$t(\"home.recent\")","$t(\"home.favorites\")","$t(\"home.folders\")","$t(\"home.shared\")","$t(\"home.noRecent\")","$t(\"home.noRecentSub\")","$t(\"home.dateOpened\")","$t(\"home.name\")","formatDate(r.opened)","$t(\"home.removeFromRecent\")","$t(\"home.removeFromRecent\")","$t(\"home.noFavorites\")","$t(\"home.noFavoritesSub\")","$t(\"home.removeFromFavorites\")","$t(\"home.noRecentFolders\")","$t(\"home.noRecentFoldersSub\")","$t(\"home.removeFromRecentFolders\")","$t(\"home.removeFromRecentFolders\")","$t(\"home.add\")","$t(\"common.cancel\")","$t(\"home.sharedLoading\")","$t(\"home.noShared\")","$t(\"home.noSharedSub\")","$t(\"home.removeNetworkLocation\")","$t(\"home.removeNetworkLocation\")"],
  "DetailsPane.svelte": ["typeName(one)","formatSize(one.size) || \"0 B\"","formatDate(one.modified) || \"—\"","selected.length","selected.filter((e) => e.is_dir).length","selected.filter((e) => !e.is_dir).length","formatSize(totalSize) || \"0 B\"","itemCount","itemCount === 1 ? \"\" : \"s\""],
  // CPE-1827: line numbers reshuffled by the titlebar overflow-menu rewrite (recomputed via
  // `findUnsafeRenderLines` against the new file — see the ticket's Work Log). Same offenders as before
  // (all i18n params/labels/counts, none a raw filesystem name/path) plus two new `$t("trash.moreActions")`
  // entries (the overflow trigger's `title`/`aria-label`) and one new `$t("trash.docs")` (the overflow
  // menu's Docs row, replacing the removed `HelpButton` usage) — all plain static-key i18n lookups.
  "TrashView.svelte": ["$t(\"trash.title\")","itemCountLabel","selectedCountLabel","$t(\"trash.stillLoading\")","selectedCountLabel","selectedCountLabel","$t(\"trash.moreActions\")","$t(\"trash.moreActions\")","allSelected ? $t(\"trash.deselectAll\") : $t(\"trash.selectAll\")","$t(\"trash.restoreSelected\")","$t(\"trash.emptySelected\")","$t(\"trash.emptyAll\")","$t(\"trash.refresh\")","$t(\"trash.docs\")","$t(\"trash.restoreFailed\", { name: displaySafeName(f.name), error: f.error })","$t(\"trash.loading\")","$t(\"trash.error\", { error })","noticeMessage","$t(\"trash.empty\")","degradedMessage","$t(\"trash.selectAll\")","$t(\"trash.columnsName\")","$t(\"trash.columnsOriginalPath\")","$t(\"trash.columnsDeleted\")","formatSize(e.size)","formatDate(e.time_deleted * 1000)","$t(\"trash.emptyConfirmTitle\")"],
  "NavToolbar.svelte": ["$t('nav.back')","$t('nav.forward')","$t('nav.up')","$t('nav.refresh')","density === \"compact\" ? \"Switch to comfortable density\" : \"Switch to compact density\"","density === \"compact\" ? \"Switch to comfortable density\" : \"Switch to compact density\"","$t('nav.search')","searchScope","$t(\"nav.searchHint\")"],
  "PropertiesDialog.svelte": ["$t(\"prop.title\")","$t(\"common.close\")","error","$t(\"prop.type\")","typeName(single)","$t(\"prop.location\")","$t(\"prop.size\")","$t(\"prop.calculating\")","$t(\"prop.sizeBytes\", { size: formatSize(folderSize) || \"0 B\", bytes: folderSize.toLocaleString() })","$t(\"prop.unavailable\")","$t(\"prop.size\")","$t(\"prop.sizeBytes\", { size: formatSize(single.size) || \"0 B\", bytes: single.size.toLocaleString() })","$t(\"prop.created\")","formatDate(info.created) || \"—\"","$t(\"prop.modified\")","formatDate(info.modified) || \"—\"","$t(\"prop.attributes\")","[info.readonly ? $t(\"prop.readonly\") : null, info.hidden ? $t(\"prop.hidden\") : null] .filter(Boolean) .join(\", \") || $t(\"prop.none\")","label","value","label","value","$t(\"prop.typeMismatch\")","inspection.type_mismatch","checksum","$t(\"prop.copyChecksumTip\")","copied ? $t(\"prop.copied\") : $t(\"prop.copy\")","$t(\"prop.match\")","$t(\"prop.matchTip\")","$t(\"prop.noMatch\")","$t(\"prop.noMatchTip\")","$t(\"prop.computing\")","hashError","$t(\"prop.compute\")","$t(\"prop.contents\")","$t(\"prop.contentStats\", { lines: stats.lines.toLocaleString(), words: stats.words.toLocaleString(), chars: stats.chars.toLocaleString() })","$t(\"prop.counting\")","statError","$t(\"prop.count\")","$t(\"prop.itemsSelected\", { count: entries.length })","$t(\"prop.folders\")","folderCount","$t(\"prop.files\")","fileCount","$t(\"prop.sizeOfFiles\")","$t(\"prop.sizeBytes\", { size: formatSize(totalSize) || \"0 B\", bytes: totalSize.toLocaleString() })","$t(\"prop.folderNote\")","$t(\"prop.note\")","nativeStoreName","tag","nativeEntry.label || \"None\"","nativePulling ? \"Pulling…\" : \"Pull\"","nativeError","$t(\"common.close\")"],
  "InstantSearch.svelte": ["$t(\"search.instantTitle\")","$t(\"search.instantTitle\")","$t(\"search.docsTitle\")","$t(\"common.close\")","$t(\"search.instantPlaceholder\")","$t(\"search.instantOffTitle\")","$t(\"search.instantOffBody\")","$t(\"search.buildingIndex\", { count: buildStats?.dirs_scanned ?? 0 })","buildError","$t(\"search.buildIndex\")","$t(\"search.instantOpenFolderFirst\")","$t(\"search.searching\")","error","$t(\"search.instantTypeHint\")","$t(\"search.instantNoMatches\")"],
  "ArchiveSafetyDialog.svelte": ["$t(\"arcsafe.title\")","$t(\"arcsafe.title\")","$t(\"common.close\")","$t(\"arcsafe.scanning\")","error","$t(\"arcsafe.retry\")","$t(\"arcsafe.unreadable\")","$t(\"arcsafe.encrypted\", { count: result.unreadable_entries })","$t(\"arcsafe.dangerous\")","$t(\"arcsafe.encrypted\", { count: result.unreadable_entries })","$t(\"arcsafe.safe\")","$t(\"arcsafe.ratio\")","ratioLabel(result.report.overall_ratio)","$t(\"arcsafe.sizes\")","sizeLabel(result.report.total_compressed)","sizeLabel(result.report.total_uncompressed)","$t(\"arcsafe.entries\")","result.entries_scanned.toLocaleString()","$t(\"arcsafe.capped\")","$t(\"arcsafe.unreadableEntries\")","result.unreadable_entries.toLocaleString()","$t(\"arcsafe.flaggedHead\", { count: result.report.flagged.length })","ratioLabel(f.ratio)","$t(\"arcsafe.noneFlagged\")"],
  "PreviewPane.svelte": ["$t(action.labelKey)","$t(action.labelKey)","actionMessage","$t(\"pv.model.title\")","$t(\"pv.model.format\")","modelFormatLabel","$t(\"pv.model.encoding\")","modelInfo.ascii ? $t(\"pv.model.ascii\") : $t(\"pv.model.binary\")","$t(\"pv.model.meshes\")","modelInfo.mesh_count.toLocaleString()","modelCountLabel","modelInfo.triangle_count.toLocaleString()","$t(\"pv.model.vertices\")","modelInfo.vertex_count.toLocaleString()","$t(\"pv.model.dimensions\")","fmtDim(modelDims.d)","fmtDim(modelDims.h)","fmtDim(modelDims.w)","$t(\"pv.dicom.title\")","name","value","$t(\"pv.loading\")","$t(\"pv.cantImage\")","$t(\"pv.loading\")","$t(\"pv.loading\")","$t(\"pv.loading\")","$t(\"pv.loading\")","$t(\"pv.loading\")","$t(\"pv.cantArchive\")","e.is_dir ? \"\" : formatSize(e.size)","entries.length === 1 ? $t(\"pv.itemOne\", { count: entries.length }) : $t(\"pv.itemMany\", { count: entries.length })","$t(\"pv.loading\")","$t(\"pv.cantFile\")","info","$t(\"pv.loading\")","$t(\"pv.cantFile\")","saving ? $t(\"pv.saving\") : $t(\"pv.save\")","$t(\"common.cancel\")","saveError","$t(\"pv.json.viewTree\")","$t(\"pv.json.viewRaw\")","$t(\"pv.edit\")","cell","$t(\"pv.showingRows\", { cap: CSV_ROW_CAP, total: tableRows.length })","prettyJson(text)","mdHtml","breadcrumbSym.name","`Jump to ${sym.kind} ${sym.name}, line ${sym.line}`","`${sym.name} — line ${sym.line}`","sym.name","foldCollapsed.has(i + 1) ? `Expand lines ${i + 1}–${foldByStart.get(i + 1)?.end_line}` : `Collapse lines ${i + 1}–${foldByStart.get(i + 1)?.end_line}`","foldLen(i + 1)","line","$t(\"menu.cut\")","$t(\"menu.copy\")","$t(\"menu.paste\")","$t(\"ctx.selectAll\")"],
  "QuickLook.svelte": ["images.length","index + 1"],
  "DiskSpaceView.svelte": ["formatSize(total)","loading ? \" · scanning…\" : \"\"","error","formatSize(c?.size ?? 0)","pct(c?.size ?? 0)","formatSize(c?.size ?? 0)","formatSize(c.size)"],
  "DropStackPanel.svelte": ["open ? \"Hide Drop Stack\" : \"Show Drop Stack\"","$dropStackEntries.length","canTransfer ? \"Move every shelved item into the current folder\" : \"Not a valid destination for the Drop Stack\"","canTransfer ? \"Copy every shelved item into the current folder\" : \"Not a valid destination for the Drop Stack\""],
  "FolderBrowser.svelte": ["$t(\"pv.loading\")","$t(\"pv.folder.cantOpen\")","$t(\"fl.empty\")","formatSize(entry.size)"],
  "SidebarNode.svelte": ["open ? \"Collapse\" : \"Expand\""],
  "RunCommandConfirm.svelte": ["commands.length","commands.length === 1 ? \"command\" : \"commands\"","cwd ? ` in ${displaySafePath(cwd)}` : \"\"","running ? \"Running…\" : \"Run\"","r.command","r.error","r.code ?? \"signal\"","r.truncated ? \" · output truncated\" : \"\"","r.stdout","r.stderr"],
  "ContentSearchDialog.svelte": ["$t(\"search.inFilesTitle\")","$t(\"search.docsTitle\")","$t(\"common.close\")","$t(\"search.matchCase\")","$t(\"search.button\")","$t(\"search.searching\")","error","$t(\"search.noMatchesInFolder\")","$t(\"search.filterResultsAria\")","$t(\"search.matchesInFiles\", { matches: result.matches.length === 1 ? $t(\"search.matchOne\", { count: result.matches.length }) : $t(\"search.matchMany\", { count: result.matches.length }), files: groups.length === 1 ? $t(\"search.fileOne\", { count: groups.length }) : $t(\"search.fileMany\", { count: groups.length }), })","$t(\"search.shown\", { count: shownGroups.length })","$t(\"search.truncated\")","$t(\"search.noFilesMatch\", { query: resultFilter.trim() })","$t(\"search.toggleFile\")","collapsedFiles.has(g.path) ? \"▸\" : \"▾\"","collapsedFiles.has(g.path) ? $t(\"home.expand\") : $t(\"home.collapse\")","g.matches.length","mt.line_number","seg.text"],
  "DuplicatesDialog.svelte": ["$t(\"dup.title\")","$t(\"common.close\")","$t(\"dup.intro\")","$t(\"dup.scan\")","$t(\"dup.scanning\")","error","$t(\"dup.none\", { count: result.files_scanned.toLocaleString() })","result.groups.length === 1 ? $t(\"dup.summaryOne\", { count: result.groups.length, size: formatSize(reclaimable) || \"0 B\" }) : $t(\"dup.summaryMany\", { count: result.groups.length, size: formatSize(reclaimable) || \"0 B\" })","$t(\"dup.capped\")","$t(\"dup.selectRedundant\")","$t(\"dup.selectRedundantTip\")","deleting ? $t(\"dup.removing\") : $t(\"dup.moveToBin\", { count: selected.size })","$t(\"dup.copiesEach\", { count: g.paths.length, size: formatSize(g.size) || \"0 B\" })","$t(\"dup.extra\", { size: formatSize(g.size * (g.paths.length - 1)) || \"0 B\" })","$t(\"dup.markForBin\")"],

  // --- The ticket's originally-disclosed "not yet covered" dialogs — pinned exactly, not fixed here ---
  "ContentIndexSearchDialog.svelte": ["$t(\"search.byContentTitle\")","baseName(root) || root","root","$t(\"search.rebuildContentIndex\")","$t(\"search.rebuildContentIndex\")","$t(\"search.docsTitle\")","$t(\"common.close\")","$t(\"search.byContentPlaceholder\")","$t(\"search.byContentNeedsBuildTitle\")","$t(\"search.byContentNeedsBuildBody\")","$t(\"search.buildingContentIndex\", { count: buildProgress?.files_indexed ?? 0 })","buildProgress.current_path","buildError","$t(\"search.buildContentIndex\")","$t(\"search.checkingContentIndex\")","$t(\"search.searching\")","error","$t(\"search.byContentTypeHint\")","$t(\"search.byContentNoMatches\")","$t(\"search.buildingContentIndex\", { count: buildProgress?.files_indexed ?? 0 })","hits.length === 1 ? $t(\"search.matchOne\", { count: hits.length }) : $t(\"search.matchMany\", { count: hits.length })","h.path","baseName(h.path)","relativeToRoot(h.path, root)","$t(\"search.byContentScoreTitle\")","scorePercent(h.score)","seg.text"],
  "FileHealthDialog.svelte": ["$t(\"fh.title\")","$t(\"fh.title\")","baseName(root) || root","root","$t(\"common.close\")","$t(tab.labelKey)","$t(\"fh.excludeLabel\")","pattern","$t(\"fh.excludeRemove\")","$t(\"fh.excludeRemove\")","$t(\"fh.excludeEmpty\")","$t(\"fh.excludeAddLabel\")","$t(\"fh.excludeSuggest\")","s","$t(\"fh.excludeHint\")","$t(\"fh.intro\")","$t(\"fh.scan\")","$t(\"fh.scanning\")","error","$t(\"fh.scan\")","$t(\"fh.none\", { count: scanned.toLocaleString() })","$t(\"fh.scan\")","links.length === 1 ? $t(\"fh.summaryOne\", { count: links.length }) : $t(\"fh.summaryMany\", { count: links.length })","$t(\"fh.scanned\", { count: scanned.toLocaleString() })","$t(\"fh.capped\")","$t(\"fh.scan\")","l.path","baseName(l.path)","parentDir(l.path)","reasonLabel(l.reason)","$t(\"fh.introMismatch\")","$t(\"fh.scan\")","$t(\"fh.scanning\")","mismatchError","$t(\"fh.scan\")","$t(\"fh.noneMismatch\", { count: mismatchScanned.toLocaleString() })","$t(\"fh.scan\")","mismatchHits.length === 1 ? $t(\"fh.summaryOneMismatch\", { count: mismatchHits.length }) : $t(\"fh.summaryManyMismatch\", { count: mismatchHits.length })","$t(\"fh.scanned\", { count: mismatchScanned.toLocaleString() })","$t(\"fh.capped\")","$t(\"fh.scan\")","h.path","baseName(h.path)","parentDir(h.path)","$t(\"fh.mismatchBadge\", { claimed: h.claimedExt, detected: h.detectedLabel })","h.fixError","$t(\"fh.mismatchFix\", { ext: h.detectedExt })","h.fixing ? $t(\"fh.mismatchFixing\") : $t(\"fh.mismatchFix\", { ext: h.detectedExt })","$t(\"fh.introOrphan\")","$t(\"fh.scan\")","$t(\"fh.scanning\")","orphanError","$t(\"fh.scan\")","$t(\"fh.noneOrphan\", { count: orphanScanned.toLocaleString() })","$t(\"fh.scan\")","orphans.length === 1 ? $t(\"fh.summaryOneOrphan\", { count: orphans.length }) : $t(\"fh.summaryManyOrphan\", { count: orphans.length })","$t(\"fh.scanned\", { count: orphanScanned.toLocaleString() })","$t(\"fh.capped\")","$t(\"fh.scan\")","o.path","baseName(o.path)","parentDir(o.path)","$t(\"fh.orphanBadge\")","$t(\"fh.introEmpty\")","$t(\"fh.scan\")","$t(\"fh.scanning\")","emptyError","$t(\"fh.scan\")","$t(\"fh.noneEmpty\", { count: emptyScanned.toLocaleString() })","$t(\"fh.scan\")","emptyDirs.length === 1 ? $t(\"fh.summaryOneEmpty\", { count: emptyDirs.length }) : $t(\"fh.summaryManyEmpty\", { count: emptyDirs.length })","$t(\"fh.scanned\", { count: emptyScanned.toLocaleString() })","$t(\"fh.capped\")","$t(\"fh.scan\")","d.path","baseName(d.path)","parentDir(d.path)"],
  "NearDuplicatesDialog.svelte": ["title","title","baseName(root) || root","root","$t(\"common.close\")","$t(\"nd.intro\")","$t(\"nd.scan\")","$t(\"nd.scanning\")","error","$t(\"nd.scan\")","$t(\"nd.none\", { count: scannedCount.toLocaleString() })","$t(\"nd.scan\")","groups.length === 1 ? $t(\"nd.summaryOne\", { count: groups.length }) : $t(\"nd.summaryMany\", { count: groups.length })","$t(\"sim.scanned\", { count: scannedCount.toLocaleString() })","$t(\"sim.capped\")","$t(\"nd.scan\")","$t(\"nd.selectExtrasTip\")","$t(\"sim.selectExtras\")","deleting ? $t(\"sim.removing\") : $t(\"sim.moveToBin\", { count: selected.size })","$t(\"nd.groupHead\", { count: g.paths.length })","$t(\"nd.markForBin\")","p","baseName(p)","parentDir(p)"],
  "SimilarImagesDialog.svelte": ["$t(\"sim.title\")","$t(\"sim.title\")","baseName(root) || root","root","$t(\"common.close\")","$t(\"sim.intro\")","$t(\"sim.scan\")","$t(\"sim.scanning\")","error","$t(\"sim.scan\")","$t(\"sim.none\", { count: filesScanned.toLocaleString() })","$t(\"sim.scan\")","groups.length === 1 ? $t(\"sim.summaryOne\", { count: groups.length }) : $t(\"sim.summaryMany\", { count: groups.length })","$t(\"sim.scanned\", { count: filesScanned.toLocaleString() })","$t(\"sim.capped\")","$t(\"sim.scan\")","$t(\"sim.selectExtras\")","$t(\"sim.selectExtrasTip\")","deleting ? $t(\"sim.removing\") : $t(\"sim.moveToBin\", { count: selected.size })","$t(\"sim.groupHead\", { count: g.paths.length })","$t(\"sim.markForBin\")","p","baseName(p)","parentDir(p)"],
  "DeclutterDialog.svelte": ["$t(\"dc.title\")","$t(\"dc.title\")","baseName(root) || root","root","$t(\"common.close\")","$t(\"dc.intro\")","$t(\"dc.scan\")","$t(\"dc.scanning\")","error","$t(\"dc.scan\")","$t(\"dc.none\")","$t(\"dc.scan\")","findings.length === 1 ? $t(\"dc.summaryOne\", { count: findings.length }) : $t(\"dc.summaryMany\", { count: findings.length })","$t(\"dc.scan\")","deleting ? $t(\"dc.removing\") : $t(\"dc.moveToBin\", { count: selected.size })","g.rows.length","reasonLabel(g.reason)","$t(\"dc.markForBin\")","f.path","f.name"],
  "BatchMediaDialog.svelte": ["paths.length","paths.length === 1 ? \"\" : \"s\"","watermarkImage || \"No image chosen — no watermark\"","watermarkImage ? baseName(watermarkImage) : \"No image chosen (no watermark)\"","$t(\"bm.renameEscapes\")","$t(\"bm.convertEscapes\")","mediaOpLabel(op)","RENAME_DEFAULT_TEMPLATE","baseName(it.input)","it.input","baseName(it.output)","it.output","it.summary","MAX_PREVIEW","previewCappedTotal","planError","applyError","planned.length","planned.length === 1 ? \"\" : \"s\"","done","failed > 0 ? `, ${failed} failed` : \"\"","total","completed.skipped.length","completed.written","s.name","s.reason","checkpointFailures.length","checkpointFailures.length === 1 ? \"\" : \"s\"","baseName(dir) || dir","dir","checkpointFailures.length === 1 ? \"that folder\" : \"those folders\"","checkpointPartial.length","checkpointPartial.length === 1 ? \"\" : \"s\"","p.dir","baseName(p.dir) || p.dir","p.skippedCount","p.skippedCount === 1 ? \"\" : \"s\"","checkpointPartial.length === 1 ? \"that folder\" : \"those folders\"","overwriteItems.length","overwriteItems.length === 1 ? \"\" : \"s\"","uniqueParentDirs(overwriteItems.map((it) => it.input)).length === 1 ? \"\" : \"s\"","overwriteItems.length","overwriteItems.length === 1 ? \"\" : \"s\"","applying ? \"Applying…\" : \"Apply\""],
  "SplitFileDialog.svelte": ["baseName(path)","result.part_count","result.part_count === 1 ? \"\" : \"s\"","result.part_count","formatSize(result.part_size)","formatSize(result.total_size)","outDir","baseName(path)","p.label","outDir","error","busy ? \"Splitting…\" : \"Split\""],
  "JoinPartsDialog.svelte": ["baseName(joinedPath)","joinedPath","baseName(path)","preview.partCount","formatSize(preview.totalSize)","previewError","outPath","error","busy ? \"Joining…\" : \"Join\""],
  "ExplorerPane.svelte": ["$t(\"menu.view\")","$t(\"view.details\")","$t(\"view.list\")","$t(\"tb.icons\")","$t(\"view.gallery\")","$t(\"tb.sortBy\")","$t(\"sort.name\")","$t(\"tb.modified\")","$t(\"sort.type\")","$t(\"sort.size\")","$t(\"tb.direction\")","$t(\"cmd.ascending\")","$t(\"cmd.descending\")","$t(\"cmd.showHidden\")","$t(\"tb.fileList\")","$t(\"agent.watch\", { name: watchedAgentName })","baseName(c.path)","c.kind === \"removed\" ? \"−\" : c.kind === \"created\" ? \"+\" : \"~\"","c.path","$t(\"agent.watching\")","$t(\"agent.showLog\")","$agentTimeline.length ? `(${$agentTimeline.length})` : \"\"","$t(\"agent.log\")","selectedTag","visible.length"],
  "TerminalPanel.svelte": ["t.cwd","basename(t.cwd) || \"shell\"","c.label","openError"],

  // --- CPE-1768: newly-registered candidates discovered by the membership-rule sweep (45 files) ---
  "AboutDialog.svelte": ["version || \"—\"","s.id","s.name","s.version || \"—\"","s.contract || \"—\"","h.label"],
  "AttributesDialog.svelte": ["heading","targets.length","error","modePreview","ch","error","notice"],
  "BackupDashboard.svelte": ["job.name","progress","total ? ` / ${total}` : \"\"","fmtTime(st.when)","st.failed ? `, ${st.failed} failed` : \"\"","st.label","st.ok","history[job.id].length","history[job.id].length === 1 ? \"\" : \"s\"","showHistory === job.id ? \"▾\" : \"▸\"","fmtTime(run.when)","run.failed ? `, ${run.failed} failed` : \"\"","run.label","run.ok","error","plan.copy.length","plan.delete.length","plan.unchanged","plan.update.length"],
  "BinaryPreview.svelte": ["loadError","fmtCount(info.sections.length)","fmtCount(info.imports.length)","fmtCount(info.exports.length)","fmtCount(info.symbols.length)","formatLabel(info.format)","info.arch ?? \"Unknown\"","info.is_64 ? \"64-bit\" : \"32-bit\"","formatSize(size)","fmtCount(info.sections.length)","fmtCount(info.imports.length)","fmtCount(info.exports.length)","fmtCount(info.symbols.length)","formatSize(s.size)","hexAddress(s.address)","s.name","fmtCount(BINARY_TABLE_ROW_CAP)","fmtCount(sectionsCap.total)","i.library ?? \"—\"","i.name","fmtCount(BINARY_TABLE_ROW_CAP)","fmtCount(importsCap.total)","e.name","hexAddress(e.address)","fmtCount(BINARY_TABLE_ROW_CAP)","fmtCount(exportsCap.total)","info.format === \"Pe\" ? \"No symbol table — a typical PE EXE/DLL doesn't carry one (only object files and PDBs do).\" : \"No symbols found.\"","hexAddress(s.address)","s.name","fmtCount(BINARY_TABLE_ROW_CAP)","fmtCount(symbolsCap.total)","dotnetMeta.assembly.name","dotnetMeta.assembly.version","cultureLabel(dotnetMeta.assembly.culture)","hexOrDash(dotnetMeta.assembly.public_key)","rawAssemblyFlags(dotnetMeta.assembly.flags)","f","dotnetMeta.runtime_version","fmtCount(dotnetMeta.assembly_refs.length)","cultureLabel(r.culture)","hexOrDash(r.public_key_token)","r.name","r.version","fmtCount(assemblyRefsCap.total)","fmtCount(BINARY_TABLE_ROW_CAP)","fmtCount(dotnetMeta.types.length)","t.name","t.namespace || \"—\"","fmtCount(BINARY_TABLE_ROW_CAP)","fmtCount(typesCap.total)","fmtCount(dotnetMeta.methods.length)","m.name","fmtCount(BINARY_TABLE_ROW_CAP)","fmtCount(methodsCap.total)","hexAddress(ins.address)","ins.bytes","ins.text","fmtCount(disasm.length)"],
  "CertPreview.svelte": ["loadError","data.error","cert.subject","cert.issuer","cert.serial","cert.version","humanIso(cert.not_before)","humanIso(cert.not_after)","cert.signature_algorithm","keyLabel(cert.public_key)","cert.is_ca ? \"Yes\" : \"No\"","san","ku","eku","cert.sha256_fingerprint","copiedKey === \"sha256\" ? \"Copied\" : \"Copy\"","cert.sha1_fingerprint","copiedKey === \"sha1\" ? \"Copied\" : \"Copy\"","csr.subject","keyLabel(csr.public_key)","san","keyLabel(pubKey)","keyLabel(privKey)","data.encoding.toUpperCase()"],
  "CommandBar.svelte": ["$t('cmd.new')","selectionCount > 1 ? \"Rename one item at a time\" : \"Rename (F2)\"","$t('cmd.open')","$t('cmd.sort')","$t(s.labelKey)","$t('cmd.ascending')","$t('cmd.descending')","$t('cmd.view')","$t(v.labelKey)","$t('cmd.showHidden')","$t('cmd.groupFolders')","FILE_FILTERS.find((f) => f.key === fileFilter) ? $t('filter.' + fileFilter) : $t('cmd.filter')","$t('filter.' + f.key)","`${$t('palette.ariaPalette')} (Ctrl+Shift+P)`","`${c.name} (user command)`","c.name","showDetails ? \"Hide details pane (Alt+P)\" : \"Show details pane (Alt+P)\"","showTerminal ? \"Hide terminal\" : \"Show terminal\""],
  "CompareDialog.svelte": ["summary.added","summary.removed","summary.changed","summary.identical","error","textDiff.added","textDiff.removed","row.op === \"add\" ? \"+\" : row.op === \"del\" ? \"−\" : \" \"","row.text","(fileDiff.firstDiff ?? 0).toString(16).toUpperCase()","fileDiff.firstDiff","fileDiff.ranges.length","fileDiff.lengthDiffers ? \"differ\" : \"match\"","row.hasChildren ? (collapsed.has(row.path) ? \"▸\" : \"▾\") : \"\"","STATUS_LABEL[row.node.status]"],
  "ContextMenu.svelte": ["selectionCount > 1 ? \"Rename one item at a time\" : \"Rename (F2)\"","$t('ctx.open')","$t('ctx.execute')","$t('ctx.executeAdmin')","$t('ctx.openNewTab')","$t('ctx.openInTerminal')","$t('ctx.workOnThis')","$t('ctx.repairLink')","$t('ctx.folder')","$t('ctx.textFile')","$t(ft.labelKey)","$t('ctx.duplicate')","$t('ctx.copyAsPath')","$t('ctx.copyToFolder')","$t('ctx.moveToFolder')","$t('ctx.copyName')","$t('ctx.rename')","$t('ctx.batchMedia')","$t('ctx.compareFiles')","$t('ctx.selectAllExt', { ext: sameTypeExt })","$t('ctx.extract')","$t('ctx.extractTo')","$t('ctx.archiveSafety')","$t('ctx.compressZip')","$t('ctx.compressTarGz')","$t('ctx.compressWithPassword')","pinned ? $t('ctx.unpinFromHome') : $t('ctx.pinToHome')","favorited ? $t('ctx.removeFavorite') : $t('ctx.addFavorite')","$t('ctx.tags')","name","c.name","$t('ctx.reveal')","$t('ctx.properties')","$t('studio.menu')","$t('ctx.shred')","$t('ctx.open')","$t('ctx.folder')","$t('ctx.textFile')","$t(ft.labelKey)","$t('ctx.copyAsPath')","$t('ctx.openInTerminal')","$t('ctx.properties')","$t('ctx.ejectDrive')","$t('ctx.open')","$t('ctx.openNewTab')","$t('ctx.copyAsPath')","$t('ctx.properties')","$t('home.removeNetworkLocation')","$t('home.disconnectShare')","$t('ctx.open')","$t('ctx.openNewTab')","$t('ctx.reveal')","$t('ctx.copy')","$t('ctx.copyAsPath')","$t('ctx.rename')","$t('ctx.folder')","$t('ctx.textFile')","$t(ft.labelKey)","$t('ctx.properties')","$t('ctx.delete')","$t('ctx.addFavorite')","$t('home.pinToQuickAccess')","$t('home.removeFromFavorites')","$t('home.removeFromRecentFolders')","$t('home.removeFromRecent')","$t('home.clearAll')","$t('ctx.folder')","$t('ctx.textFile')","$t(ft.labelKey)","$t('ctx.newLink')","$t('ctx.paste')","$t('ctx.undo')","undoLabel ? ` ${undoLabel}` : ''","$t('view.details')","$t('view.list')","$t('view.icons')","$t('view.gallery')","$t('sort.name')","$t('sort.modified')","$t('sort.type')","$t('sort.size')","$t('cmd.ascending')","$t('cmd.descending')","$t('ctx.selectAll')","$t('ctx.invertSelection')","$t('ctx.selectByPattern')","$t('ctx.refresh')","$t('ctx.openInTerminal')","$t('ctx.workOnFolder')","$t('ctx.reveal')","$t('ctx.properties')","$t('palette.ariaPalette')"],
  "DataBrowser.svelte": ["isSqlite ? \"Table / view\" : \"Sheet\"","s","offset + 1","offset + page.rows.length","page.total","error","c.type || \"column\"","c.name","sortDir === 1 ? \"▲\" : \"▼\"","cell","loading ? \"Loading…\" : \"No rows.\"","loading ? \"Loading…\" : \"\""],
  "DocsView.svelte": ["expanded ? \"Collapse section\" : \"Expand section\"","g.name","g.docs.length","d.title","html"],
  "EmailPreview.svelte": ["loadError","data.error","data.from ?? \"—\"","data.to.join(\", \")","data.cc.join(\", \")","data.subject ?? \"—\"","dateText","data.attachments.length === 1 ? \"1 attachment\" : `${data.attachments.length} attachments`","`${displaySafeName(att.filename)} — ${att.content_type}`","formatSize(att.size)","data.body"],
  "FloatPreview.svelte": [],
  "FontPreview.svelte": ["$t(\"pv.cantFont\")","sampleText","format ?? formatLabelForExt(extension)","metadata.family","metadata.style","metadata.version","metadata.numGlyphs.toLocaleString()","formatSize(size)","codepointLabel(selectedGlyph)","glyphChar(selectedGlyph)","codepointLabel(cp)","`Glyph ${codepointLabel(cp)}`","glyphChar(cp)","glyphGrid.shown.length","glyphGrid.total.toLocaleString()","glyphGrid.total.toLocaleString()","glyphGrid.total === 1 ? \"character\" : \"characters\"","glyphGrid.total"],
  "HexView.svelte": ["sig.ext","sig.name","(pageOffset + bytes.length).toString(16).toUpperCase()","pageOffset.toString(16).toUpperCase()","size","error","row.offset","hex2(b)","row.ascii","cursor.toString(16).toUpperCase()","row.type","row.value"],
  "IcalPreview.svelte": ["loadError","data.calendar_name","data.method","data.error","ev.summary ?? \"(no title)\"","componentBadge(ev.component)","whenText(ev)","ev.location","ev.organizer","ev.status","ev.attendees.length === 1 ? \"1 attendee\" : `${ev.attendees.length} attendees`","att","ev.recurrence","ev.description"],
  "Icon.svelte": [],
  "JwtPreview.svelte": ["loadError","data.error","data.alg ?? \"—\"","data.typ ?? \"—\"","data.kid","human(data.iat.raw)","human(data.nbf.raw)","human(data.exp.raw)","data.signature_len === 1 ? \"byte\" : \"bytes\"","data.signature_len.toLocaleString()","data.alg === \"none\" ? \"alg: none\" : \"empty or malformed\"","payloadJson","headerJson"],
  "LinkBadge.svelte": ["title"],
  "LogPreview.svelte": ["loadError","LEVEL_LABEL[level]","log.counts[level]","unleveledCount","log.lines.length","log.lines.length === 1 ? \"\" : \"s\"","visibleLines.length","formatSize(win.file_len)","formatSize(win.window_end - win.window_start)","win.file_len.toLocaleString()","win.window_end.toLocaleString()","win.window_start.toLocaleString()","win.window_end.toLocaleString()","win.window_start.toLocaleString()","formatSize(win.file_len)","win.file_len.toLocaleString()","log.lines.length.toLocaleString()","log.totalLines.toLocaleString()","line.index + 1","line.level ? LEVEL_LABEL[line.level] : \"\"","line.text","line.truncated ? \"…\" : \"\""],
  "MacroRunConfirm.svelte": ["macro.name","macro.steps.length","macro.steps.length === 1 ? \"\" : \"s\"","inputs.length","inputs.length === 1 ? \"\" : \"s\"","planError","op.detail","op.input","op.kind","runError","running ? \"Running…\" : \"Run\"","macro.name","run.ops.length","run.ops.length === 1 ? \"\" : \"s\"","undoError","undoing ? \"Undoing…\" : \"Undo\""],
  "MacrosDialog.svelte": ["m.name","m.steps","m.steps === 1 ? \"\" : \"s\"","STEP_LABEL[kindOf(step)]","STEP_LABEL[k]","error","note","macros.length","macros.length === 1 ? \"\" : \"s\""],
  "MediaPlayer.svelte": ["state.playing ? \"Pause\" : \"Play\"","state.playing ? \"Pause\" : \"Play\"","mt.formatTime(state.currentTime)","mt.formatTime(state.duration)","state.muted ? \"Unmute\" : \"Mute\"","state.muted ? \"Unmute\" : \"Mute\"","state.rate"],
  "MediaQuickLook.svelte": ["count","position + 1","repeatLabel","repeatLabel","shuffled ? \"Shuffle on\" : \"Shuffle off\"","shuffled ? \"on\" : \"off\""],
  "MenuBar.svelte": ["$t(menu.labelKey)","$t(menu.labelKey)","item.label ?? (item.labelKey ? $t(item.labelKey) : \"\")","item.hint","$t(\"menu.language\")","$t(\"menu.language\")","$t(\"menu.language\")","$locale === l.code ? \"✓\" : \"\"","l.english","l.name","cov === 0 ? \"Not yet translated — shows in English\" : `${Math.round(cov * 100)}% translated — the rest shows in English`","cov === 0 ? \"English\" : `${Math.round(cov * 100)}%`"],
  "MetadataStudioDialog.svelte": ["$t(\"studio.title\")","$t(\"studio.title\")","$t(\"common.close\")","$t(\"studio.noFile\")","$t(\"studio.viewOnly\")","$t(\"studio.loading\")","error","$t(\"studio.noMeta\")","groupLabel(g)","f.key","$t(\"studio.revertFieldHint\")","$t(\"studio.revertFieldAria\", { field: f.key })","writable ? $t(\"studio.fieldReadonly\") : $t(\"studio.viewOnly\")","currentValue(f, edited) || \"—\"","$t(\"studio.applyAll\", { n: files.length })","$t(\"studio.stripEditableHint\")","$t(\"studio.stripEditable\")","$t(\"studio.copyFromFirstHint\")","$t(\"studio.copyFromFirst\")","$t(\"studio.resetAllHint\")","$t(\"studio.resetAll\")","notice","$t(\"common.close\")","saving ? $t(\"studio.saving\") : $t(\"studio.save\")"],
  "NetworkConnectionForm.svelte": ["editing ? `Edit connection ${editing.name}` : \"Add a connection\"","editing ? `Edit “${editing.name}”` : \"Add a connection\"","s","hints.hostLabel","hints.userLabel","hints.pathLabel","AUTH_LABELS[kind]","error","editing ? \"Save\" : \"Add\""],
  "NetworkConnectionMenu.svelte": ["`${name} actions`"],
  "NetworkSecretPrompt.svelte": ["`${label} for ${name}`","label","name","label"],
  "NotebookPreview.svelte": ["loadError","parseError","rawFallback","RAW_FALLBACK_CHARS.toLocaleString()","notebook.cells.length","notebook.totalCells","cell.type","cell.executionCount != null ? `In [${cell.executionCount}]` : \"In [ ]\"","cellHtml[cell.index] ?? \"\"","cellHtml[cell.index] ?? \"\"","cell.source","output.text","output.ename","output.evalue","output.traceback","output.text","output.otherMimeTypes.join(\", \")","output.otherMimeTypes.length","cell.outputs.length","cell.outputsTotal"],
  "OrganizeDialog.svelte": ["$t(\"org.title\")","$t(\"org.title\")","$t(r.labelKey)","error","$t(\"org.result\", { moved: movedCount, skipped: skippedCount })","$t(\"org.checkpointNote\", { label: outcome.checkpoint.checkpoint.label || outcome.checkpoint.checkpoint.manifest_id.slice(0, 12) })","$t(\"org.undo\")","$t(\"org.loading\")","$t(\"org.empty\")","$t(\"org.willMove\", { count: plan.length, groups: groups.length })","g.items.length","g.subdir","$t(\"common.cancel\")","applying ? $t(\"org.applying\") : $t(\"org.apply\")"],
  "ScheduledSnapshots.svelte": ["error","rule.enabled ? \"on\" : \"paused\"","key"],
  "SidecarManager.svelte": ["$t(\"mgr.checking\")","$t(\"mgr.none\")","row.running ? $t(\"mgr.running\") : $t(\"mgr.stopped\")","row.name","row.version","$t(\"mgr.\" + health.key)","$t(\"mgr.contractTip\")","row.compatible ? $t(\"mgr.contractOk\", { v: row.contract }) : $t(\"mgr.contractBad\", { v: row.contract })","row.enabled ? $t(\"mgr.enabled\") : $t(\"mgr.disabled\")","row.enabled ? $t(\"mgr.enabled\") : $t(\"mgr.disabled\")","CAPABILITY_INFO[cap].label","$t(\"mgr.revoke\")","$t(\"mgr.grant\")","$t(\"mgr.grantTip\")","$t(\"mgr.noCapabilities\")","$t(\"mgr.lastError\")","diag.last_error","$t(\"mgr.healthy\")","$t(\"mgr.notRunning\")","$t(\"mgr.repair\")","logsOpen[row.id] ? $t(\"mgr.hideLogs\") : $t(\"mgr.viewLogs\", { count: diag.logs.length })","$t(\"mgr.noLogs\")","$t(\"mgr.repairDid\")","repairMsg[row.id]","line.level","line.message","$t(\"mgr.open\")","$t(\"mgr.stop\")"],
  "SmartFolderMenu.svelte": ["name","$t(\"ctx.rename\")","$t(\"smart.moveUp\")","$t(\"smart.moveUp\")","$t(\"smart.moveDown\")","$t(\"smart.moveDown\")","$t(\"common.apply\")","$t(\"menu.delete\")","$t(\"common.cancel\")"],
  "SyncDialog.svelte": ["status?.branch ? `“${status.branch}”` : \"repository\"","status.upstream","status.behind","status.ahead","status.blocked","syncActionLabel(action)","w","m < 60 ? `${m} min` : `${m / 60} h`","line","running ? \"Syncing…\" : \"Run sync\""],
  "TagEditor.svelte": ["$t(\"tags.title\")","$t(\"tags.title\")","$t(\"status.items\", { count })","tag","$t(\"tags.remove\")","$t(\"tags.none\")","$t(\"tags.addLabel\")","$t(\"tags.colorLabel\")","$t(`tags.color.${key === \"\" ? \"none\" : key}`)","$t(`tags.color.${key === \"\" ? \"none\" : key}`)","nativeName","$t(\"tags.pullNative\")","$t(\"tags.pushNative\")","syncNote","$t(\"tags.cancel\")","$t(\"tags.apply\")"],
  "TemplatesDialog.svelte": ["path ? `Capture ${displaySafeName(base(path))}` : \"No folder\"","t.name","t.dirs","t.files","path ? `Stamp into ${displaySafeName(base(path))}` : \"No folder\"","error","note","templates.length","templates.length === 1 ? \"\" : \"s\""],
  "ThumbnailImage.svelte": [],
  // CPE-1775 removed the `53:t.report.errors.join("\n")` entry: those reason lines start with an
  // ARCHIVE-CONTROLLED entry name and were the one genuine spoof surface in this file. They now render
  // through `displaySafePath` in a click-to-open list instead of a raw hover tooltip. What is left is
  // counts and literals (`whyLabel` builds "· N skipped — why?" from a number).
  "TransferPanel.svelte": ["label(t)","percent(t)","t.done_items","t.total_items","transferReasonsLabel(t.report)"],
  "UserCommandsDialog.svelte": ["c.name","c.template","c.mode","s","s"],
  "VaultBadge.svelte": ["title"],
  "VaultBanner.svelte": ["locking ? $t(\"vault.lockingTitle\") : $t(\"vault.lockTitle\")","locking ? $t(\"vault.locking\") : $t(\"vault.lock\")"],
  "VcardPreview.svelte": ["loadError","data.cards.length","data.error","heading(card)","subheading(card)","formatSize(card.photo_size)","tel.number","t","em.address","t","adr.label","t","url","card.birthday"],
  "WatchRulesDialog.svelte": ["rule.name","condSummary(rule.when)","rule.actions.map(actSummary).join(\", \")","actSummary(a)","preview.actions.map((a) => a.resolved).join(\", \")","preview.rule.name","f","fire.summary"],
  "WorkspacesDialog.svelte": ["w.name","w.tabs.length","w.tabs.length === 1 ? '' : 's'"],
  "YamlTomlPreview.svelte": ["loadError","parseErrorMessage","format === \"yaml\" ? \"YAML\" : \"TOML\"","parseErrorMessage","rawFallback","RAW_FALLBACK_CHARS.toLocaleString()"],

  // --- B1 (reviewer, round 2): 6 more candidates surfaced by the widened CANDIDATE_PATTERN ---
  "CardDetailDialog.svelte": ["id","title","error","k","v","bodyHtml","sending ? \"…\" : \"Send ▸\"","detail?.location || \"Tickets\"","bodyLines","bodyLines === 1 ? \"\" : \"s\"","metaFields.length","metaFields.length === 1 ? \"\" : \"s\""],
  "CreateCertDialog.svelte": ["v","`Remove ${v}`","v","`Remove ${v}`","kt.label","error","busy ? \"Creating…\" : \"Create\""],
  "NewLinkDialog.svelte": ["$t(\"link.newLinkTitle\")","$t(\"link.newLinkTitle\")","$t(\"link.kindLabel\")","$t(\"link.kindSymlink\")","$t(\"link.kindHardlink\")","$t(\"link.kindJunction\")","$t(\"link.targetLabel\")","$t(\"link.browse\")","$t(\"link.junctionTargetHint\")","$t(\"link.nameLabel\")","error","$t(\"common.cancel\")","$t(\"link.create\")"],
  "RepairLinkDialog.svelte": ["$t(\"link.repairTitle\")","$t(\"link.repairTitle\")","$t(\"link.repairIntro\")","$t(\"link.repairLoading\")","$t(\"link.repairSuggestionLabel\")","$t(\"link.repairNoSuggestion\")","translate($locale, \"link.repairConfirm\", { target: displaySafePath(chosenTarget ?? \"\") })","$t(\"common.cancel\")","$t(\"link.repairConfirmYes\")","error","$t(\"common.close\")","$t(\"link.repairBrowse\")","$t(\"link.repairAccept\")"],
  "Spotlight.svelte": ["$t(\"spotlight.title\")","$t(\"spotlight.ariaSearch\")","query.trim() ? $t(\"spotlight.noMatches\") : $t(\"spotlight.typeHint\")","$t(GROUP_LABEL[section.kind])"],
  "WorkbenchView.svelte": ["branch || \"detached\"","stats.added","stats.files","stats.files === 1 ? \"\" : \"s\"","stats.removed","error","branch || \"the working tree\"","isCollapsed ? \"Expand\" : \"Collapse\"","isCollapsed ? \"▸\" : \"▾\"","fs.added","fs.removed","copiedFile === key ? \"✓ Copied\" : \"Copy\"","h.header","l.kind === \"add\" ? \"+\" : l.kind === \"del\" ? \"−\" : \" \"","l.newLine ?? \"\"","l.oldLine ?? \"\"","l.text","s.text"],

  // --- CPE-1790: the confirm/password-prompt dialogs, previously invisible to isCandidateComponent
  // because their own props (`title`/`message`/`error`/`confirmLabel`) don't match any name/path SHAPE
  // — see the ticket and bidiRenderScan.ts's CANDIDATE_PATTERN doc for why generic-prop leaves needed
  // their own membership trigger (a call to displaySafeName/displaySafePath), not just a wider
  // vocabulary list. Both dialogs now escape EVERY free-text prop (`title`/`message`/`error`/
  // `confirmLabel`) on arrival — CPE-1760's "leaf escapes what it renders" model — so every App.svelte
  // call site is covered whether or not it remembers to wrap its own name first. `confirmLabel` is
  // included even though every caller today passes a static verb ("OK"/"Delete"/"Extract"/"Compress"/
  // "Unlock"/"Delete permanently"/"Close all"): that was true only BY CONVENTION — an ordinary
  // caller-supplied prop, not static BY CONSTRUCTION the way a `$t(...)` call is — and this ticket
  // exists specifically to stop a free-text render slot being protected by convention instead of by the
  // leaf (review round 2, PR #949). Both files are fully provably safe: `[]`.
  "ConfirmDialog.svelte": [],
  "PasswordPromptDialog.svelte": [],

  // --- CPE-1790 (review round 2/3): MacroParamPrompt.svelte shares ConfirmDialog/PasswordPromptDialog's
  // exact `title`/`message` shape, and its one caller (App.svelte's run-macro flow, `title="Macro
  // parameters — {macroParamPromptFor.macro.name}"`) is NOT static — a macro can be imported from a
  // pasted definition (MacrosDialog.svelte's import flow), so `macro.name` is externally-supplied text,
  // the same accepted-but-disclosed raw render MacroRunConfirm.svelte's REGISTRY entry already carries
  // for the run confirmation itself (`"macro.name"`, below). `title`/`message` are escaped on
  // arrival, the same leaf-escapes model as the other two dialogs, which is also what makes this file a
  // `CANDIDATE_PATTERN` match through the CPE-1790 `displaySafeName(`-call trigger rather than through
  // an incidental `.name` mention. `label` (the per-parameter field caption, taken from the macro's own
  // `{ask:label}` token — also externally-supplied, same as `macro.name`) is escaped too, in its one
  // render position (the `<label>` text node) — review round 3 rejected an earlier draft that left it
  // raw on the reasoning that it doubled as the `for=`/`id=` pairing value: that reasoning didn't hold,
  // since `for=`/`id=`/`data-testid=` and the `values[label]` object key all still reference the RAW
  // `label` string unchanged (none of them are DOM render positions this engine scans), so wrapping only
  // the text node touches that pairing in no way at all. Fully provably safe: `[]`.
  "MacroParamPrompt.svelte": [],

  // --- CPE-1798: StatusBar's `notice` prop is fed 35 live backend-error strings (App.svelte's
  // showNotice(String(e), true) call sites, plus one hand-built "Sync failed: " + e.message) that
  // routinely embed the offending filesystem path — the same "generic prop, no name/path SHAPE, but not
  // actually static" gap CPE-1790 closed for ConfirmDialog/PasswordPromptDialog/MacroParamPrompt.
  // `notice` is now escaped on arrival at both render positions (text + title), which is also what makes
  // this file a CANDIDATE_PATTERN match through the `displaySafeName(`-call trigger. Every OTHER offender
  // below (item/selection counts, git branch/ahead/behind, the disk-free label) is unrelated pre-existing
  // UI text this engine can't prove safe (numbers, i18n-free plain labels) — none of it is a filesystem
  // name/path, and `notice`/`filteredHiddenText`/`filteredHiddenTitle` (already escaped at the source
  // that builds them, see CPE-1708) do not appear here, proving the fix.
  // CPE-1833: line numbers shifted (the accessible-announcement fix added script/markup above these),
  // and `advisoryAnnouncement` (168) is a NEW entry — it is built purely from `filteredHiddenText`/
  // `unreadableText` (both already in this list, both counts + fixed phrases, never a filesystem
  // name/path), so it is exactly as safe as the two it concatenates.
  "StatusBar.svelte": ["itemCount","totalCount","itemCount","itemCount === 1 ? \"\" : \"s\"","selectedCount","selectedSize > 0 ? ` — ${formatSize(selectedSize)}` : \"\"","filteredHiddenTitle","filteredHiddenText","unreadableTitle","unreadableText","advisoryAnnouncement","git.upstream ? `Tracking ${git.upstream}` : \"No upstream branch\"","git.branch || \"detached\"","git.behind","git.ahead","diskLabel"],

  // --- CPE-1798 sibling audit: AgentMenu's `sessionLabel` prop is built by its one real caller
  // (Sidebar.svelte:436, `${s.agentName || s.agentId || "Agent"}${model ? " · " + model : ""}`) from an
  // agent's own self-reported identity string, not static UI copy — the same shape-cleared-but-not-
  // runtime-cleared gap the ticket found for StatusBar. Escaped on arrival at both render positions
  // ("Open "/"Close " rows); `sessionNum(sessionId)` (a numeric chip label, unrelated to this ticket) and
  // `label` (verified static — every real caller passes either the literal default or `$t(...)`, see
  // Toolbar.svelte's equivalent, unlike `sessionLabel`) are left as accepted-but-unprovable entries below.
  "AgentMenu.svelte": ["sessionNum(sessionId)","sessionNum(sessionId)","label"],
};

/** The subset of REGISTRY whose non-empty array is an ACTUAL disclosed "still renders a raw filesystem
 *  name/path" gap (matching the ticket's own priority-ordered residual list), as opposed to harmless
 *  UI text this engine merely can't prove safe. `src/docs/03-explorer.md`'s "Not yet covered" paragraph
 *  must name exactly this set — see the `doc-parity` test below, checked in BOTH directions. */
const DISCLOSED_GAPS = [
  "ContentIndexSearchDialog", "FileHealthDialog", "NearDuplicatesDialog", "SimilarImagesDialog",
  "DeclutterDialog", "BatchMediaDialog", "SplitFileDialog", "JoinPartsDialog", "ExplorerPane",
  "TerminalPanel", "Sidebar",
];

/** App.svelte's markup-level offenders, same exact-equality treatment as REGISTRY. App.svelte is far
 *  too large to add wholesale to COVERED_FILES' original per-file review, so it's split from REGISTRY. */
const APP_MARKUP_OFFENDERS = ["6518:$t(\"palette.openAgentBoardWindow\")","6530:$t(\"sidebar.repositories\")","6533:$t(\"sidebar.repositories\")","6539:$agentSessions.length === 0 ? $t(\"tb.openConsole\") : $agentSessions.length === 1 ? $t(\"tb.openConsoleOne\") : $t(\"tb.openConsoleMany\", { count: $agentSessions.length })","6547:$t(\"tb.aiConsole\")","6549:$agentSessions.length","6549:$t(\"tb.agentsRunning\", { count: $agentSessions.length })","6556:$t(\"tb.showDetailsPane\")","6561:$t(\"cmd.showHidden\")","6566:$t(\"cmd.folderSizes\")","6571:$t(\"tb.resetSettings\")","6658:$t(\"tb.paneWidth\")","6737:$t(\"tb.resizeNav\")","6738:$t(\"tb.resizeTip\")","6901:$t(\"tb.resizeDetails\")","6902:$t(\"tb.resizeTip\")","6911:$t(\"tb.popoutTip\")","6912:$t(\"tb.popoutAria\")","6917:$t(\"tb.defaultTab\")","6925:$t(\"tb.preview\")","6926:$t(\"view.details\")","6930:$t(\"tb.paneWidth\")","6946:$t(\"tb.previewOrDetails\")","6947:$t(\"tb.dragPopoutTip\")","6958:$t(\"tb.preview\")","6964:$t(\"view.details\")","7200:confirm.title","7215:passwordPrompt.title","7553:runConfirm.title","7571:macroParamPromptFor.macro.name","7837:$t(\"dnd.dropToImport\")"];

/** App.svelte's two already-disclosed SplitFileDialog/JoinPartsDialog completion notices
 *  (`showNotice($t(..., { name: baseName(path) }))`) are built in `<script>` code, not markup — the one
 *  shape `findUnsafeRenderLines` (markup-only, see its module doc) genuinely cannot see, since the
 *  eventual DOM render happens through a separate `{notice}`-style span elsewhere, not at this call
 *  site. Checked here with a narrow, targeted scan instead: every `baseName(`/`basename(` call anywhere
 *  in App.svelte's source not immediately wrapped in `displaySafeName(`/`displaySafePath(` must be one
 *  of these two allowlisted lines. */
const APP_SCRIPT_BASENAME_ALLOWLIST = [2855, 2870];

function findRawBaseNameCallsInScript(src: string): number[] {
  const lines = new Set<number>();
  const re = /\bbase[Nn]ame\(/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src)) !== null) {
    const before = src.slice(Math.max(0, m.index - 40), m.index);
    if (!/displaySafe(?:Name|Path)\(\s*$/.test(before)) {
      lines.add(src.slice(0, m.index).split("\n").length);
    }
  }
  return [...lines].sort((a, b) => a - b);
}

describe("bidi/format-character escape guard (CPE-1757 round 2)", () => {
  it("every registered component's raw-render set matches its recorded expressions EXACTLY", () => {
    const mismatches: string[] = [];
    for (const [file, recorded] of Object.entries(REGISTRY)) {
      const src = readFileSync(join(COMPONENTS, file), "utf8");
      const found = findUnsafeRenderLines(src, file);
      // CPE-1885: compare by expression MULTISET, not by "line:expr" string — a pure reformat that
      // shifts every line but changes no expression must stay green (see the header note above).
      const foundExprs = exprMultiset(found);
      const recordedSorted = [...recorded].sort((a, b) => a.localeCompare(b));
      if (JSON.stringify(foundExprs) !== JSON.stringify(recordedSorted)) {
        const newlyRaw = multisetDiff(foundExprs, recordedSorted);
        const stale = multisetDiff(recordedSorted, foundExprs);
        // F5 (reviewer, CPE-1761 attempt 2): the useful delta goes FIRST — a developer reading a failed
        // registry file must see what actually changed before wading through the full recorded/found
        // dumps (which, for a file like AgentTimeline.svelte, run to several KB and bury the diff).
        // CPE-1885: name the component + bare expression first, addresses last — most of a wall like
        // this used to be noise around a one-word fact. `found` (with its live line numbers) is still
        // included at the end so a real investigation can jump straight to the offending line.
        mismatches.push(
          `${file}:` +
            (newlyRaw.length ? ` NEW raw offender expression(s): ${newlyRaw.join(",")}` : "") +
            (stale.length ? ` STALE recorded expression(s), no longer rendered raw: ${stale.join(",")}` : "") +
            ` — full found (line:expr) [${found.join(",")}] vs recorded expressions [${recordedSorted.join(",")}]`,
        );
      }
    }
    expect(
      mismatches,
      `wrap a genuinely new offender in displaySafeName/displaySafePath, or update REGISTRY here (and, ` +
        `if it's a real disclosed gap, src/docs/03-explorer.md's "Not yet covered" list) to match reality: ` +
        `${mismatches.join(" | ")}`,
    ).toEqual([]);
  });

  it("App.svelte's markup-level raw-render set matches its recorded lines exactly", () => {
    const src = readFileSync(APP, "utf8");
    const found = findUnsafeRenderLines(src, "App.svelte");
    const recorded = [...APP_MARKUP_OFFENDERS].sort(compareOffenders);
    expect(found, `App.svelte markup offenders drifted from REGISTRY: found [${found.join(",")}]`).toEqual(recorded);
  });

  it("App.svelte's script-side baseName(…)/basename(…) calls are all wrapped, except the two allowlisted notices", () => {
    const src = readFileSync(APP, "utf8");
    const found = findRawBaseNameCallsInScript(src);
    expect(found, `raw baseName()/basename() call(s) in App.svelte outside the allowlist: ${found.join(",")}`).toEqual(
      [...APP_SCRIPT_BASENAME_ALLOWLIST].sort((a, b) => a - b),
    );
  });

  it("the demonstration: a brand-new raw render in a new component reds this guard (see PR description for the live run + revert)", () => {
    // Reproduced literally, not asserted: findUnsafeRenderLines is the exact function REGISTRY is
    // checked against, so proving it flags a fresh violation IS proving the guard would have failed.
    const demoSrc = `<script>\n  export let entry;\n</script>\n\n<span title={entry.path}>{entry.name}</span>\n`;
    expect(findUnsafeRenderLines(demoSrc)).toEqual(["5:entry.name", "5:entry.path"]);
  });

  // CPE-1761 #2: recording only the LINE NUMBER (not the expression) meant editing an already-recorded
  // line in place — swapping its harmless expression for a genuinely raw filesystem name — left the
  // computed set unchanged (same line number, still present) and the guard green. Demonstrated on the
  // exact case the ticket measured: PreviewPane.svelte:1015 (`title={$t(action.labelKey)}`, harmless
  // i18n) edited in place to `title={entry.name}` (a real, unescaped filesystem name in a tooltip).
  it("CPE-1761 #2: substituting a raw filesystem name into an already-recorded line reds the guard (PreviewPane.svelte:1015)", () => {
    const src = readFileSync(join(COMPONENTS, "PreviewPane.svelte"), "utf8");
    const lines = src.split("\n");
    const ORIGINAL_LINE = 'title={$t(action.labelKey)}';
    const SPOOFED_LINE = "title={entry.name}";
    // Guard the fixture itself: if PreviewPane.svelte changes shape, this demonstration must fail loudly
    // rather than silently prove nothing (the exact failure mode this whole ticket is about).
    expect(lines[1014], "PreviewPane.svelte:1015 no longer contains the exact text this demonstration substitutes — update the fixture").toContain(ORIGINAL_LINE);
    expect(REGISTRY["PreviewPane.svelte"], "PreviewPane.svelte must currently record $t(action.labelKey) as an offender expression for this demonstration to mean anything").toContain("$t(action.labelKey)");

    lines[1014] = lines[1014].replace(ORIGINAL_LINE, SPOOFED_LINE);
    const mutated = lines.join("\n");
    const found = findUnsafeRenderLines(mutated, "PreviewPane.svelte (mutated: raw name substituted on an already-recorded line)");

    // Prove this is genuinely the LINE-NUMBER-ONLY blind spot, not some unrelated difference: line 1015
    // is still flagged either way (a bare line-number check would see "1015 present in both" and call it
    // green — the exact bug CPE-1757 round 1 shipped).
    expect(found.some((e) => e.startsWith("1015:")), "line 1015 should still be an offender after the substitution").toBe(true);
    // The REASON it must red: the recorded EXPRESSION at line 1015 no longer matches reality.
    expect(found).toContain("1015:entry.name");
    expect(found).not.toContain("1015:$t(action.labelKey)");
    // CPE-1885: the actual guard comparison is now by expression MULTISET, not "line:expr" string, so
    // prove it fails on those terms too — one `$t(action.labelKey)` occurrence traded for a new
    // `entry.name` occurrence changes the multiset even though the total offender COUNT is unchanged
    // (this is exactly the case a naive Set-based dedup would miss: same size, different bag).
    const foundExprs = exprMultiset(found);
    const recordedExprs = [...REGISTRY["PreviewPane.svelte"]].sort((a, b) => a.localeCompare(b));
    expect(JSON.stringify(foundExprs)).not.toBe(JSON.stringify(recordedExprs));
  });

  it('the doc names exactly the disclosed gaps — bidirectional (a name missing OR a name wrongly present both fail)', () => {
    const doc = readFileSync(DOC, "utf8");
    const section = /\*\*Not yet covered\*\*[\s\S]*?(?=\n- \*\*|\n## |$)/.exec(doc)?.[0];
    expect(section, `src/docs/03-explorer.md must have a "Not yet covered" bullet`).toBeTruthy();
    const paragraph = section!;

    const missing = DISCLOSED_GAPS.filter((name) => !paragraph.includes(`\`${name}\``));
    expect(missing, `these disclosed gaps must be named in the doc's "Not yet covered" paragraph: ${missing.join(", ")}`).toEqual([]);

    // The reverse direction (review round 2's B3b): a fully-covered file's name must NOT be listed as a
    // gap — e.g. deleting `Sidebar`'s mention wouldn't be caught by a plain doc.includes("Sidebar") check
    // (it matches an unrelated "sidebar folder tree" bullet elsewhere), which is exactly why this test
    // scopes to the extracted paragraph specifically, in both directions.
    const registeredNames = Object.keys(REGISTRY).map((f) => f.replace(/\.svelte$/, ""));
    const wronglyPresent = registeredNames.filter((name) => !DISCLOSED_GAPS.includes(name) && paragraph.includes(`\`${name}\``));
    expect(wronglyPresent, `these are NOT disclosed gaps but appear in the doc's "Not yet covered" paragraph: ${wronglyPresent.join(", ")}`).toEqual([]);
  });

  // CPE-1761: the doc drifted twice — calling this file's exported constant `ALLOWLIST` (renamed to
  // REGISTRY in round 2) and calling the guard "grep-based" (it has been a parser since round 2). Neither
  // phrase was covered by the doc-parity test above (which only checks the DISCLOSED_GAPS names), which
  // is exactly why they went stale silently. Covering the literal wording here closes that gap.
  //
  // Reviewer (CPE-1761 attempt 2, F7): scoped to the bidi-escape bullet specifically (same extraction
  // technique the "Not yet covered" test above uses, just anchored at the bullet's own start rather than
  // its "Not yet covered" sub-heading), NOT the whole doc — an unrelated future feature legitimately using
  // the word "allowlist" or "grep-based" elsewhere in 03-explorer.md must not red this test. Anchored to
  // the bullet's start (not the "Not yet covered" sub-string) because "grep-based" lives EARLIER in the
  // same bullet, before "Not yet covered" begins — scoping to only the later sub-paragraph would silently
  // stop covering that phrase at all.
  it("the doc does not use the stale round-1 vocabulary (ALLOWLIST / grep-based) for this guard", () => {
    const doc = readFileSync(DOC, "utf8");
    const bidiBullet = /- \*\*A name that tries to disguise its own extension is flagged, not hidden\.\*\*[\s\S]*?(?=\n- \*\*|\n## |$)/.exec(doc)?.[0];
    expect(bidiBullet, `src/docs/03-explorer.md must have the bidi-escape bullet ("A name that tries to disguise...")`).toBeTruthy();
    const paragraph = bidiBullet!;

    expect(paragraph, `src/docs/03-explorer.md's bidi-escape bullet still says "ALLOWLIST" — that exported constant was renamed to REGISTRY in CPE-1757 round 2; update the prose to match the real name`).not.toContain("ALLOWLIST");
    expect(paragraph, `src/docs/03-explorer.md's bidi-escape bullet still calls the guard "grep-based" — it has been a parser (bidiRenderScan.ts) since CPE-1757 round 2, not a regex/grep scan; update the prose`).not.toMatch(/grep-based/i);
    // Positive check, not just the negative ones above: the doc must actually name the real constant.
    expect(paragraph, `src/docs/03-explorer.md's bidi-escape bullet should point readers at the guard test's real REGISTRY constant`).toContain("REGISTRY");
  });

  // CPE-1768: REGISTRY used to cover 41 of 136 .svelte files with no stated rule for which files MUST be
  // in it — a new component rendering a raw filesystem name could go unregistered forever with nothing to
  // notice. This is the mechanical enforcement: walk every REAL .svelte file under src/lib/components, and
  // require every one `isCandidateComponent` flags (see its doc in bidiRenderScan.ts for the exact
  // criterion) to be a REGISTRY key — registration becomes the thing you cannot forget, not the thing you
  // must remember. REGISTRY now carries 92 keys (up from 41), matching the criterion's live output, not a
  // number frozen in prose that can go stale the moment a new component ships.
  it("CPE-1768: every candidate component (a name/path-shaped reference) is registered in REGISTRY", () => {
    const files = readdirSync(COMPONENTS).filter((f) => f.endsWith(".svelte"));
    const missing = files.filter((f) => {
      const src = readFileSync(join(COMPONENTS, f), "utf8");
      return isCandidateComponent(src) && !(f in REGISTRY);
    });
    expect(
      missing,
      `these components reference a filesystem name/path shape (see isCandidateComponent's criterion in ` +
        `bidiRenderScan.ts) but have no REGISTRY entry here — add one (findUnsafeRenderLines's live output, ` +
        `even [] once it's clean) rather than leaving them unscanned: ${missing.join(", ")}`,
    ).toEqual([]);
  });

  // B1 (reviewer, round 2): the FIRST CANDIDATE_PATTERN (`.name`/`.path`/`.fullPath`/`.oldName`/`.cwd`
  // property access, plus `export let name`/`export let path`) was itself a five-property regex zoo, and
  // missed three components already shipping a raw filesystem render: WorkbenchView.svelte's
  // `export let root` (rendered raw in body text), CreateCertDialog.svelte's `let folder = outDir`
  // (rendered raw in a title= tooltip), and RepairLinkDialog.svelte's `export let linkPath`/`chosenTarget`
  // (a symlink target, the component's whole subject). All three are fixed (displaySafeName/Path) and
  // registered above, but a REGRESSION that narrows CANDIDATE_PATTERN back down must be caught even if it
  // doesn't happen to touch one of REGISTRY's current keys — pin these three by name, directly against the
  // detector, not just indirectly via "is it in REGISTRY" (which a narrower pattern could vacuously pass
  // if the file simply stayed registered from a previous, wider pass).
  it("B1: the three components the review found missing under the narrower CANDIDATE_PATTERN are still detected as candidates", () => {
    for (const f of ["WorkbenchView.svelte", "CreateCertDialog.svelte", "RepairLinkDialog.svelte"]) {
      const src = readFileSync(join(COMPONENTS, f), "utf8");
      expect(isCandidateComponent(src), `${f} must be detected as a candidate — this is the exact shape CPE-1768's review found missing`).toBe(true);
    }
  });

  // CPE-1790: `ConfirmDialog`/`PasswordPromptDialog` take generic `title`/`message`(/`error`) props —
  // none of them match ANY name/path SHAPE in CANDIDATE_PATTERN, so `isCandidateComponent` used to return
  // `false` for both and `findUnsafeRenderLines` was never run on either file at all, despite
  // `src/docs/03-explorer.md` claiming delete/extract/unlock/run-command confirmations were covered.
  // Reproduced literally (not asserted in prose) against the exact shape both files had BEFORE this
  // ticket: a raw `{message}` body-text render and no call to either escape helper anywhere in the file.
  it("CPE-1790 red proof: the pre-fix generic-prop dialog shape is invisible to isCandidateComponent (the bug this ticket closes)", () => {
    const preFixConfirmDialogSrc = `<script>\n  export let title = "Are you sure?";\n  export let message = "";\n  export let confirmLabel = "OK";\n</script>\n\n<h2>{title}</h2>\n<p>{message}</p>\n<button>{confirmLabel}</button>\n`;
    // The exact spoof this shape lets through: a caller composing \`message\` straight from
    // \`item.name\` with no escape (the mistake CPE-1790's own ticket says "nothing currently stops").
    expect(findUnsafeRenderLines(preFixConfirmDialogSrc), "the render IS unsafe by the engine's own rules").toEqual(
      ["7:title", "8:message", "9:confirmLabel"].sort(compareOffenders),
    );
    expect(
      isCandidateComponent(preFixConfirmDialogSrc),
      "this is the bug: a real, unescaped filesystem-name render sits in this file, but nothing marks it as a REGISTRY candidate, so the membership test (CPE-1768, above) never asks for it to be registered and findUnsafeRenderLines is never run against it by the guard suite at all",
    ).toBe(false);
  });

  // The fix: once the leaf calls displaySafeName/displaySafePath on arrival (CPE-1760's model), the same
  // shape becomes a candidate through the NEW CANDIDATE_PATTERN bullet (a call to the escape helper
  // itself, not a name/path-shaped identifier) — proving the membership rule, not just REGISTRY's
  // hand-added keys above, now catches this exact class of component. MacroParamPrompt.svelte (review
  // round 2, PR #949) is the same shape: its `title` render was never static-by-caller the way an
  // earlier draft of bidiRenderScan.ts's own doc comment claimed (App.svelte composes it around
  // `macro.name`, and a macro can be imported from a pasted definition), so it needed the identical fix.
  it("CPE-1790: ConfirmDialog/PasswordPromptDialog/MacroParamPrompt are detected as candidates now that they escape on arrival", () => {
    for (const f of ["ConfirmDialog.svelte", "PasswordPromptDialog.svelte", "MacroParamPrompt.svelte"]) {
      const src = readFileSync(join(COMPONENTS, f), "utf8");
      expect(isCandidateComponent(src), `${f} must be detected as a candidate — it now calls displaySafeName on arrival`).toBe(true);
    }
  });

  // CPE-1790's own "a call site that forgets the wrap must fail CI" AC, relocated to where the duty now
  // lives: the LEAF's render, not the caller's message-composition. Mutate ConfirmDialog.svelte's real,
  // current source to drop the displaySafeName wrap from its message render (reverting to the exact
  // pre-fix shape) and prove the guard reds — same substitution-demonstration technique as the
  // PreviewPane.svelte:1015 test above.
  it("CPE-1790 regression proof: reverting ConfirmDialog's message render to raw {message} reds the guard", () => {
    const src = readFileSync(join(COMPONENTS, "ConfirmDialog.svelte"), "utf8");
    const ORIGINAL = "<p>{displaySafeName(message)}</p>";
    const SPOOFED = "<p>{message}</p>";
    expect(src, "ConfirmDialog.svelte no longer contains the exact text this demonstration substitutes — update the fixture").toContain(ORIGINAL);

    const mutated = src.replace(ORIGINAL, SPOOFED);
    const found = findUnsafeRenderLines(mutated, "ConfirmDialog.svelte (mutated: displaySafeName wrap dropped)");
    const foundExprs = exprMultiset(found);
    const recordedExprs = [...REGISTRY["ConfirmDialog.svelte"]].sort((a, b) => a.localeCompare(b));

    expect(found, "the un-escaped message render must be flagged").toContain("38:message");
    expect(JSON.stringify(foundExprs), "the mutated file's expression multiset must no longer equal what's recorded — this is what makes the real REGISTRY-equality test above red").not.toBe(
      JSON.stringify(recordedExprs),
    );
  });

  // CPE-1798: the same "a call site that forgets the wrap must fail CI" AC as CPE-1790's proof above,
  // applied to StatusBar's `notice` — mutate the real, current source to drop BOTH displaySafeName wraps
  // (reverting to the exact pre-fix `{notice}`/`title={notice}` shape the ticket found) and prove the
  // guard reds. See StatusBar.test.ts's companion test for the end-to-end proof that a real bidi override
  // renders the `[RLO]` escape marker, not the raw override character, at both positions today.
  it("CPE-1798 regression proof: reverting StatusBar's notice render to raw {notice}/title={notice} reds the guard", () => {
    const src = readFileSync(join(COMPONENTS, "StatusBar.svelte"), "utf8");
    const ORIGINAL = '<span class="notice" class:error={noticeIsError} title={displaySafeName(notice)}>{displaySafeName(notice)}</span>';
    const SPOOFED = '<span class="notice" class:error={noticeIsError} title={notice}>{notice}</span>';
    expect(src, "StatusBar.svelte no longer contains the exact text this demonstration substitutes — update the fixture").toContain(ORIGINAL);

    const mutated = src.replace(ORIGINAL, SPOOFED);
    const found = findUnsafeRenderLines(mutated, "StatusBar.svelte (mutated: displaySafeName wraps dropped)");
    const foundExprs = exprMultiset(found);
    const recordedExprs = [...REGISTRY["StatusBar.svelte"]].sort((a, b) => a.localeCompare(b));

    // Both the text-content render and the title= render sit on the same line with the same expression
    // (`notice`), so findUnsafeRenderLines' offender Set records ONE line:expr entry covering both —
    // still proof both are flagged, since a Set entry can only exist if at least one render position
    // resolved unsafe, and this file's markup has no OTHER bare `{notice}`/`title={notice}` anywhere.
    expect(found.some((e) => e.endsWith(":notice")), "the un-escaped notice render (text + title, same line) must be flagged").toBe(true);
    expect(JSON.stringify(foundExprs), "the mutated file's expression multiset must no longer equal what's recorded — this is what makes the real REGISTRY-equality test above red").not.toBe(
      JSON.stringify(recordedExprs),
    );
  });

  // CPE-1798 sibling fix: same proof for AgentMenu's `sessionLabel`.
  it("CPE-1798 regression proof: reverting AgentMenu's sessionLabel render to raw {sessionLabel} reds the guard", () => {
    const src = readFileSync(join(COMPONENTS, "AgentMenu.svelte"), "utf8");
    const ORIGINAL = "Open {displaySafeName(sessionLabel)}";
    const SPOOFED = "Open {sessionLabel}";
    expect(src, "AgentMenu.svelte no longer contains the exact text this demonstration substitutes — update the fixture").toContain(ORIGINAL);

    const mutated = src.replace(ORIGINAL, SPOOFED);
    const found = findUnsafeRenderLines(mutated, "AgentMenu.svelte (mutated: displaySafeName wrap dropped)");
    const foundExprs = exprMultiset(found);
    const recordedExprs = [...REGISTRY["AgentMenu.svelte"]].sort((a, b) => a.localeCompare(b));

    expect(found.some((e) => e.includes("sessionLabel")), "the un-escaped sessionLabel render must be flagged").toBe(true);
    expect(JSON.stringify(foundExprs), "the mutated file's expression multiset must no longer equal what's recorded — this is what makes the real REGISTRY-equality test above red").not.toBe(
      JSON.stringify(recordedExprs),
    );
  });

  // The AC's demonstration ("adding a new component… shows CI red without any manual registration step"),
  // reproduced the same way this file's other demonstrations are (the PreviewPane.svelte:1015 substitution
  // test above): mutate a COPY of the real registered-file set to drop one known candidate, and prove the
  // exact enumeration logic the test above runs would flag it as missing — the live end-to-end version of
  // this is the StatusBar.svelte mutation-probe in bidiRenderScan.test.ts (isCandidateComponent flips to
  // `true` the instant a raw {entry.name} lands in it); this proves the OTHER half — that a flagged-but-
  // unregistered file actually fails the membership check, not just the detector.
  it("CPE-1768: the demonstration — dropping a known candidate from REGISTRY reds the membership check", () => {
    const files = readdirSync(COMPONENTS).filter((f) => f.endsWith(".svelte"));
    const DROPPED = "AttributesDialog.svelte";
    expect(REGISTRY, `fixture assumption broken — ${DROPPED} must currently be registered for this demonstration to mean anything`).toHaveProperty(DROPPED);
    const registryWithoutDropped = Object.fromEntries(Object.entries(REGISTRY).filter(([f]) => f !== DROPPED));

    const missing = files.filter((f) => {
      const src = readFileSync(join(COMPONENTS, f), "utf8");
      return isCandidateComponent(src) && !(f in registryWithoutDropped);
    });
    expect(missing, `expected ${DROPPED} to be reported missing once removed from REGISTRY`).toContain(DROPPED);
  });
});
