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
// ticket).
//
// Round 1 of THIS fix (this PR's first attempt) keyed REGISTRY by expression text ALONE, comparing by
// occurrence-counted multiset. Review caught a real hole in it: a `Set`/multiset over bare expression
// text cannot tell two occurrences of the identical text apart AT ALL, only count them — so an edit that
// fixes ONE occurrence of a duplicated expression (wraps it in `displaySafeName`/`displaySafePath`) while
// introducing a brand-new, unrelated raw occurrence of the IDENTICAL text elsewhere in the same
// component leaves the total count unchanged. The multiset stays equal, the guard stays green, and the
// actual unsafe surface has silently moved to a new, unreviewed line — the exact "guard silently stops
// guarding" failure class this ticket exists to close, reintroduced one level down, and WORSE in kind
// than the line-number bug it replaced: that one was a false POSITIVE (noisy, but safe — it only ever
// reported something as raw that had, in fact, moved); this is a false NEGATIVE (something raw passes as
// clean). Demonstrated live in this file's "reviewer's exact swap" test below, before AND after the fix.
//
// The fix (this round): REGISTRY is now keyed by (matched EXPRESSION TEXT, render-position KIND) —
// `findUnsafeRenderSites` (bidiRenderScan.ts) reports each raw occurrence's `kind`: `"text"` for body
// text-node content, `"@html"` for an `{@html …}` block, or the exact attribute name (`"title"`,
// `"aria-label"`, `"alt"`) for an attribute value — never a line address, so it is exactly as stable
// under insertion/deletion/reformatting as bare expression text was (the whole point of this ticket),
// while distinguishing two identical-text occurrences that reach DIFFERENT DOM sinks. Comparison is
// still by MULTISET, not deduplicated membership — `siteKey`/`siteKeyMultiset`/`multisetDiff` below count
// occurrences of each `(kind, expr)` pair on both sides and only pass when every pair's count matches
// exactly, in both directions (TrashView.svelte's two `$t("trash.moreActions")` render positions are one
// `aria-label:` and one `title:` occurrence — genuinely different KEYS now, not merely two of the same
// key — while a component that truly repeats the identical text in the identical position, like
// SplitFileDialog.svelte's `baseName(path)` rendered raw in two separate body-text spots, still needs
// two matching `text:baseName(path)` entries). `findUnsafeRenderLines`'s own `"<line>:<expr>"` output
// shape is UNCHANGED (other tests in this file and `bidiRenderScan.test.ts` depend on it) — only
// REGISTRY's own recorded keys and this file's comparison logic moved to the richer
// `findUnsafeRenderSites`/`UnsafeRenderSite` API bidiRenderScan.ts now also exports.
//
// **Residual, stated precisely rather than implied away (Foreman's explicit instruction after round 1's
// overclaim)**: `(kind, expr)` does NOT distinguish two occurrences of the identical expression in the
// identical KIND within one file — the SplitFileDialog.svelte case above. A swap between exactly those
// two stays invisible, the same way a swap between two same-text/same-line occurrences already was
// invisible under the OLD line-keyed design (a `Set` entry there could only ever record ONE of a
// `title={x}>{x}` pair too — see `findUnsafeRenderLines`'s own doc comment in bidiRenderScan.ts).
// Closing that last residual would need a full occurrence-index, which reintroduces a position-shaped
// key and its own reformatting fragility — exactly the trade this ticket exists to avoid. This is not
// hypothetical noise: regenerating REGISTRY through `findUnsafeRenderSites` (rather than transcribing the
// old line-keyed entries by hand) surfaced 29 previously-invisible occurrences across 20 files, ALL of
// them the `title={x}>{x}`-shaped same-line dual-position case `findUnsafeRenderLines`'s `Set` has always
// collapsed to one entry (e.g. SplitFileDialog.svelte:107's `<span title={outDir}>{outDir}</span>` was
// one recorded `"outDir"` offender before this pass; it is now the two real occurrences it always was,
// `"title:outDir"` and `"text:outDir"`) — a strict precision GAIN from switching to per-occurrence
// tracking, not a scope change to what's being verified, and the same class of previously-silent gap the
// rest of this file's history keeps finding and closing.
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { findUnsafeRenderLines, findUnsafeRenderSites, compareOffenders, isCandidateComponent, type UnsafeRenderSite } from "./bidiRenderScan";

const COMPONENTS = join(process.cwd(), "src", "lib", "components");
const APP = join(process.cwd(), "src", "App.svelte");
const DOC = join(process.cwd(), "src", "docs", "03-explorer.md");

/** Compose one `UnsafeRenderSite` (or a REGISTRY-recorded pair) into the comparable string REGISTRY
 *  entries are stored as: `"<kind>:<expr>"`. `kind` is never a line number (it is `"text"`, `"@html"`, or
 *  an attribute name), so this is stable under reformatting the same way bare expression text was —
 *  see the CPE-1885 header above for why expression text ALONE stopped being enough. */
function siteKey(kind: string, expr: string): string {
  return `${kind}:${expr}`;
}

/** `findUnsafeRenderSites`'s raw (undeduplicated) occurrences, reduced to a sorted MULTISET of
 *  `"<kind>:<expr>"` keys — duplicates are kept, so a component with the identical (kind, expr) pair at
 *  two different lines still requires two matching recorded entries, not one. */
function siteKeyMultiset(sites: Pick<UnsafeRenderSite, "kind" | "expr">[]): string[] {
  return sites.map((s) => siteKey(s.kind, s.expr)).sort((a, b) => a.localeCompare(b));
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

/** file -> the EXACT multiset of `"<kind>:<expr>"` keys `findUnsafeRenderSites` currently reports for it
 *  (line numbers were never part of the key; see the CPE-1885 header above for why `kind` is, now, too).
 *  Recomputed live every run and checked for multiset equality (not "offenders minus this array must be
 *  empty") — see the file header above for why that specific shape closes round 1's inert-allowlist hole,
 *  and the CPE-1885 notes for why the key is `(kind, expr)` rather than a line. A non-empty array is NOT
 *  necessarily a disclosed spoof risk: most entries here are UI text this engine can't prove safe (i18n
 *  params, counts, labels, diagnostic error/note/reason strings, diff/metadata CONTENT, macro/workspace/
 *  rule/ticket/agent identity strings) — read `bidiEscape.doc-parity` below for which specific files'
 *  entries are an actual disclosed filesystem-name/path gap vs. harmless-but-unprovable text. */
const REGISTRY: Record<string, string[]> = {
  "ConflictDialog.svelte": ["text:error || note || `${opLabel || \"No\"} operation in progress`","text:f.label","text:opLabel ? `— ${opLabel}` : \"\"","text:opLabel.toLowerCase()","text:opLabel.toLowerCase()","text:showBase ? \"Hide\" : \"Show\"","text:unresolved","text:versions.base ?? \"— absent —\"","text:versions.ours ?? \"— absent —\"","text:versions.theirs ?? \"— absent —\"","title:unresolved > 0 ? \"Resolve every file first\" : `Continue the ${opLabel.toLowerCase()}`"],
  "FileNameSearchDialog.svelte": ["aria-label:$t(\"search.docsTitle\")","text:$t(\"search.button\")","text:$t(\"search.findByNameTitle\")","text:$t(\"search.noNameMatches\")","text:$t(\"search.searching\")","text:$t(\"search.truncated\")","text:error","text:hits.length === 1 ? $t(\"search.matchOne\", { count: hits.length }) : $t(\"search.matchMany\", { count: hits.length })","title:$t(\"common.close\")","title:$t(\"search.docsTitle\")"],
  "RepoBrowser.svelte": ["text:cloning ? \"Cloning…\" : \"Clone\"","text:consent.host","text:fmtSize(e.size)","text:isGeneric ? \"Git URL\" : \"Repository\"","text:isGeneric ? \"https only\" : \"private repos\"","text:loaded ? repo : \"No repository open\"","text:loading ? \"Browsing…\" : \"Browse\"","text:provider","text:repo","text:statusText"],
  "AgentTimeline.svelte": ["aria-label:`Checkpoint ${m.cp.label || cpShortId(m.cp.manifest_id)}`","aria-label:historyMetric === 'cost' ? 'Cost' : 'Tokens'","text:agentName","text:c.sessionId","text:clock(e.at)","text:clock(e.at)","text:clock(re.ts)","text:clock(replayCurrent.at)","text:cpTime(selectedCheckpoint.ts)","text:entries.length","text:formatBytes(c.churnBytes)","text:formatBytes(c.churnPer1kTokens)","text:formatBytes(historyRollup.ratios.churnPer1kTokens)","text:formatBytes(historyRollup.totals.churnBytes)","text:formatBytes(revertPreview.bytes_written)","text:formatDuration(c.wallClockMs)","text:formatDuration(historyRollup.totals.wallClockMs)","text:formatPerMinute(c.tokensPerMinute)","text:formatPerMinute(historyRollup.ratios.tokensPerMinute)","text:formatTokens(c.editCount)","text:formatTokens(c.filesTouched)","text:formatTokens(c.inputTokens)","text:formatTokens(c.outputTokens)","text:formatTokens(c.totalTokens)","text:formatTokens(historyRollup.totals.filesTouched)","text:formatTokens(historyRollup.totals.sessions)","text:formatTokens(historyRollup.totals.totalTokens)","text:formatTokens(row.sessions)","text:formatTokens(row.sessions)","text:formatTokens(row.totalTokens)","text:formatTokens(row.totalTokens)","text:formatUsd(c.costUsd)","text:formatUsd(c.usdPerFile)","text:formatUsd(historyRollup.ratios.usdPerFile)","text:formatUsd(historyRollup.ratios.usdPerSession)","text:formatUsd(historyRollup.totals.costUsd)","text:formatUsd(row.costUsd)","text:formatUsd(row.costUsd)","text:friendlyActor(a, sessions)","text:friendlyActor(a, sessions)","text:historyBarDate(p.bucketStart)","text:historyDurationLabel(row)","text:historyError","text:historyMetric === \"cost\" ? formatUsd(v) : formatTokens(v)","text:historyRollup.totals.sessions","text:historyRollup.totals.sessions === 1 ? \"\" : \"s\"","text:historyShare(row.costUsd, historyRollup.totals.costUsd)","text:historyShare(row.costUsd, historyRollup.totals.costUsd)","text:historyUnclean","text:historyUnclean === 1 ? \"is\" : \"are\"","text:historyUnclean === 1 ? \"it\" : \"them\"","text:historyUnclean === 1 ? \"its\" : \"their\"","text:isSessionEndedCleanly(row) ? \"Clean\" : \"Ended unexpectedly\"","text:KIND_LABEL[e.kind]","text:KIND_LABEL[e.kind]","text:KIND_LABEL[replayCurrent.kind]","text:Math.round(sliderFraction(range, t) * 100)","text:new Date(row.startedAt).toLocaleString()","text:new Date(t).toLocaleTimeString()","text:playing ? \"Pause\" : \"Play\"","text:rc.kind === \"divergence\" ? \"diverged\" : \"collided\"","text:relativeLabel(o.lastAt, Date.now())","text:relativeLabel(rc.lastAt, Date.now())","text:renameConflictNote(rc.kind)","text:replayKindLabel(re.kind)","text:revertError","text:revertPreview.creates","text:revertPreview.deletes","text:revertPreview.drift_count","text:revertPreview.drift_count","text:revertPreview.drift_count","text:revertPreview.drift_count === 1 ? \"\" : \"s\"","text:revertPreview.drift_count === 1 ? \"\" : \"s\"","text:revertPreview.overwrites","text:revertPreviewError","text:row.agentName","text:row.agentName || row.agentId || \"(unknown)\"","text:row.model","text:s","text:selectedCheckpoint.label || cpShortId(selectedCheckpoint.manifest_id)","text:selectedCheckpoint.label || cpShortId(selectedCheckpoint.manifest_id)","text:stats.add","text:stats.del","title:c.sessionId","title:cpMarkerTitle(m)","title:diff ? `${displaySafePath(e.path)} — hover to see what changed` : displaySafePath(e.path)","title:playing ? \"Pause\" : \"Play\"","title:row.agentName","title:row.agentName || row.agentId","title:row.model","title:selectedCheckpoint.manifest_id"],
  "ConsultedFiles.svelte": ["aria-label:e.count","text:$agentConsulted.length","text:e.count"],
  "SessionHistoryDialog.svelte": ["text:e.kind","text:error","text:filtered.length","text:filtered.length === 1 ? \"\" : \"s\"","text:formatDate(e.ts)","text:k","text:s"],
  "IntegrityDialog.svelte": ["text:error","text:hasBaseline ? `Baseline: ${baseline.length} files` : \"No baseline stored\"","text:label","text:list.length","text:note","text:report.corrupted.length","text:report.edited.length","text:report.intact.length","text:report.intact.length","text:report.missing.length","text:report.new.length"],
  "CheckpointDialog.svelte": ["text:$t(\"ckpt.failedTitle\")","text:cf.operation","text:cf.reason","text:cp.label || shortId(cp.manifest_id)","text:diffError","text:diffOpenPath === p ? \"Close diff\" : \"Open diff\"","text:error","text:fmtBytes(preview.bytes_written)","text:fmtTime(cf.ts)","text:fmtTime(cp.ts)","text:note","text:preview.creates","text:preview.deletes","text:preview.drift_count","text:preview.overwrites","text:selected.label || shortId(selected.manifest_id)","text:selected.label || shortId(selected.manifest_id)","text:shortId(cp.manifest_id)","title:$t('ckpt.failedTitle')","title:cf.reason"],
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
  "RevertOutcomePanel.svelte": ["text:copied ? \"Copied\" : `Copy all ${summary.heldBack} held-back path${summary.heldBack === 1 ? \"\" : \"s\"}`","text:headline","text:summary.more","text:summary.nextStep","text:summary.reason"],
  "DiffSideBySide.svelte": ["text:r.left ?? \"\"","text:r.right ?? \"\""],
  "InspectCryptoDialog.svelte": [],
  "BoardView.svelte": ["aria-label:\"Copy \" + c.id","aria-label:\"Copy \" + e.id","text:bar.label","text:boardQuery.trim()","text:boardQuery.trim()","text:c.epic","text:c.id","text:c.priority","text:c.sprint","text:c.title","text:col","text:col","text:copiedId === c.id ? \"✓\" : \"⧉\"","text:copiedId === e.id ? \"✓\" : \"⧉\"","text:e.id","text:e.status","text:e.title","text:error","text:error || note || \"\"","text:grouped[l].length","text:l","text:list.length","text:list.length","text:showArchived ? \"hide\" : `+${archived.length} archived`","text:showArchived ? \"hide\" : `+${archivedEpicList.length} archived`","text:t","title:\"Open \" + c.id + \" — details\"","title:\"Open \" + e.id + \" — details\"","title:bar.state === \"empty\" ? \"No sub-tickets yet\" : bar.state === \"complete\" && p.total === 0 ? \"Epic complete\" : p.done + \" of \" + p.total + \" tickets done\""],
  "CopilotDialog.svelte": ["text:execError","text:execResult.results.filter((r) => r.ok).length","text:execResult.results.length","text:instruction","text:opKind(op)","text:phase === \"planning\" ? \"Planning…\" : \"Plan\"","text:planError","text:planResult.summary.copies","text:planResult.summary.deletes","text:planResult.summary.deletes === 1 ? \"\" : \"s\"","text:planResult.summary.mkdirs","text:planResult.summary.mkdirs === 1 ? \"\" : \"s\"","text:planResult.summary.moves","text:planResult.summary.moves === 1 ? \"\" : \"s\"","text:planResult.summary.renames","text:planResult.summary.renames === 1 ? \"\" : \"s\"","text:r.error","text:undoError","text:undoing ? \"Undoing…\" : \"Undo\"","text:v","text:v","title:execResult.checkpoint.checkpoint.manifest_id"],

  // --- B4: the 19 components CPE-1712 itself originally escaped ---------------------------------
  "FileList.svelte": ["aria-label:$t(\"fl.agentLegend\")","aria-label:$t(\"fl.columnsButton\")","aria-label:$t(\"fl.resizeColumn\", { col: handleLabel(i) })","text:$t(\"fl.loading\")","text:$t(ACTIVITY_LABEL_KEY[act.kind])","text:$t(col.labelKey)","text:ac.col.label","text:cell.display","text:error","text:folderSizes.has(entry.path) ? formatSize(folderSizes.get(entry.path) ?? 0) : \"…\"","text:formatDate(entry.modified)","text:formatSize(entry.size)","text:friendlyActor(a, sessions)","text:ruleStyle.label","text:searching ? $t(\"fl.noMatch\") : $t(\"fl.empty\")","text:tag","text:typeName(entry)","title:$t(\"fl.agentInside\")","title:$t(\"fl.columnsButton\")","title:$t(\"fl.resizeTip\")","title:$t(\"fl.sortBy\", { col: $t(col.labelKey) })","title:$t(\"fl.sortBy\", { col: ac.col.label })","title:tagEntry.label"],
  "Sidebar.svelte": ["text:$t(\"sidebar.agents\")","text:$t(\"sidebar.drives\")","text:$t(\"sidebar.explore\")","text:$t(\"sidebar.quickAccess\")","text:$t(\"sidebar.repositories\")","text:$t(\"sidebar.trash\")","text:$t(\"smart.searchSection\")","text:$t(\"smart.section\")","text:$t(\"trash.macLabel\")","text:$t(\"trash.open\")","text:baseName(s.cwd)","text:count","text:formatSize(u.free)","text:model","text:s.agentName || s.agentId || \"Agent\"","text:sessionNum(s.sessionId)","text:sf.name","text:ss.name","text:tag","title:`${conn.scheme}://${conn.host} — ${stateTitle(state, connectionErrors[conn.name])} (right-click for more)`","title:`${count} item${count === 1 ? \"\" : \"s\"} tagged “${tag}” — click to filter, right-click to rename/delete`","title:`${formatSize(u.free)} free of ${formatSize(u.total)}`","title:`${s.agentName}${s.provider ? \" · \" + s.provider : \"\"}${s.model ? \" · \" + s.model : \"\"} · ${s.cwd} (double-click to open its tab · right-click for more)`","title:$t(\"smart.itemTip\", { tag: sf.tag })","title:$t(\"smart.searchItemTip\")","title:$t(\"trash.macMessage\")","title:$t(\"trash.openTip\")","title:agentsOpen ? \"Collapse\" : \"Expand\"","title:drivesOpen ? \"Collapse\" : \"Expand\"","title:exploreOpen ? \"Collapse\" : \"Expand\"","title:favOpen ? \"Collapse\" : \"Expand\"","title:networkOpen ? \"Collapse\" : \"Expand\"","title:open ? \"Collapse\" : \"Expand\"","title:placesOpen ? \"Collapse\" : \"Expand\"","title:savable ? `${displaySafePath(s.path)} — discovered on your network; click to add it as a connection` : `${displaySafePath(s.path)} — discovered on your network; ${prefill.scheme.toUpperCase()} isn't supported yet`","title:savedSearchOpen ? \"Collapse\" : \"Expand\"","title:smartOpen ? \"Collapse\" : \"Expand\"","title:tagsOpen ? \"Collapse\" : \"Expand\"","title:trashOpen ? \"Collapse\" : \"Expand\""],
  "TabBar.svelte": ["title:$t(\"app.closeTab\")","title:$t(\"app.newTab\")"],
  "HomeView.svelte": ["aria-label:$t(\"home.removeFromRecent\")","aria-label:$t(\"home.removeFromRecentFolders\")","aria-label:$t(\"home.removeNetworkLocation\")","text:$t(\"common.cancel\")","text:$t(\"home.add\")","text:$t(\"home.addNetworkLocation\")","text:$t(\"home.clear\")","text:$t(\"home.dateOpened\")","text:$t(\"home.favorites\")","text:$t(\"home.folders\")","text:$t(\"home.name\")","text:$t(\"home.noFavorites\")","text:$t(\"home.noFavoritesSub\")","text:$t(\"home.noRecent\")","text:$t(\"home.noRecentFolders\")","text:$t(\"home.noRecentFoldersSub\")","text:$t(\"home.noRecentSub\")","text:$t(\"home.noShared\")","text:$t(\"home.noSharedSub\")","text:$t(\"home.quickAccess\")","text:$t(\"home.recent\")","text:$t(\"home.shared\")","text:$t(\"home.sharedLoading\")","text:formatDate(r.opened)","text:tab === \"favorites\" ? $t(\"home.favorites\") : tab === \"folders\" ? $t(\"home.recentFolders\") : tab === \"shared\" ? $t(\"home.shared\") : $t(\"home.recent\")","title:$t(\"home.removeFromFavorites\")","title:$t(\"home.removeFromRecent\")","title:$t(\"home.removeFromRecentFolders\")","title:$t(\"home.removeNetworkLocation\")","title:$t(\"home.unpinQuick\")","title:quickOpen ? $t(\"home.collapse\") : $t(\"home.expand\")","title:recentOpen ? $t(\"home.collapse\") : $t(\"home.expand\")"],
  "DetailsPane.svelte": ["text:formatDate(one.modified) || \"—\"","text:formatSize(one.size) || \"0 B\"","text:formatSize(totalSize) || \"0 B\"","text:itemCount","text:itemCount === 1 ? \"\" : \"s\"","text:selected.filter((e) => !e.is_dir).length","text:selected.filter((e) => e.is_dir).length","text:selected.length","text:typeName(one)"],
  // CPE-1827: line numbers reshuffled by the titlebar overflow-menu rewrite (recomputed via
  // `findUnsafeRenderLines` against the new file — see the ticket's Work Log). Same offenders as before
  // (all i18n params/labels/counts, none a raw filesystem name/path) plus two new `$t("trash.moreActions")`
  // entries (the overflow trigger's `title`/`aria-label`) and one new `$t("trash.docs")` (the overflow
  // menu's Docs row, replacing the removed `HelpButton` usage) — all plain static-key i18n lookups.
  "TrashView.svelte": ["aria-label:$t(\"trash.moreActions\")","aria-label:$t(\"trash.selectAll\")","text:$t(\"trash.columnsDeleted\")","text:$t(\"trash.columnsName\")","text:$t(\"trash.columnsOriginalPath\")","text:$t(\"trash.docs\")","text:$t(\"trash.empty\")","text:$t(\"trash.emptyAll\")","text:$t(\"trash.emptySelected\")","text:$t(\"trash.error\", { error })","text:$t(\"trash.loading\")","text:$t(\"trash.refresh\")","text:$t(\"trash.restoreFailed\", { name: displaySafeName(f.name), error: f.error })","text:$t(\"trash.restoreSelected\")","text:$t(\"trash.stillLoading\")","text:$t(\"trash.title\")","text:allSelected ? $t(\"trash.deselectAll\") : $t(\"trash.selectAll\")","text:degradedMessage","text:formatDate(e.time_deleted * 1000)","text:formatSize(e.size)","text:itemCountLabel","text:noticeMessage","text:selectedCountLabel","text:selectedCountLabel","text:selectedCountLabel","title:$t(\"trash.emptyConfirmTitle\")","title:$t(\"trash.moreActions\")"],
  "NavToolbar.svelte": ["aria-label:$t('nav.search')","aria-label:density === \"compact\" ? \"Switch to comfortable density\" : \"Switch to compact density\"","aria-label:searchScope","title:$t('nav.back')","title:$t('nav.forward')","title:$t('nav.refresh')","title:$t('nav.up')","title:$t(\"nav.searchHint\")","title:density === \"compact\" ? \"Switch to comfortable density\" : \"Switch to compact density\""],
  "PropertiesDialog.svelte": ["text:[info.readonly ? $t(\"prop.readonly\") : null, info.hidden ? $t(\"prop.hidden\") : null] .filter(Boolean) .join(\", \") || $t(\"prop.none\")","text:$t(\"common.close\")","text:$t(\"prop.attributes\")","text:$t(\"prop.calculating\")","text:$t(\"prop.compute\")","text:$t(\"prop.computing\")","text:$t(\"prop.contents\")","text:$t(\"prop.contentStats\", { lines: stats.lines.toLocaleString(), words: stats.words.toLocaleString(), chars: stats.chars.toLocaleString() })","text:$t(\"prop.count\")","text:$t(\"prop.counting\")","text:$t(\"prop.created\")","text:$t(\"prop.files\")","text:$t(\"prop.folderNote\")","text:$t(\"prop.folders\")","text:$t(\"prop.itemsSelected\", { count: entries.length })","text:$t(\"prop.location\")","text:$t(\"prop.match\")","text:$t(\"prop.modified\")","text:$t(\"prop.noMatch\")","text:$t(\"prop.note\")","text:$t(\"prop.size\")","text:$t(\"prop.size\")","text:$t(\"prop.sizeBytes\", { size: formatSize(folderSize) || \"0 B\", bytes: folderSize.toLocaleString() })","text:$t(\"prop.sizeBytes\", { size: formatSize(single.size) || \"0 B\", bytes: single.size.toLocaleString() })","text:$t(\"prop.sizeBytes\", { size: formatSize(totalSize) || \"0 B\", bytes: totalSize.toLocaleString() })","text:$t(\"prop.sizeOfFiles\")","text:$t(\"prop.title\")","text:$t(\"prop.type\")","text:$t(\"prop.typeMismatch\")","text:$t(\"prop.unavailable\")","text:checksum","text:copied ? $t(\"prop.copied\") : $t(\"prop.copy\")","text:error","text:fileCount","text:folderCount","text:formatDate(info.created) || \"—\"","text:formatDate(info.modified) || \"—\"","text:hashError","text:inspection.type_mismatch","text:label","text:label","text:nativeEntry.label || \"None\"","text:nativeError","text:nativePulling ? \"Pulling…\" : \"Pull\"","text:nativeStoreName","text:statError","text:tag","text:typeName(single)","text:value","text:value","title:$t(\"common.close\")","title:$t(\"prop.copyChecksumTip\")","title:$t(\"prop.matchTip\")","title:$t(\"prop.noMatchTip\")"],
  "InstantSearch.svelte": ["aria-label:$t(\"common.close\")","aria-label:$t(\"search.docsTitle\")","aria-label:$t(\"search.instantPlaceholder\")","aria-label:$t(\"search.instantTitle\")","text:$t(\"search.buildIndex\")","text:$t(\"search.buildingIndex\", { count: buildStats?.dirs_scanned ?? 0 })","text:$t(\"search.instantNoMatches\")","text:$t(\"search.instantOffBody\")","text:$t(\"search.instantOffTitle\")","text:$t(\"search.instantOpenFolderFirst\")","text:$t(\"search.instantTitle\")","text:$t(\"search.instantTypeHint\")","text:$t(\"search.searching\")","text:buildError","text:error","title:$t(\"common.close\")","title:$t(\"search.docsTitle\")"],
  "ArchiveSafetyDialog.svelte": ["aria-label:$t(\"arcsafe.title\")","text:$t(\"arcsafe.capped\")","text:$t(\"arcsafe.dangerous\")","text:$t(\"arcsafe.encrypted\", { count: result.unreadable_entries })","text:$t(\"arcsafe.encrypted\", { count: result.unreadable_entries })","text:$t(\"arcsafe.entries\")","text:$t(\"arcsafe.flaggedHead\", { count: result.report.flagged.length })","text:$t(\"arcsafe.noneFlagged\")","text:$t(\"arcsafe.ratio\")","text:$t(\"arcsafe.retry\")","text:$t(\"arcsafe.safe\")","text:$t(\"arcsafe.scanning\")","text:$t(\"arcsafe.sizes\")","text:$t(\"arcsafe.title\")","text:$t(\"arcsafe.unreadable\")","text:$t(\"arcsafe.unreadableEntries\")","text:error","text:ratioLabel(f.ratio)","text:ratioLabel(result.report.overall_ratio)","text:result.entries_scanned.toLocaleString()","text:result.unreadable_entries.toLocaleString()","text:sizeLabel(result.report.total_compressed)","text:sizeLabel(result.report.total_uncompressed)","title:$t(\"common.close\")"],
  "PreviewPane.svelte": ["@html:line","@html:mdHtml","aria-label:`Jump to ${sym.kind} ${sym.name}, line ${sym.line}`","aria-label:foldCollapsed.has(i + 1) ? `Expand lines ${i + 1}–${foldByStart.get(i + 1)?.end_line}` : `Collapse lines ${i + 1}–${foldByStart.get(i + 1)?.end_line}`","text:$t(\"common.cancel\")","text:$t(\"ctx.selectAll\")","text:$t(\"menu.copy\")","text:$t(\"menu.cut\")","text:$t(\"menu.paste\")","text:$t(\"pv.cantArchive\")","text:$t(\"pv.cantFile\")","text:$t(\"pv.cantFile\")","text:$t(\"pv.cantImage\")","text:$t(\"pv.dicom.title\")","text:$t(\"pv.edit\")","text:$t(\"pv.json.viewRaw\")","text:$t(\"pv.json.viewTree\")","text:$t(\"pv.loading\")","text:$t(\"pv.loading\")","text:$t(\"pv.loading\")","text:$t(\"pv.loading\")","text:$t(\"pv.loading\")","text:$t(\"pv.loading\")","text:$t(\"pv.loading\")","text:$t(\"pv.loading\")","text:$t(\"pv.model.dimensions\")","text:$t(\"pv.model.encoding\")","text:$t(\"pv.model.format\")","text:$t(\"pv.model.meshes\")","text:$t(\"pv.model.title\")","text:$t(\"pv.model.vertices\")","text:$t(\"pv.showingRows\", { cap: CSV_ROW_CAP, total: tableRows.length })","text:$t(action.labelKey)","text:actionMessage","text:breadcrumbSym.name","text:cell","text:e.is_dir ? \"\" : formatSize(e.size)","text:entries.length === 1 ? $t(\"pv.itemOne\", { count: entries.length }) : $t(\"pv.itemMany\", { count: entries.length })","text:fmtDim(modelDims.d)","text:fmtDim(modelDims.h)","text:fmtDim(modelDims.w)","text:foldLen(i + 1)","text:info","text:modelCountLabel","text:modelFormatLabel","text:modelInfo.ascii ? $t(\"pv.model.ascii\") : $t(\"pv.model.binary\")","text:modelInfo.mesh_count.toLocaleString()","text:modelInfo.triangle_count.toLocaleString()","text:modelInfo.vertex_count.toLocaleString()","text:name","text:prettyJson(text)","text:saveError","text:saving ? $t(\"pv.saving\") : $t(\"pv.save\")","text:sym.name","text:value","title:`${sym.name} — line ${sym.line}`","title:$t(action.labelKey)"],
  "QuickLook.svelte": ["text:images.length","text:index + 1"],
  "DiskSpaceView.svelte": ["text:error","text:formatSize(c?.size ?? 0)","text:formatSize(c?.size ?? 0)","text:formatSize(c.size)","text:formatSize(total)","text:loading ? \" · scanning…\" : \"\"","text:pct(c?.size ?? 0)"],
  "DropStackPanel.svelte": ["aria-label:open ? \"Hide Drop Stack\" : \"Show Drop Stack\"","text:$dropStackEntries.length","title:canTransfer ? \"Copy every shelved item into the current folder\" : \"Not a valid destination for the Drop Stack\"","title:canTransfer ? \"Move every shelved item into the current folder\" : \"Not a valid destination for the Drop Stack\""],
  "FolderBrowser.svelte": ["text:$t(\"fl.empty\")","text:$t(\"pv.folder.cantOpen\")","text:$t(\"pv.loading\")","text:formatSize(entry.size)"],
  "SidebarNode.svelte": ["title:open ? \"Collapse\" : \"Expand\""],
  "RunCommandConfirm.svelte": ["text:commands.length","text:commands.length === 1 ? \"command\" : \"commands\"","text:cwd ? ` in ${displaySafePath(cwd)}` : \"\"","text:r.code ?? \"signal\"","text:r.command","text:r.error","text:r.stderr","text:r.stdout","text:r.truncated ? \" · output truncated\" : \"\"","text:running ? \"Running…\" : \"Run\""],
  "ContentSearchDialog.svelte": ["aria-label:$t(\"search.docsTitle\")","aria-label:$t(\"search.filterResultsAria\")","aria-label:$t(\"search.toggleFile\")","text:$t(\"search.button\")","text:$t(\"search.inFilesTitle\")","text:$t(\"search.matchesInFiles\", { matches: result.matches.length === 1 ? $t(\"search.matchOne\", { count: result.matches.length }) : $t(\"search.matchMany\", { count: result.matches.length }), files: groups.length === 1 ? $t(\"search.fileOne\", { count: groups.length }) : $t(\"search.fileMany\", { count: groups.length }), })","text:$t(\"search.noFilesMatch\", { query: resultFilter.trim() })","text:$t(\"search.noMatchesInFolder\")","text:$t(\"search.searching\")","text:$t(\"search.shown\", { count: shownGroups.length })","text:$t(\"search.truncated\")","text:collapsedFiles.has(g.path) ? \"▸\" : \"▾\"","text:error","text:g.matches.length","text:mt.line_number","text:seg.text","text:seg.text","title:$t(\"common.close\")","title:$t(\"search.docsTitle\")","title:$t(\"search.matchCase\")","title:collapsedFiles.has(g.path) ? $t(\"home.expand\") : $t(\"home.collapse\")"],
  "DuplicatesDialog.svelte": ["text:$t(\"dup.capped\")","text:$t(\"dup.copiesEach\", { count: g.paths.length, size: formatSize(g.size) || \"0 B\" })","text:$t(\"dup.extra\", { size: formatSize(g.size * (g.paths.length - 1)) || \"0 B\" })","text:$t(\"dup.intro\")","text:$t(\"dup.none\", { count: result.files_scanned.toLocaleString() })","text:$t(\"dup.scan\")","text:$t(\"dup.scanning\")","text:$t(\"dup.selectRedundant\")","text:$t(\"dup.title\")","text:deleting ? $t(\"dup.removing\") : $t(\"dup.moveToBin\", { count: selected.size })","text:error","text:result.groups.length === 1 ? $t(\"dup.summaryOne\", { count: result.groups.length, size: formatSize(reclaimable) || \"0 B\" }) : $t(\"dup.summaryMany\", { count: result.groups.length, size: formatSize(reclaimable) || \"0 B\" })","title:$t(\"common.close\")","title:$t(\"dup.markForBin\")","title:$t(\"dup.selectRedundantTip\")"],

  // --- The ticket's originally-disclosed "not yet covered" dialogs — pinned exactly, not fixed here ---
  "ContentIndexSearchDialog.svelte": ["aria-label:$t(\"common.close\")","aria-label:$t(\"search.byContentPlaceholder\")","aria-label:$t(\"search.docsTitle\")","text:$t(\"search.buildContentIndex\")","text:$t(\"search.buildingContentIndex\", { count: buildProgress?.files_indexed ?? 0 })","text:$t(\"search.buildingContentIndex\", { count: buildProgress?.files_indexed ?? 0 })","text:$t(\"search.byContentNeedsBuildBody\")","text:$t(\"search.byContentNeedsBuildTitle\")","text:$t(\"search.byContentNoMatches\")","text:$t(\"search.byContentTitle\")","text:$t(\"search.byContentTypeHint\")","text:$t(\"search.checkingContentIndex\")","text:$t(\"search.rebuildContentIndex\")","text:$t(\"search.searching\")","text:baseName(h.path)","text:baseName(root) || root","text:buildError","text:buildProgress.current_path","text:error","text:hits.length === 1 ? $t(\"search.matchOne\", { count: hits.length }) : $t(\"search.matchMany\", { count: hits.length })","text:relativeToRoot(h.path, root)","text:scorePercent(h.score)","text:seg.text","text:seg.text","title:$t(\"common.close\")","title:$t(\"search.byContentScoreTitle\")","title:$t(\"search.docsTitle\")","title:$t(\"search.rebuildContentIndex\")","title:buildProgress.current_path","title:h.path","title:root"],
  "FileHealthDialog.svelte": ["aria-label:$t(\"fh.excludeAddLabel\")","aria-label:$t(\"fh.excludeRemove\")","aria-label:$t(\"fh.title\")","text:$t(\"fh.capped\")","text:$t(\"fh.capped\")","text:$t(\"fh.capped\")","text:$t(\"fh.capped\")","text:$t(\"fh.excludeEmpty\")","text:$t(\"fh.excludeHint\")","text:$t(\"fh.excludeLabel\")","text:$t(\"fh.excludeSuggest\")","text:$t(\"fh.intro\")","text:$t(\"fh.introEmpty\")","text:$t(\"fh.introMismatch\")","text:$t(\"fh.introOrphan\")","text:$t(\"fh.mismatchBadge\", { claimed: h.claimedExt, detected: h.detectedLabel })","text:$t(\"fh.none\", { count: scanned.toLocaleString() })","text:$t(\"fh.noneEmpty\", { count: emptyScanned.toLocaleString() })","text:$t(\"fh.noneMismatch\", { count: mismatchScanned.toLocaleString() })","text:$t(\"fh.noneOrphan\", { count: orphanScanned.toLocaleString() })","text:$t(\"fh.orphanBadge\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scan\")","text:$t(\"fh.scanned\", { count: emptyScanned.toLocaleString() })","text:$t(\"fh.scanned\", { count: mismatchScanned.toLocaleString() })","text:$t(\"fh.scanned\", { count: orphanScanned.toLocaleString() })","text:$t(\"fh.scanned\", { count: scanned.toLocaleString() })","text:$t(\"fh.scanning\")","text:$t(\"fh.scanning\")","text:$t(\"fh.scanning\")","text:$t(\"fh.scanning\")","text:$t(\"fh.title\")","text:$t(tab.labelKey)","text:baseName(d.path)","text:baseName(h.path)","text:baseName(l.path)","text:baseName(o.path)","text:baseName(root) || root","text:emptyDirs.length === 1 ? $t(\"fh.summaryOneEmpty\", { count: emptyDirs.length }) : $t(\"fh.summaryManyEmpty\", { count: emptyDirs.length })","text:emptyError","text:error","text:h.fixError","text:h.fixing ? $t(\"fh.mismatchFixing\") : $t(\"fh.mismatchFix\", { ext: h.detectedExt })","text:links.length === 1 ? $t(\"fh.summaryOne\", { count: links.length }) : $t(\"fh.summaryMany\", { count: links.length })","text:mismatchError","text:mismatchHits.length === 1 ? $t(\"fh.summaryOneMismatch\", { count: mismatchHits.length }) : $t(\"fh.summaryManyMismatch\", { count: mismatchHits.length })","text:orphanError","text:orphans.length === 1 ? $t(\"fh.summaryOneOrphan\", { count: orphans.length }) : $t(\"fh.summaryManyOrphan\", { count: orphans.length })","text:parentDir(d.path)","text:parentDir(h.path)","text:parentDir(l.path)","text:parentDir(o.path)","text:pattern","text:reasonLabel(l.reason)","text:s","title:$t(\"common.close\")","title:$t(\"fh.excludeRemove\")","title:$t(\"fh.mismatchFix\", { ext: h.detectedExt })","title:d.path","title:h.path","title:l.path","title:o.path","title:pattern","title:root"],
  "NearDuplicatesDialog.svelte": ["aria-label:title","text:$t(\"nd.groupHead\", { count: g.paths.length })","text:$t(\"nd.intro\")","text:$t(\"nd.none\", { count: scannedCount.toLocaleString() })","text:$t(\"nd.scan\")","text:$t(\"nd.scan\")","text:$t(\"nd.scan\")","text:$t(\"nd.scan\")","text:$t(\"nd.scanning\")","text:$t(\"sim.capped\")","text:$t(\"sim.scanned\", { count: scannedCount.toLocaleString() })","text:$t(\"sim.selectExtras\")","text:baseName(p)","text:baseName(root) || root","text:deleting ? $t(\"sim.removing\") : $t(\"sim.moveToBin\", { count: selected.size })","text:error","text:groups.length === 1 ? $t(\"nd.summaryOne\", { count: groups.length }) : $t(\"nd.summaryMany\", { count: groups.length })","text:parentDir(p)","text:title","title:$t(\"common.close\")","title:$t(\"nd.markForBin\")","title:$t(\"nd.selectExtrasTip\")","title:p","title:root"],
  "SimilarImagesDialog.svelte": ["aria-label:$t(\"sim.title\")","text:$t(\"sim.capped\")","text:$t(\"sim.groupHead\", { count: g.paths.length })","text:$t(\"sim.intro\")","text:$t(\"sim.none\", { count: filesScanned.toLocaleString() })","text:$t(\"sim.scan\")","text:$t(\"sim.scan\")","text:$t(\"sim.scan\")","text:$t(\"sim.scan\")","text:$t(\"sim.scanned\", { count: filesScanned.toLocaleString() })","text:$t(\"sim.scanning\")","text:$t(\"sim.selectExtras\")","text:$t(\"sim.title\")","text:baseName(p)","text:baseName(root) || root","text:deleting ? $t(\"sim.removing\") : $t(\"sim.moveToBin\", { count: selected.size })","text:error","text:groups.length === 1 ? $t(\"sim.summaryOne\", { count: groups.length }) : $t(\"sim.summaryMany\", { count: groups.length })","text:parentDir(p)","title:$t(\"common.close\")","title:$t(\"sim.markForBin\")","title:$t(\"sim.selectExtrasTip\")","title:p","title:root"],
  "DeclutterDialog.svelte": ["aria-label:$t(\"dc.title\")","text:$t(\"dc.intro\")","text:$t(\"dc.none\")","text:$t(\"dc.scan\")","text:$t(\"dc.scan\")","text:$t(\"dc.scan\")","text:$t(\"dc.scan\")","text:$t(\"dc.scanning\")","text:$t(\"dc.title\")","text:baseName(root) || root","text:deleting ? $t(\"dc.removing\") : $t(\"dc.moveToBin\", { count: selected.size })","text:error","text:f.name","text:findings.length === 1 ? $t(\"dc.summaryOne\", { count: findings.length }) : $t(\"dc.summaryMany\", { count: findings.length })","text:g.rows.length","text:reasonLabel(g.reason)","title:$t(\"common.close\")","title:$t(\"dc.markForBin\")","title:f.path","title:root"],
  "BatchMediaDialog.svelte": ["text:$t(\"bm.convertEscapes\")","text:$t(\"bm.renameEscapes\")","text:applyError","text:applying ? \"Applying…\" : \"Apply\"","text:baseName(dir) || dir","text:baseName(it.input)","text:baseName(it.output)","text:baseName(p.dir) || p.dir","text:checkpointFailures.length","text:checkpointFailures.length === 1 ? \"\" : \"s\"","text:checkpointFailures.length === 1 ? \"that folder\" : \"those folders\"","text:checkpointPartial.length","text:checkpointPartial.length === 1 ? \"\" : \"s\"","text:checkpointPartial.length === 1 ? \"that folder\" : \"those folders\"","text:completed.skipped.length","text:completed.written","text:done","text:failed > 0 ? `, ${failed} failed` : \"\"","text:it.summary","text:MAX_PREVIEW","text:mediaOpLabel(op)","text:overwriteItems.length","text:overwriteItems.length","text:overwriteItems.length === 1 ? \"\" : \"s\"","text:overwriteItems.length === 1 ? \"\" : \"s\"","text:p.skippedCount","text:p.skippedCount === 1 ? \"\" : \"s\"","text:paths.length","text:paths.length === 1 ? \"\" : \"s\"","text:planError","text:planned.length","text:planned.length === 1 ? \"\" : \"s\"","text:previewCappedTotal","text:RENAME_DEFAULT_TEMPLATE","text:s.name","text:s.reason","text:total","text:uniqueParentDirs(overwriteItems.map((it) => it.input)).length === 1 ? \"\" : \"s\"","text:watermarkImage ? baseName(watermarkImage) : \"No image chosen (no watermark)\"","title:dir","title:it.input","title:it.output","title:p.dir","title:s.name","title:watermarkImage || \"No image chosen — no watermark\""],
  "SplitFileDialog.svelte": ["text:baseName(path)","text:baseName(path)","text:busy ? \"Splitting…\" : \"Split\"","text:error","text:formatSize(result.part_size)","text:formatSize(result.total_size)","text:outDir","text:p.label","text:result.part_count","text:result.part_count","text:result.part_count === 1 ? \"\" : \"s\"","title:outDir","title:outDir"],
  "JoinPartsDialog.svelte": ["text:baseName(joinedPath)","text:baseName(path)","text:busy ? \"Joining…\" : \"Join\"","text:error","text:formatSize(preview.totalSize)","text:joinedPath","text:preview.partCount","text:previewError","title:joinedPath","title:outPath"],
  "ExplorerPane.svelte": ["aria-label:$t(\"tb.fileList\")","text:$agentTimeline.length ? `(${$agentTimeline.length})` : \"\"","text:$t(\"agent.log\")","text:$t(\"agent.watch\", { name: watchedAgentName })","text:$t(\"agent.watching\")","text:$t(\"cmd.ascending\")","text:$t(\"cmd.descending\")","text:$t(\"cmd.showHidden\")","text:$t(\"menu.view\")","text:$t(\"sort.name\")","text:$t(\"sort.size\")","text:$t(\"sort.type\")","text:$t(\"tb.direction\")","text:$t(\"tb.icons\")","text:$t(\"tb.modified\")","text:$t(\"tb.sortBy\")","text:$t(\"view.details\")","text:$t(\"view.gallery\")","text:$t(\"view.list\")","text:baseName(c.path)","text:c.kind === \"removed\" ? \"−\" : c.kind === \"created\" ? \"+\" : \"~\"","text:selectedTag","text:visible.length","title:$t(\"agent.showLog\")","title:c.path"],
  "TerminalPanel.svelte": ["text:basename(t.cwd) || \"shell\"","text:c.label","text:openError","title:t.cwd"],

  // --- CPE-1768: newly-registered candidates discovered by the membership-rule sweep (45 files) ---
  "AboutDialog.svelte": ["text:h.label","text:s.contract || \"—\"","text:s.name","text:s.version || \"—\"","text:version || \"—\"","title:s.id"],
  "AttributesDialog.svelte": ["text:ch","text:error","text:error","text:heading","text:modePreview","text:notice","text:targets.length"],
  "BackupDashboard.svelte": ["text:error","text:fmtTime(run.when)","text:fmtTime(st.when)","text:history[job.id].length","text:history[job.id].length === 1 ? \"\" : \"s\"","text:job.name","text:plan.copy.length","text:plan.delete.length","text:plan.unchanged","text:plan.update.length","text:progress","text:run.failed ? `, ${run.failed} failed` : \"\"","text:run.label","text:run.ok","text:showHistory === job.id ? \"▾\" : \"▸\"","text:st.failed ? `, ${st.failed} failed` : \"\"","text:st.label","text:st.ok","text:total ? ` / ${total}` : \"\""],
  "BinaryPreview.svelte": ["text:cultureLabel(dotnetMeta.assembly.culture)","text:cultureLabel(r.culture)","text:dotnetMeta.assembly.name","text:dotnetMeta.assembly.version","text:dotnetMeta.runtime_version","text:e.name","text:f","text:fmtCount(assemblyRefsCap.total)","text:fmtCount(BINARY_TABLE_ROW_CAP)","text:fmtCount(BINARY_TABLE_ROW_CAP)","text:fmtCount(BINARY_TABLE_ROW_CAP)","text:fmtCount(BINARY_TABLE_ROW_CAP)","text:fmtCount(BINARY_TABLE_ROW_CAP)","text:fmtCount(BINARY_TABLE_ROW_CAP)","text:fmtCount(BINARY_TABLE_ROW_CAP)","text:fmtCount(disasm.length)","text:fmtCount(dotnetMeta.assembly_refs.length)","text:fmtCount(dotnetMeta.methods.length)","text:fmtCount(dotnetMeta.types.length)","text:fmtCount(exportsCap.total)","text:fmtCount(importsCap.total)","text:fmtCount(info.exports.length)","text:fmtCount(info.exports.length)","text:fmtCount(info.imports.length)","text:fmtCount(info.imports.length)","text:fmtCount(info.sections.length)","text:fmtCount(info.sections.length)","text:fmtCount(info.symbols.length)","text:fmtCount(info.symbols.length)","text:fmtCount(methodsCap.total)","text:fmtCount(sectionsCap.total)","text:fmtCount(symbolsCap.total)","text:fmtCount(typesCap.total)","text:formatLabel(info.format)","text:formatSize(s.size)","text:formatSize(size)","text:hexAddress(e.address)","text:hexAddress(ins.address)","text:hexAddress(s.address)","text:hexAddress(s.address)","text:hexOrDash(dotnetMeta.assembly.public_key)","text:hexOrDash(r.public_key_token)","text:i.library ?? \"—\"","text:i.name","text:info.arch ?? \"Unknown\"","text:info.format === \"Pe\" ? \"No symbol table — a typical PE EXE/DLL doesn't carry one (only object files and PDBs do).\" : \"No symbols found.\"","text:info.is_64 ? \"64-bit\" : \"32-bit\"","text:ins.bytes","text:ins.text","text:loadError","text:m.name","text:r.name","text:r.version","text:rawAssemblyFlags(dotnetMeta.assembly.flags)","text:s.name","text:s.name","text:t.name","text:t.namespace || \"—\""],
  "CertPreview.svelte": ["text:cert.is_ca ? \"Yes\" : \"No\"","text:cert.issuer","text:cert.serial","text:cert.sha1_fingerprint","text:cert.sha256_fingerprint","text:cert.signature_algorithm","text:cert.subject","text:cert.version","text:copiedKey === \"sha1\" ? \"Copied\" : \"Copy\"","text:copiedKey === \"sha256\" ? \"Copied\" : \"Copy\"","text:csr.subject","text:data.encoding.toUpperCase()","text:data.error","text:eku","text:humanIso(cert.not_after)","text:humanIso(cert.not_before)","text:keyLabel(cert.public_key)","text:keyLabel(csr.public_key)","text:keyLabel(privKey)","text:keyLabel(pubKey)","text:ku","text:loadError","text:san","text:san","title:san","title:san"],
  "CommandBar.svelte": ["text:$t('cmd.ascending')","text:$t('cmd.descending')","text:$t('cmd.groupFolders')","text:$t('cmd.new')","text:$t('cmd.open')","text:$t('cmd.showHidden')","text:$t('cmd.sort')","text:$t('cmd.view')","text:$t('filter.' + f.key)","text:$t(s.labelKey)","text:$t(v.labelKey)","text:c.name","text:FILE_FILTERS.find((f) => f.key === fileFilter) ? $t('filter.' + fileFilter) : $t('cmd.filter')","title:`${$t('palette.ariaPalette')} (Ctrl+Shift+P)`","title:`${c.name} (user command)`","title:selectionCount > 1 ? \"Rename one item at a time\" : \"Rename (F2)\"","title:showDetails ? \"Hide details pane (Alt+P)\" : \"Show details pane (Alt+P)\"","title:showTerminal ? \"Hide terminal\" : \"Show terminal\""],
  "CompareDialog.svelte": ["text:(fileDiff.firstDiff ?? 0).toString(16).toUpperCase()","text:error","text:fileDiff.firstDiff","text:fileDiff.lengthDiffers ? \"differ\" : \"match\"","text:fileDiff.ranges.length","text:row.hasChildren ? (collapsed.has(row.path) ? \"▸\" : \"▾\") : \"\"","text:row.op === \"add\" ? \"+\" : row.op === \"del\" ? \"−\" : \" \"","text:row.text","text:STATUS_LABEL[row.node.status]","text:summary.added","text:summary.changed","text:summary.identical","text:summary.removed","text:textDiff.added","text:textDiff.removed"],
  "ContextMenu.svelte": ["text:$t('cmd.ascending')","text:$t('cmd.descending')","text:$t('ctx.addFavorite')","text:$t('ctx.archiveSafety')","text:$t('ctx.batchMedia')","text:$t('ctx.compareFiles')","text:$t('ctx.compressTarGz')","text:$t('ctx.compressWithPassword')","text:$t('ctx.compressZip')","text:$t('ctx.copy')","text:$t('ctx.copyAsPath')","text:$t('ctx.copyAsPath')","text:$t('ctx.copyAsPath')","text:$t('ctx.copyAsPath')","text:$t('ctx.copyName')","text:$t('ctx.copyToFolder')","text:$t('ctx.delete')","text:$t('ctx.duplicate')","text:$t('ctx.ejectDrive')","text:$t('ctx.execute')","text:$t('ctx.executeAdmin')","text:$t('ctx.extract')","text:$t('ctx.extractTo')","text:$t('ctx.folder')","text:$t('ctx.folder')","text:$t('ctx.folder')","text:$t('ctx.folder')","text:$t('ctx.invertSelection')","text:$t('ctx.moveToFolder')","text:$t('ctx.newLink')","text:$t('ctx.open')","text:$t('ctx.open')","text:$t('ctx.open')","text:$t('ctx.open')","text:$t('ctx.openInTerminal')","text:$t('ctx.openInTerminal')","text:$t('ctx.openInTerminal')","text:$t('ctx.openNewTab')","text:$t('ctx.openNewTab')","text:$t('ctx.openNewTab')","text:$t('ctx.paste')","text:$t('ctx.properties')","text:$t('ctx.properties')","text:$t('ctx.properties')","text:$t('ctx.properties')","text:$t('ctx.properties')","text:$t('ctx.refresh')","text:$t('ctx.rename')","text:$t('ctx.rename')","text:$t('ctx.repairLink')","text:$t('ctx.reveal')","text:$t('ctx.reveal')","text:$t('ctx.reveal')","text:$t('ctx.selectAll')","text:$t('ctx.selectAllExt', { ext: sameTypeExt })","text:$t('ctx.selectByPattern')","text:$t('ctx.shred')","text:$t('ctx.tags')","text:$t('ctx.textFile')","text:$t('ctx.textFile')","text:$t('ctx.textFile')","text:$t('ctx.textFile')","text:$t('ctx.undo')","text:$t('ctx.workOnFolder')","text:$t('ctx.workOnThis')","text:$t('home.clearAll')","text:$t('home.disconnectShare')","text:$t('home.pinToQuickAccess')","text:$t('home.removeFromFavorites')","text:$t('home.removeFromRecent')","text:$t('home.removeFromRecentFolders')","text:$t('home.removeNetworkLocation')","text:$t('palette.ariaPalette')","text:$t('sort.modified')","text:$t('sort.name')","text:$t('sort.size')","text:$t('sort.type')","text:$t('studio.menu')","text:$t('view.details')","text:$t('view.gallery')","text:$t('view.icons')","text:$t('view.list')","text:$t(ft.labelKey)","text:$t(ft.labelKey)","text:$t(ft.labelKey)","text:$t(ft.labelKey)","text:c.name","text:favorited ? $t('ctx.removeFavorite') : $t('ctx.addFavorite')","text:name","text:pinned ? $t('ctx.unpinFromHome') : $t('ctx.pinToHome')","text:undoLabel ? ` ${undoLabel}` : ''","title:selectionCount > 1 ? \"Rename one item at a time\" : \"Rename (F2)\""],
  "DataBrowser.svelte": ["text:c.name","text:cell","text:error","text:loading ? \"Loading…\" : \"\"","text:loading ? \"Loading…\" : \"No rows.\"","text:offset + 1","text:offset + page.rows.length","text:page.total","text:s","text:sortDir === 1 ? \"▲\" : \"▼\"","title:c.type || \"column\"","title:isSqlite ? \"Table / view\" : \"Sheet\""],
  "DocsView.svelte": ["@html:html","text:d.title","text:g.docs.length","text:g.name","title:expanded ? \"Collapse section\" : \"Expand section\""],
  "EmailPreview.svelte": ["text:data.attachments.length === 1 ? \"1 attachment\" : `${data.attachments.length} attachments`","text:data.body","text:data.cc.join(\", \")","text:data.error","text:data.from ?? \"—\"","text:data.subject ?? \"—\"","text:data.to.join(\", \")","text:dateText","text:formatSize(att.size)","text:loadError","title:`${displaySafeName(att.filename)} — ${att.content_type}`"],
  "FloatPreview.svelte": [],
  "FontPreview.svelte": ["aria-label:`Glyph ${codepointLabel(cp)}`","text:$t(\"pv.cantFont\")","text:codepointLabel(selectedGlyph)","text:format ?? formatLabelForExt(extension)","text:formatSize(size)","text:glyphChar(cp)","text:glyphChar(selectedGlyph)","text:glyphGrid.shown.length","text:glyphGrid.total","text:glyphGrid.total === 1 ? \"character\" : \"characters\"","text:glyphGrid.total.toLocaleString()","text:glyphGrid.total.toLocaleString()","text:metadata.family","text:metadata.numGlyphs.toLocaleString()","text:metadata.style","text:metadata.version","text:sampleText","title:codepointLabel(cp)"],
  "HexView.svelte": ["text:(pageOffset + bytes.length).toString(16).toUpperCase()","text:cursor.toString(16).toUpperCase()","text:error","text:hex2(b)","text:pageOffset.toString(16).toUpperCase()","text:row.ascii","text:row.offset","text:row.type","text:row.value","text:sig.ext","text:sig.name","text:size"],
  "IcalPreview.svelte": ["text:att","text:componentBadge(ev.component)","text:data.calendar_name","text:data.error","text:data.method","text:ev.attendees.length === 1 ? \"1 attendee\" : `${ev.attendees.length} attendees`","text:ev.description","text:ev.location","text:ev.organizer","text:ev.recurrence","text:ev.status","text:ev.summary ?? \"(no title)\"","text:loadError","text:whenText(ev)","title:att"],
  "Icon.svelte": [],
  "JwtPreview.svelte": ["text:data.alg ?? \"—\"","text:data.alg === \"none\" ? \"alg: none\" : \"empty or malformed\"","text:data.error","text:data.kid","text:data.signature_len === 1 ? \"byte\" : \"bytes\"","text:data.signature_len.toLocaleString()","text:data.typ ?? \"—\"","text:headerJson","text:human(data.exp.raw)","text:human(data.iat.raw)","text:human(data.nbf.raw)","text:loadError","text:payloadJson"],
  "LinkBadge.svelte": ["aria-label:title"],
  "LogPreview.svelte": ["text:formatSize(win.file_len)","text:formatSize(win.file_len)","text:formatSize(win.window_end - win.window_start)","text:LEVEL_LABEL[level]","text:line.index + 1","text:line.level ? LEVEL_LABEL[line.level] : \"\"","text:line.text","text:line.truncated ? \"…\" : \"\"","text:loadError","text:log.counts[level]","text:log.lines.length","text:log.lines.length === 1 ? \"\" : \"s\"","text:log.lines.length.toLocaleString()","text:log.totalLines.toLocaleString()","text:unleveledCount","text:visibleLines.length","text:win.file_len.toLocaleString()","text:win.file_len.toLocaleString()","text:win.window_end.toLocaleString()","text:win.window_end.toLocaleString()","text:win.window_start.toLocaleString()","text:win.window_start.toLocaleString()"],
  "MacroRunConfirm.svelte": ["text:inputs.length","text:inputs.length === 1 ? \"\" : \"s\"","text:macro.name","text:macro.name","text:macro.steps.length","text:macro.steps.length === 1 ? \"\" : \"s\"","text:op.detail","text:op.input","text:op.kind","text:planError","text:run.ops.length","text:run.ops.length === 1 ? \"\" : \"s\"","text:runError","text:running ? \"Running…\" : \"Run\"","text:undoError","text:undoing ? \"Undoing…\" : \"Undo\""],
  "MacrosDialog.svelte": ["text:error","text:m.name","text:m.steps","text:m.steps === 1 ? \"\" : \"s\"","text:macros.length","text:macros.length === 1 ? \"\" : \"s\"","text:note","text:STEP_LABEL[k]","text:STEP_LABEL[kindOf(step)]","title:m.name"],
  "MediaPlayer.svelte": ["aria-label:state.muted ? \"Unmute\" : \"Mute\"","aria-label:state.playing ? \"Pause\" : \"Play\"","text:mt.formatTime(state.currentTime)","text:mt.formatTime(state.duration)","text:state.rate","title:state.muted ? \"Unmute\" : \"Mute\"","title:state.playing ? \"Pause\" : \"Play\""],
  "MediaQuickLook.svelte": ["aria-label:repeatLabel","aria-label:shuffled ? \"Shuffle on\" : \"Shuffle off\"","text:count","text:position + 1","text:repeatLabel","text:shuffled ? \"on\" : \"off\""],
  "MenuBar.svelte": ["aria-label:$t(\"menu.language\")","aria-label:$t(menu.labelKey)","text:$locale === l.code ? \"✓\" : \"\"","text:$t(\"menu.language\")","text:$t(menu.labelKey)","text:cov === 0 ? \"English\" : `${Math.round(cov * 100)}%`","text:item.hint","text:item.label ?? (item.labelKey ? $t(item.labelKey) : \"\")","text:l.english","text:l.name","title:$t(\"menu.language\")","title:cov === 0 ? \"Not yet translated — shows in English\" : `${Math.round(cov * 100)}% translated — the rest shows in English`"],
  "MetadataStudioDialog.svelte": ["aria-label:$t(\"studio.revertFieldAria\", { field: f.key })","aria-label:$t(\"studio.title\")","text:$t(\"common.close\")","text:$t(\"studio.applyAll\", { n: files.length })","text:$t(\"studio.copyFromFirst\")","text:$t(\"studio.loading\")","text:$t(\"studio.noFile\")","text:$t(\"studio.noMeta\")","text:$t(\"studio.resetAll\")","text:$t(\"studio.stripEditable\")","text:$t(\"studio.title\")","text:$t(\"studio.viewOnly\")","text:currentValue(f, edited) || \"—\"","text:error","text:f.key","text:groupLabel(g)","text:notice","text:saving ? $t(\"studio.saving\") : $t(\"studio.save\")","title:$t(\"common.close\")","title:$t(\"studio.copyFromFirstHint\")","title:$t(\"studio.resetAllHint\")","title:$t(\"studio.revertFieldHint\")","title:$t(\"studio.stripEditableHint\")","title:writable ? $t(\"studio.fieldReadonly\") : $t(\"studio.viewOnly\")"],
  "NetworkConnectionForm.svelte": ["aria-label:editing ? `Edit connection ${editing.name}` : \"Add a connection\"","text:AUTH_LABELS[kind]","text:editing ? \"Save\" : \"Add\"","text:editing ? `Edit “${editing.name}”` : \"Add a connection\"","text:error","text:hints.hostLabel","text:hints.pathLabel","text:hints.userLabel","text:s"],
  "NetworkConnectionMenu.svelte": ["aria-label:`${name} actions`"],
  "NetworkSecretPrompt.svelte": ["aria-label:`${label} for ${name}`","aria-label:label","text:label","text:name"],
  "NotebookPreview.svelte": ["@html:cellHtml[cell.index] ?? \"\"","@html:cellHtml[cell.index] ?? \"\"","text:cell.executionCount != null ? `In [${cell.executionCount}]` : \"In [ ]\"","text:cell.outputs.length","text:cell.outputsTotal","text:cell.source","text:cell.type","text:loadError","text:notebook.cells.length","text:notebook.totalCells","text:output.ename","text:output.evalue","text:output.otherMimeTypes.join(\", \")","text:output.otherMimeTypes.length","text:output.text","text:output.text","text:output.traceback","text:parseError","text:RAW_FALLBACK_CHARS.toLocaleString()","text:rawFallback"],
  "OrganizeDialog.svelte": ["aria-label:$t(\"org.title\")","text:$t(\"common.cancel\")","text:$t(\"org.checkpointNote\", { label: outcome.checkpoint.checkpoint.label || outcome.checkpoint.checkpoint.manifest_id.slice(0, 12) })","text:$t(\"org.empty\")","text:$t(\"org.loading\")","text:$t(\"org.result\", { moved: movedCount, skipped: skippedCount })","text:$t(\"org.title\")","text:$t(\"org.undo\")","text:$t(\"org.willMove\", { count: plan.length, groups: groups.length })","text:$t(r.labelKey)","text:applying ? $t(\"org.applying\") : $t(\"org.apply\")","text:error","text:g.items.length","text:g.subdir"],
  "ScheduledSnapshots.svelte": ["text:error","text:key","text:rule.enabled ? \"on\" : \"paused\""],
  "SidecarManager.svelte": ["text:$t(\"mgr.\" + health.key)","text:$t(\"mgr.checking\")","text:$t(\"mgr.grant\")","text:$t(\"mgr.healthy\")","text:$t(\"mgr.noCapabilities\")","text:$t(\"mgr.noLogs\")","text:$t(\"mgr.none\")","text:$t(\"mgr.notRunning\")","text:$t(\"mgr.open\")","text:$t(\"mgr.repair\")","text:$t(\"mgr.repairDid\")","text:$t(\"mgr.stop\")","text:CAPABILITY_INFO[cap].label","text:diag.last_error","text:line.level","text:line.message","text:logsOpen[row.id] ? $t(\"mgr.hideLogs\") : $t(\"mgr.viewLogs\", { count: diag.logs.length })","text:repairMsg[row.id]","text:repairMsg[row.id]","text:row.compatible ? $t(\"mgr.contractOk\", { v: row.contract }) : $t(\"mgr.contractBad\", { v: row.contract })","text:row.enabled ? $t(\"mgr.enabled\") : $t(\"mgr.disabled\")","text:row.name","text:row.version","title:$t(\"mgr.contractTip\")","title:$t(\"mgr.grantTip\")","title:$t(\"mgr.lastError\")","title:$t(\"mgr.revoke\")","title:row.enabled ? $t(\"mgr.enabled\") : $t(\"mgr.disabled\")","title:row.running ? $t(\"mgr.running\") : $t(\"mgr.stopped\")"],
  "SmartFolderMenu.svelte": ["aria-label:$t(\"ctx.rename\")","aria-label:$t(\"smart.moveDown\")","aria-label:$t(\"smart.moveUp\")","text:$t(\"common.apply\")","text:$t(\"common.cancel\")","text:$t(\"menu.delete\")","text:name","title:$t(\"smart.moveDown\")","title:$t(\"smart.moveUp\")"],
  "SyncDialog.svelte": ["text:line","text:m < 60 ? `${m} min` : `${m / 60} h`","text:running ? \"Syncing…\" : \"Run sync\"","text:status?.branch ? `“${status.branch}”` : \"repository\"","text:status.ahead","text:status.behind","text:status.blocked","text:status.upstream","text:syncActionLabel(action)","text:w"],
  "TagEditor.svelte": ["aria-label:$t(\"tags.addLabel\")","aria-label:$t(\"tags.remove\")","aria-label:$t(\"tags.title\")","aria-label:$t(`tags.color.${key === \"\" ? \"none\" : key}`)","text:$t(\"status.items\", { count })","text:$t(\"tags.apply\")","text:$t(\"tags.cancel\")","text:$t(\"tags.colorLabel\")","text:$t(\"tags.none\")","text:$t(\"tags.pullNative\")","text:$t(\"tags.pushNative\")","text:$t(\"tags.title\")","text:nativeName","text:syncNote","text:tag","title:$t(\"tags.remove\")","title:$t(`tags.color.${key === \"\" ? \"none\" : key}`)"],
  "TemplatesDialog.svelte": ["text:error","text:note","text:t.dirs","text:t.files","text:t.name","text:templates.length","text:templates.length === 1 ? \"\" : \"s\"","title:path ? `Capture ${displaySafeName(base(path))}` : \"No folder\"","title:path ? `Stamp into ${displaySafeName(base(path))}` : \"No folder\"","title:t.name"],
  "ThumbnailImage.svelte": [],
  // CPE-1775 removed the `53:t.report.errors.join("\n")` entry: those reason lines start with an
  // ARCHIVE-CONTROLLED entry name and were the one genuine spoof surface in this file. They now render
  // through `displaySafePath` in a click-to-open list instead of a raw hover tooltip. What is left is
  // counts and literals (`whyLabel` builds "· N skipped — why?" from a number).
  "TransferPanel.svelte": ["text:label(t)","text:percent(t)","text:t.done_items","text:t.total_items","text:transferReasonsLabel(t.report)","title:label(t)"],
  "UserCommandsDialog.svelte": ["text:c.mode","text:c.name","text:c.template","text:s","text:s"],
  "VaultBadge.svelte": ["aria-label:title"],
  "VaultBanner.svelte": ["text:locking ? $t(\"vault.locking\") : $t(\"vault.lock\")","title:locking ? $t(\"vault.lockingTitle\") : $t(\"vault.lockTitle\")"],
  "VcardPreview.svelte": ["text:adr.label","text:card.birthday","text:data.cards.length","text:data.error","text:em.address","text:formatSize(card.photo_size)","text:heading(card)","text:loadError","text:subheading(card)","text:t","text:t","text:t","text:tel.number","text:url"],
  "WatchRulesDialog.svelte": ["text:actSummary(a)","text:condSummary(rule.when)","text:f","text:fire.summary","text:preview.actions.map((a) => a.resolved).join(\", \")","text:preview.rule.name","text:rule.actions.map(actSummary).join(\", \")","text:rule.name"],
  "WorkspacesDialog.svelte": ["text:w.name","text:w.tabs.length","text:w.tabs.length === 1 ? '' : 's'"],
  "YamlTomlPreview.svelte": ["text:format === \"yaml\" ? \"YAML\" : \"TOML\"","text:loadError","text:parseErrorMessage","text:parseErrorMessage","text:RAW_FALLBACK_CHARS.toLocaleString()","text:rawFallback"],

  // --- B1 (reviewer, round 2): 6 more candidates surfaced by the widened CANDIDATE_PATTERN ---
  "CardDetailDialog.svelte": ["@html:bodyHtml","text:bodyLines","text:bodyLines === 1 ? \"\" : \"s\"","text:detail?.location || \"Tickets\"","text:error","text:id","text:k","text:metaFields.length","text:metaFields.length === 1 ? \"\" : \"s\"","text:sending ? \"…\" : \"Send ▸\"","text:title","text:v","title:title"],
  "CreateCertDialog.svelte": ["aria-label:`Remove ${v}`","aria-label:`Remove ${v}`","text:busy ? \"Creating…\" : \"Create\"","text:error","text:kt.label","text:v","text:v"],
  "NewLinkDialog.svelte": ["aria-label:$t(\"link.newLinkTitle\")","text:$t(\"common.cancel\")","text:$t(\"link.browse\")","text:$t(\"link.create\")","text:$t(\"link.junctionTargetHint\")","text:$t(\"link.kindHardlink\")","text:$t(\"link.kindJunction\")","text:$t(\"link.kindLabel\")","text:$t(\"link.kindSymlink\")","text:$t(\"link.nameLabel\")","text:$t(\"link.newLinkTitle\")","text:$t(\"link.targetLabel\")","text:error"],
  "RepairLinkDialog.svelte": ["aria-label:$t(\"link.repairTitle\")","text:$t(\"common.cancel\")","text:$t(\"common.close\")","text:$t(\"link.repairAccept\")","text:$t(\"link.repairBrowse\")","text:$t(\"link.repairConfirmYes\")","text:$t(\"link.repairIntro\")","text:$t(\"link.repairLoading\")","text:$t(\"link.repairNoSuggestion\")","text:$t(\"link.repairSuggestionLabel\")","text:$t(\"link.repairTitle\")","text:error","text:translate($locale, \"link.repairConfirm\", { target: displaySafePath(chosenTarget ?? \"\") })"],
  "Spotlight.svelte": ["aria-label:$t(\"spotlight.ariaSearch\")","aria-label:$t(\"spotlight.title\")","text:$t(GROUP_LABEL[section.kind])","text:query.trim() ? $t(\"spotlight.noMatches\") : $t(\"spotlight.typeHint\")"],
  "WorkbenchView.svelte": ["text:branch || \"detached\"","text:branch || \"the working tree\"","text:copiedFile === key ? \"✓ Copied\" : \"Copy\"","text:error","text:fs.added","text:fs.removed","text:h.header","text:isCollapsed ? \"▸\" : \"▾\"","text:l.kind === \"add\" ? \"+\" : l.kind === \"del\" ? \"−\" : \" \"","text:l.newLine ?? \"\"","text:l.oldLine ?? \"\"","text:l.text","text:s.text","text:s.text","text:stats.added","text:stats.files","text:stats.files === 1 ? \"\" : \"s\"","text:stats.removed","title:isCollapsed ? \"Expand\" : \"Collapse\""],

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
  "StatusBar.svelte": ["text:advisoryAnnouncement","text:diskLabel","text:filteredHiddenText","text:git.ahead","text:git.behind","text:git.branch || \"detached\"","text:itemCount","text:itemCount","text:itemCount === 1 ? \"\" : \"s\"","text:selectedCount","text:selectedSize > 0 ? ` — ${formatSize(selectedSize)}` : \"\"","text:totalCount","text:unreadableText","title:filteredHiddenTitle","title:git.ahead","title:git.behind","title:git.upstream ? `Tracking ${git.upstream}` : \"No upstream branch\"","title:unreadableTitle"],

  // --- CPE-1798 sibling audit: AgentMenu's `sessionLabel` prop is built by its one real caller
  // (Sidebar.svelte:436, `${s.agentName || s.agentId || "Agent"}${model ? " · " + model : ""}`) from an
  // agent's own self-reported identity string, not static UI copy — the same shape-cleared-but-not-
  // runtime-cleared gap the ticket found for StatusBar. Escaped on arrival at both render positions
  // ("Open "/"Close " rows); `sessionNum(sessionId)` (a numeric chip label, unrelated to this ticket) and
  // `label` (verified static — every real caller passes either the literal default or `$t(...)`, see
  // Toolbar.svelte's equivalent, unlike `sessionLabel`) are left as accepted-but-unprovable entries below.
  "AgentMenu.svelte": ["text:label","text:sessionNum(sessionId)","text:sessionNum(sessionId)"],
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
  it("every registered component's raw-render set matches its recorded (kind, expression) pairs EXACTLY", () => {
    const mismatches: string[] = [];
    for (const [file, recorded] of Object.entries(REGISTRY)) {
      const src = readFileSync(join(COMPONENTS, file), "utf8");
      const sites = findUnsafeRenderSites(src, file);
      // CPE-1885 round 2: compare by (kind, expr) MULTISET, not by bare expression text — a pure
      // reformat that shifts every line but changes no expression/position must stay green (see the
      // header note above), AND a swap that fixes one occurrence of a duplicated expression while
      // introducing an unrelated new occurrence in a DIFFERENT position-kind must go red (round 1's
      // expression-only multiset could not see that; this is the fix).
      const foundKeys = siteKeyMultiset(sites);
      const recordedSorted = [...recorded].sort((a, b) => a.localeCompare(b));
      if (JSON.stringify(foundKeys) !== JSON.stringify(recordedSorted)) {
        const newlyRaw = multisetDiff(foundKeys, recordedSorted);
        const stale = multisetDiff(recordedSorted, foundKeys);
        // F5 (reviewer, CPE-1761 attempt 2): the useful delta goes FIRST — a developer reading a failed
        // registry file must see what actually changed before wading through the full recorded/found
        // dumps (which, for a file like AgentTimeline.svelte, run to several KB and bury the diff).
        // CPE-1885: name the component + kind:expression first, addresses last — most of a wall like
        // this used to be noise around a one-word fact. The line-numbered sites are still included at
        // the end so a real investigation can jump straight to the offending line.
        const foundDump = sites.map((s) => `${s.line}:${s.kind}:${s.expr}`).join(",");
        mismatches.push(
          `${file}:` +
            (newlyRaw.length ? ` NEW raw offender(s) (kind:expr): ${newlyRaw.join(",")}` : "") +
            (stale.length ? ` STALE recorded entry(ies), no longer rendered raw (kind:expr): ${stale.join(",")}` : "") +
            ` — full found (line:kind:expr) [${foundDump}] vs recorded (kind:expr) [${recordedSorted.join(",")}]`,
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
    expect(REGISTRY["PreviewPane.svelte"], "PreviewPane.svelte:1015 (a title= attribute) must currently record title:$t(action.labelKey) for this demonstration to mean anything").toContain("title:$t(action.labelKey)");

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
    // CPE-1885 round 2: the actual guard comparison is now by (kind, expr) MULTISET, so prove it fails
    // on those terms too — one `title:$t(action.labelKey)` occurrence traded for a new `title:entry.name`
    // occurrence changes the multiset even though the total offender COUNT is unchanged (this is exactly
    // the case a naive multiset over bare expression text alone — round 1's approach — would miss: same
    // size, different bag).
    const foundSites = findUnsafeRenderSites(mutated, "PreviewPane.svelte (mutated: raw name substituted on an already-recorded line)");
    const foundKeys = siteKeyMultiset(foundSites);
    const recordedKeys = [...REGISTRY["PreviewPane.svelte"]].sort((a, b) => a.localeCompare(b));
    expect(JSON.stringify(foundKeys)).not.toBe(JSON.stringify(recordedKeys));
  });

  // CPE-1885 round 2 (reviewer finding, attempt 1 CHANGES REQUESTED): keying REGISTRY by bare expression
  // text alone is a strict information loss versus the OLD "<line>:<expr>" keying whenever the SAME
  // expression appears more than once in a component — round 1's expression-only MULTISET can count
  // occurrences but cannot tell WHICH occurrence is which. Reconstructed here literally, on the exact
  // shape the reviewer demonstrated: SplitFileDialog.svelte genuinely renders `baseName(path)` raw at
  // TWO real text-node positions (lines 101 and 114 — see REGISTRY's `"text:baseName(path)"` entry,
  // recorded twice). Wrap ONE of those two occurrences (a real fix) while introducing an unrelated BRAND
  // NEW raw occurrence of the IDENTICAL expression text in a DIFFERENT position-kind (a `title=`
  // attribute) — net expression COUNT is unchanged, so an expression-only multiset cannot see the swap:
  // the unsafe surface silently moved from a reviewed text node to a brand-new, unreviewed tooltip.
  it("CPE-1885 round 2 red-proof: a position-kind swap at constant expression count — green under expression-only keying, red under (kind, expr) keying", () => {
    const src = readFileSync(join(COMPONENTS, "SplitFileDialog.svelte"), "utf8");
    const ORIGINAL_TEXT_OCCURRENCE = "Split <strong>{baseName(path)}</strong> into";
    const WRAPPED_TEXT_OCCURRENCE = "Split <strong>{displaySafeName(baseName(path))}</strong> into";
    const ANCHOR = '<div class="summary" data-testid="split-summary">';
    const NEW_TITLE_OCCURRENCE = `<span class="src-hint" title={baseName(path)}>source</span>\n      ${ANCHOR}`;
    // Guard the fixture itself, same discipline as every other substitution demo in this file: if
    // SplitFileDialog.svelte changes shape, this must fail loudly rather than silently prove nothing.
    expect(src, "SplitFileDialog.svelte no longer contains the exact text this demonstration substitutes — update the fixture").toContain(ORIGINAL_TEXT_OCCURRENCE);
    expect(src, "SplitFileDialog.svelte no longer contains the exact anchor this demonstration inserts before — update the fixture").toContain(ANCHOR);
    expect(REGISTRY["SplitFileDialog.svelte"].filter((k) => k === "text:baseName(path)"), "SplitFileDialog.svelte must currently record baseName(path) raw at exactly two TEXT positions for this demonstration to mean anything").toHaveLength(2);

    let mutated = src.replace(ORIGINAL_TEXT_OCCURRENCE, WRAPPED_TEXT_OCCURRENCE); // fix one occurrence
    mutated = mutated.replace(ANCHOR, NEW_TITLE_OCCURRENCE); // introduce an unrelated new one, different kind
    const label = "SplitFileDialog.svelte (mutated: one text occurrence wrapped, one new title occurrence added)";
    const foundSites = findUnsafeRenderSites(mutated, label);

    // BEFORE (round 1's design, reconstructed literally — not resurrected as live code — purely to prove
    // what it would have missed): an expression-only multiset sees "baseName(path)" exactly twice before
    // AND after the swap (one text occurrence traded for one title occurrence, same total count), so it
    // reports NO difference at all — this is the reviewer's `exprMultiset(foundBefore) ===
    // exprMultiset(foundAfter)` finding, reproduced against the real engine's real output rather than a
    // hand-built fixture.
    const exprOnlyBefore = REGISTRY["SplitFileDialog.svelte"].map((k) => k.replace(/^[^:]*:/, "")).sort((a, b) => a.localeCompare(b));
    const exprOnlyAfter = foundSites.map((s) => s.expr).sort((a, b) => a.localeCompare(b));
    expect(
      JSON.stringify(exprOnlyAfter),
      "the expression-only view must show NO difference across the swap — this is the false negative round 1 shipped",
    ).toBe(JSON.stringify(exprOnlyBefore));

    // AFTER (this fix): the SAME swap, viewed through (kind, expr) — the actual deployed comparison —
    // shows the real difference: one "text:baseName(path)" occurrence is gone (fixed) and a brand-new
    // "title:baseName(path)" occurrence exists that REGISTRY never recorded.
    const foundKeys = siteKeyMultiset(foundSites);
    const recordedKeys = [...REGISTRY["SplitFileDialog.svelte"]].sort((a, b) => a.localeCompare(b));
    expect(JSON.stringify(foundKeys), "the (kind, expr) view must catch the swap the expression-only view above missed").not.toBe(JSON.stringify(recordedKeys));
    expect(multisetDiff(foundKeys, recordedKeys), "the swap must surface as a brand-new title:baseName(path) occurrence").toContain("title:baseName(path)");
    expect(multisetDiff(recordedKeys, foundKeys), "the swap must ALSO surface as one text:baseName(path) occurrence going stale (the one that got fixed)").toContain("text:baseName(path)");
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
    const foundSites = findUnsafeRenderSites(mutated, "ConfirmDialog.svelte (mutated: displaySafeName wrap dropped)");
    const foundKeys = siteKeyMultiset(foundSites);
    const recordedKeys = [...REGISTRY["ConfirmDialog.svelte"]].sort((a, b) => a.localeCompare(b));

    expect(found, "the un-escaped message render must be flagged").toContain("38:message");
    expect(JSON.stringify(foundKeys), "the mutated file's (kind, expr) multiset must no longer equal what's recorded — this is what makes the real REGISTRY-equality test above red").not.toBe(
      JSON.stringify(recordedKeys),
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
    const foundSites = findUnsafeRenderSites(mutated, "StatusBar.svelte (mutated: displaySafeName wraps dropped)");
    const foundKeys = siteKeyMultiset(foundSites);
    const recordedKeys = [...REGISTRY["StatusBar.svelte"]].sort((a, b) => a.localeCompare(b));

    // Both the text-content render and the title= render sit on the same line with the same expression
    // (`notice`), so findUnsafeRenderLines' offender Set records ONE line:expr entry covering both —
    // still proof both are flagged, since a Set entry can only exist if at least one render position
    // resolved unsafe, and this file's markup has no OTHER bare `{notice}`/`title={notice}` anywhere.
    // CPE-1885 round 2: `findUnsafeRenderSites` does NOT collapse this pair — it reports two distinct
    // occurrences, `text:notice` and `title:notice` — proving both render positions independently, not
    // merely inferring "at least one" the way the line-based `found` check above has to.
    expect(found.some((e) => e.endsWith(":notice")), "the un-escaped notice render (text + title, same line) must be flagged").toBe(true);
    expect(foundKeys, "both the text and title occurrences of notice must be independently flagged as distinct (kind, expr) sites").toEqual(
      expect.arrayContaining(["text:notice", "title:notice"]),
    );
    expect(JSON.stringify(foundKeys), "the mutated file's (kind, expr) multiset must no longer equal what's recorded — this is what makes the real REGISTRY-equality test above red").not.toBe(
      JSON.stringify(recordedKeys),
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
    const foundSites = findUnsafeRenderSites(mutated, "AgentMenu.svelte (mutated: displaySafeName wrap dropped)");
    const foundKeys = siteKeyMultiset(foundSites);
    const recordedKeys = [...REGISTRY["AgentMenu.svelte"]].sort((a, b) => a.localeCompare(b));

    expect(found.some((e) => e.includes("sessionLabel")), "the un-escaped sessionLabel render must be flagged").toBe(true);
    expect(JSON.stringify(foundKeys), "the mutated file's (kind, expr) multiset must no longer equal what's recorded — this is what makes the real REGISTRY-equality test above red").not.toBe(
      JSON.stringify(recordedKeys),
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
