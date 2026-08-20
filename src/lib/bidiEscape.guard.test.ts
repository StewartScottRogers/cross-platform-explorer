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
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { findUnsafeRenderLines, compareOffenders, isCandidateComponent } from "./bidiRenderScan";

const COMPONENTS = join(process.cwd(), "src", "lib", "components");
const APP = join(process.cwd(), "src", "App.svelte");
const DOC = join(process.cwd(), "src", "docs", "03-explorer.md");

/** file -> the EXACT sorted line numbers `findUnsafeRenderLines` currently reports for it. Recomputed
 *  live every run and checked for equality (not "offenders minus this array must be empty") — see the
 *  header above for why that specific shape closes round 1's inert-allowlist hole. A non-empty array is
 *  NOT necessarily a disclosed spoof risk: most entries here are UI text this engine can't prove safe
 *  (i18n params, counts, labels, diagnostic error/note/reason strings, diff/metadata CONTENT, macro/
 *  workspace/rule/ticket/agent identity strings) — read `bidiEscape.doc-parity` below for which specific
 *  files' entries are an actual disclosed filesystem-name/path gap vs. harmless-but-unprovable text. */
const REGISTRY: Record<string, string[]> = {
  "ConflictDialog.svelte": ["108:opLabel ? `— ${opLabel}` : \"\"","117:unresolved","121:f.label","125:opLabel.toLowerCase()","136:showBase ? \"Hide\" : \"Show\"","145:versions.base ?? \"— absent —\"","146:versions.ours ?? \"— absent —\"","147:versions.theirs ?? \"— absent —\"","163:error || note || `${opLabel || \"No\"} operation in progress`","171:unresolved > 0 ? \"Resolve every file first\" : `Continue the ${opLabel.toLowerCase()}`","172:opLabel.toLowerCase()"],
  "FileNameSearchDialog.svelte": ["96:$t(\"search.findByNameTitle\")","98:$t(\"search.docsTitle\")","99:$t(\"common.close\")","116:$t(\"search.button\")","121:$t(\"search.searching\")","123:error","125:$t(\"search.noNameMatches\")","128:hits.length === 1 ? $t(\"search.matchOne\", { count: hits.length }) : $t(\"search.matchMany\", { count: hits.length })","129:$t(\"search.truncated\")"],
  "RepoBrowser.svelte": ["287:isGeneric ? \"Git URL\" : \"Repository\"","296:isGeneric ? \"https only\" : \"private repos\"","301:loading ? \"Browsing…\" : \"Browse\"","305:cloning ? \"Cloning…\" : \"Clone\"","310:provider","319:statusText","324:consent.host","336:repo","364:fmtSize(e.size)","373:loaded ? repo : \"No repository open\""],
  "AgentTimeline.svelte": ["604:agentName","605:entries.length","665:diff ? `${displaySafePath(e.path)} — hover to see what changed` : displaySafePath(e.path)","670:KIND_LABEL[e.kind]","672:stats.add","672:stats.del","673:clock(e.at)","710:playing ? \"Pause\" : \"Play\"","713:playing ? \"Pause\" : \"Play\"","734:s","760:cpMarkerTitle(m)","761:`Checkpoint ${m.cp.label || cpShortId(m.cp.manifest_id)}`","769:Math.round(sliderFraction(range, t) * 100)","769:new Date(t).toLocaleTimeString()","781:selectedCheckpoint.manifest_id","782:selectedCheckpoint.label || cpShortId(selectedCheckpoint.manifest_id)","783:cpTime(selectedCheckpoint.ts)","789:revertPreviewError","793:revertPreview.creates","794:revertPreview.overwrites","795:revertPreview.deletes","796:formatBytes(revertPreview.bytes_written)","798:revertPreview.drift_count","802:revertPreview.drift_count","802:revertPreview.drift_count === 1 ? \"\" : \"s\"","813:revertOutcome.applied","813:revertOutcome.applied === 1 ? \"\" : \"s\"","813:revertOutcome.skipped.length","816:revertError","822:selectedCheckpoint.label || cpShortId(selectedCheckpoint.manifest_id)","826:revertPreview.drift_count","826:revertPreview.drift_count === 1 ? \"\" : \"s\"","852:KIND_LABEL[replayCurrent.kind]","854:clock(replayCurrent.at)","905:replayKindLabel(re.kind)","906:clock(re.ts)","920:KIND_LABEL[e.kind]","922:clock(e.at)","943:c.sessionId","946:formatTokens(c.inputTokens)","947:formatTokens(c.outputTokens)","948:formatTokens(c.totalTokens)","949:formatUsd(c.costUsd)","951:formatTokens(c.filesTouched)","952:formatTokens(c.editCount)","953:formatBytes(c.churnBytes)","954:formatDuration(c.wallClockMs)","958:formatPerMinute(c.tokensPerMinute)","961:formatUsd(c.usdPerFile)","964:formatBytes(c.churnPer1kTokens)","988:relativeLabel(o.lastAt, Date.now())","992:friendlyActor(a, sessions)","1015:rc.kind === \"divergence\" ? \"diverged\" : \"collided\"","1017:relativeLabel(rc.lastAt, Date.now())","1021:friendlyActor(a, sessions)","1024:renameConflictNote(rc.kind)","1037:historyError","1047:formatTokens(historyRollup.totals.sessions)","1048:formatUsd(historyRollup.totals.costUsd)","1049:formatTokens(historyRollup.totals.totalTokens)","1050:formatDuration(historyRollup.totals.wallClockMs)","1051:formatTokens(historyRollup.totals.filesTouched)","1052:formatBytes(historyRollup.totals.churnBytes)","1061:historyRollup.totals.sessions","1061:historyRollup.totals.sessions === 1 ? \"\" : \"s\"","1061:historyUnclean","1062:historyUnclean === 1 ? \"its\" : \"their\"","1063:historyUnclean === 1 ? \"is\" : \"are\"","1064:historyUnclean === 1 ? \"it\" : \"them\"","1073:formatPerMinute(historyRollup.ratios.tokensPerMinute)","1076:formatUsd(historyRollup.ratios.usdPerSession)","1079:formatUsd(historyRollup.ratios.usdPerFile)","1082:formatBytes(historyRollup.ratios.churnPer1kTokens)","1096:row.model","1097:formatTokens(row.sessions)","1098:formatTokens(row.totalTokens)","1099:formatUsd(row.costUsd)","1100:historyShare(row.costUsd, historyRollup.totals.costUsd)","1116:row.agentName","1117:formatTokens(row.sessions)","1118:formatTokens(row.totalTokens)","1119:formatUsd(row.costUsd)","1120:historyShare(row.costUsd, historyRollup.totals.costUsd)","1141:new Date(row.startedAt).toLocaleString()","1142:row.agentName || row.agentId","1142:row.agentName || row.agentId || \"(unknown)\"","1143:historyDurationLabel(row)","1151:isSessionEndedCleanly(row) ? \"Clean\" : \"Ended unexpectedly\"","1185:historyMetric === 'cost' ? 'Cost' : 'Tokens'","1197:historyBarDate(p.bucketStart)","1197:historyMetric === \"cost\" ? formatUsd(v) : formatTokens(v)"],
  "ConsultedFiles.svelte": ["31:$agentConsulted.length","40:e.count"],
  "SessionHistoryDialog.svelte": ["101:s","109:k","116:error","123:formatDate(e.ts)","124:e.kind","132:filtered.length","132:filtered.length === 1 ? \"\" : \"s\""],
  "IntegrityDialog.svelte": ["84:hasBaseline ? `Baseline: ${baseline.length} files` : \"No baseline stored\"","85:note","90:error","97:report.corrupted.length","98:report.missing.length","99:report.edited.length","100:report.new.length","101:report.intact.length","107:label","107:list.length","115:report.intact.length"],
  "CheckpointDialog.svelte": ["217:error","218:note","233:cp.label || shortId(cp.manifest_id)","234:fmtTime(cp.ts)","234:shortId(cp.manifest_id)","248:$t('ckpt.failedTitle')","248:cf.reason","251:$t(\"ckpt.failedTitle\")","252:cf.operation","252:fmtTime(cf.ts)","253:cf.reason","274:preview.creates","275:preview.overwrites","276:preview.deletes","277:fmtBytes(preview.bytes_written)","279:preview.drift_count","288:diffOpenPath === p ? \"Close diff\" : \"Open diff\"","295:diffError","310:outcome.applied","310:outcome.applied === 1 ? \"\" : \"s\"","311:outcome.skipped.length","320:selected.label || shortId(selected.manifest_id)","323:selected.label || shortId(selected.manifest_id)"],
  "DiffSideBySide.svelte": ["38:r.left ?? \"\"","39:r.right ?? \"\""],
  "InspectCryptoDialog.svelte": [],
  "BoardView.svelte": ["265:error","278:boardQuery.trim()","283:boardQuery.trim()","297:col","298:list.length","301:showArchived ? \"hide\" : `+${archivedEpicList.length} archived`","311:\"Open \" + e.id + \" — details\"","313:e.id","314:\"Copy \" + e.id","315:copiedId === e.id ? \"✓\" : \"⧉\"","316:e.status","318:e.title","321:bar.state === \"empty\" ? \"No sub-tickets yet\" : bar.state === \"complete\" && p.total === 0 ? \"Epic complete\" : p.done + \" of \" + p.total + \" tickets done\"","328:bar.label","350:col","351:list.length","354:showArchived ? \"hide\" : `+${archived.length} archived`","363:\"Open \" + c.id + \" — details\"","365:c.id","366:\"Copy \" + c.id","367:copiedId === c.id ? \"✓\" : \"⧉\"","368:c.priority","370:c.title","373:c.epic","374:c.sprint","375:t","394:grouped[l].length","394:l","396:error || note || \"\""],
  "CopilotDialog.svelte": ["196:planError","199:phase === \"planning\" ? \"Planning…\" : \"Plan\"","206:instruction","213:v","219:planResult.summary.moves","219:planResult.summary.moves === 1 ? \"\" : \"s\"","220:planResult.summary.renames","220:planResult.summary.renames === 1 ? \"\" : \"s\"","221:planResult.summary.deletes","221:planResult.summary.deletes === 1 ? \"\" : \"s\"","222:planResult.summary.mkdirs","222:planResult.summary.mkdirs === 1 ? \"\" : \"s\"","223:planResult.summary.copies","232:opKind(op)","243:execError","267:v","273:execResult.results.filter((r) => r.ok).length","273:execResult.results.length","280:r.error","287:execResult.checkpoint.checkpoint.manifest_id","292:undoing ? \"Undoing…\" : \"Undo\"","295:undoError","298:undoOutcome.applied","298:undoOutcome.applied === 1 ? \"\" : \"s\"","299:undoOutcome.skipped.length"],

  // --- B4: the 19 components CPE-1712 itself originally escaped ---------------------------------
  "FileList.svelte": ["646:$t(\"fl.columnsButton\")","647:$t(\"fl.columnsButton\")","656:$t(\"fl.sortBy\", { col: $t(col.labelKey) })","658:$t(col.labelKey)","672:$t(\"fl.sortBy\", { col: ac.col.label })","674:ac.col.label","691:$t(\"fl.resizeColumn\", { col: handleLabel(i) })","694:$t(\"fl.resizeTip\")","705:error","708:$t(\"fl.loading\")","713:searching ? $t(\"fl.noMatch\") : $t(\"fl.empty\")","779:tagEntry.label","795:ruleStyle.label","800:tag","817:$t(ACTIVITY_LABEL_KEY[act.kind])","819:$t(\"fl.agentInside\")","824:formatDate(entry.modified)","825:typeName(entry)","828:folderSizes.has(entry.path) ? formatSize(folderSizes.get(entry.path) ?? 0) : \"…\"","830:formatSize(entry.size)","845:cell.display","863:$t(\"fl.agentLegend\")","867:friendlyActor(a, sessions)"],
  "Sidebar.svelte": ["415:agentsOpen ? \"Collapse\" : \"Expand\"","419:$t(\"sidebar.agents\")","428:`${s.agentName}${s.provider ? \" · \" + s.provider : \"\"}${s.model ? \" · \" + s.model : \"\"} · ${s.cwd} (double-click to open its tab · right-click for more)`","440:sessionNum(s.sessionId)","442:model","442:s.agentName || s.agentId || \"Agent\"","443:baseName(s.cwd)","471:favOpen ? \"Collapse\" : \"Expand\"","515:tagsOpen ? \"Collapse\" : \"Expand\"","527:`${count} item${count === 1 ? \"\" : \"s\"} tagged “${tag}” — click to filter, right-click to rename/delete`","533:tag","534:count","561:smartOpen ? \"Collapse\" : \"Expand\"","565:$t(\"smart.section\")","573:$t(\"smart.itemTip\", { tag: sf.tag })","579:sf.name","606:savedSearchOpen ? \"Collapse\" : \"Expand\"","610:$t(\"smart.searchSection\")","618:$t(\"smart.searchItemTip\")","624:ss.name","650:exploreOpen ? \"Collapse\" : \"Expand\"","654:$t(\"sidebar.explore\")","670:$t(\"sidebar.repositories\")","706:placesOpen ? \"Collapse\" : \"Expand\"","710:$t(\"sidebar.quickAccess\")","738:drivesOpen ? \"Collapse\" : \"Expand\"","742:$t(\"sidebar.drives\")","771:open ? \"Collapse\" : \"Expand\"","799:`${formatSize(u.free)} free of ${formatSize(u.total)}`","803:formatSize(u.free)","864:networkOpen ? \"Collapse\" : \"Expand\"","884:`${conn.scheme}://${conn.host} — ${stateTitle(state, connectionErrors[conn.name])} (right-click for more)`","919:savable ? `${displaySafePath(s.path)} — discovered on your network; click to add it as a connection` : `${displaySafePath(s.path)} — discovered on your network; ${prefill.scheme.toUpperCase()} isn't supported yet`","958:trashOpen ? \"Collapse\" : \"Expand\"","962:$t(\"sidebar.trash\")","969:$t(\"trash.openTip\")","974:$t(\"trash.open\")","981:$t(\"trash.macMessage\")","984:$t(\"trash.macLabel\")"],
  "TabBar.svelte": ["40:$t(\"app.closeTab\")","49:$t(\"app.newTab\")"],
  "HomeView.svelte": ["126:quickOpen ? $t(\"home.collapse\") : $t(\"home.expand\")","131:$t(\"home.quickAccess\")","163:$t(\"home.unpinQuick\")","180:recentOpen ? $t(\"home.collapse\") : $t(\"home.expand\")","185:tab === \"favorites\" ? $t(\"home.favorites\") : tab === \"folders\" ? $t(\"home.recentFolders\") : tab === \"shared\" ? $t(\"home.shared\") : $t(\"home.recent\")","187:$t(\"home.clear\")","190:$t(\"home.addNetworkLocation\")","197:$t(\"home.recent\")","200:$t(\"home.favorites\")","203:$t(\"home.folders\")","206:$t(\"home.shared\")","214:$t(\"home.noRecent\")","215:$t(\"home.noRecentSub\")","220:$t(\"home.dateOpened\")","220:$t(\"home.name\")","233:formatDate(r.opened)","239:$t(\"home.removeFromRecent\")","240:$t(\"home.removeFromRecent\")","253:$t(\"home.noFavorites\")","254:$t(\"home.noFavoritesSub\")","275:$t(\"home.removeFromFavorites\")","288:$t(\"home.noRecentFolders\")","289:$t(\"home.noRecentFoldersSub\")","310:$t(\"home.removeFromRecentFolders\")","311:$t(\"home.removeFromRecentFolders\")","335:$t(\"home.add\")","336:$t(\"common.cancel\")","342:$t(\"home.sharedLoading\")","347:$t(\"home.noShared\")","348:$t(\"home.noSharedSub\")","370:$t(\"home.removeNetworkLocation\")","371:$t(\"home.removeNetworkLocation\")"],
  "DetailsPane.svelte": ["28:typeName(one)","31:formatSize(one.size) || \"0 B\"","36:formatDate(one.modified) || \"—\"","42:selected.length","46:selected.filter((e) => e.is_dir).length","50:selected.filter((e) => !e.is_dir).length","54:formatSize(totalSize) || \"0 B\"","63:itemCount","63:itemCount === 1 ? \"\" : \"s\""],
  "TrashView.svelte": ["149:$t(\"trash.title\")","150:itemCountLabel","150:selectedCountLabel","155:allSelected ? $t(\"trash.deselectAll\") : $t(\"trash.selectAll\")","158:$t(\"trash.restoreSelected\")","161:$t(\"trash.emptySelected\")","164:$t(\"trash.emptyAll\")","167:$t(\"trash.refresh\")","178:$t(\"trash.restoreFailed\", { name: displaySafeName(f.name), error: f.error })","186:$t(\"trash.loading\")","188:$t(\"trash.error\", { error })","190:$t(\"trash.empty\")","194:$t(\"trash.selectAll\")","196:$t(\"trash.columnsName\")","197:$t(\"trash.columnsOriginalPath\")","198:$t(\"trash.columnsDeleted\")","215:formatSize(e.size)","218:formatDate(e.time_deleted * 1000)","228:$t(\"trash.emptyConfirmTitle\")"],
  "NavToolbar.svelte": ["85:$t('nav.back')","88:$t('nav.forward')","91:$t('nav.up')","94:$t('nav.refresh')","114:density === \"compact\" ? \"Switch to comfortable density\" : \"Switch to compact density\"","115:density === \"compact\" ? \"Switch to comfortable density\" : \"Switch to compact density\"","169:$t('nav.search')","169:searchScope","170:$t(\"nav.searchHint\")"],
  "PropertiesDialog.svelte": ["220:$t(\"prop.title\")","221:$t(\"common.close\")","225:error","232:$t(\"prop.type\")","232:typeName(single)","233:$t(\"prop.location\")","236:$t(\"prop.size\")","238:$t(\"prop.calculating\")","239:$t(\"prop.sizeBytes\", { size: formatSize(folderSize) || \"0 B\", bytes: folderSize.toLocaleString() })","240:$t(\"prop.unavailable\")","245:$t(\"prop.size\")","246:$t(\"prop.sizeBytes\", { size: formatSize(single.size) || \"0 B\", bytes: single.size.toLocaleString() })","250:$t(\"prop.created\")","250:formatDate(info.created) || \"—\"","251:$t(\"prop.modified\")","251:formatDate(info.modified) || \"—\"","253:$t(\"prop.attributes\")","255:[info.readonly ? $t(\"prop.readonly\") : null, info.hidden ? $t(\"prop.hidden\") : null] .filter(Boolean) .join(\", \") || $t(\"prop.none\")","263:label","263:value","268:label","268:value","273:$t(\"prop.typeMismatch\")","274:inspection.type_mismatch","282:checksum","283:$t(\"prop.copyChecksumTip\")","285:copied ? $t(\"prop.copied\") : $t(\"prop.copy\")","296:$t(\"prop.match\")","296:$t(\"prop.matchTip\")","298:$t(\"prop.noMatch\")","298:$t(\"prop.noMatchTip\")","302:$t(\"prop.computing\")","304:hashError","306:$t(\"prop.compute\")","313:$t(\"prop.contents\")","316:$t(\"prop.contentStats\", { lines: stats.lines.toLocaleString(), words: stats.words.toLocaleString(), chars: stats.chars.toLocaleString() })","318:$t(\"prop.counting\")","320:statError","322:$t(\"prop.count\")","331:$t(\"prop.itemsSelected\", { count: entries.length })","334:$t(\"prop.folders\")","334:folderCount","335:$t(\"prop.files\")","335:fileCount","337:$t(\"prop.sizeOfFiles\")","338:$t(\"prop.sizeBytes\", { size: formatSize(totalSize) || \"0 B\", bytes: totalSize.toLocaleString() })","341:$t(\"prop.folderNote\")","341:$t(\"prop.note\")","349:nativeStoreName","355:tag","364:nativeEntry.label || \"None\"","368:nativePulling ? \"Pulling…\" : \"Pull\"","370:nativeError","376:$t(\"common.close\")"],
  "InstantSearch.svelte": ["165:$t(\"search.instantTitle\")","167:$t(\"search.instantTitle\")","168:$t(\"search.docsTitle\")","169:$t(\"common.close\")","182:$t(\"search.instantPlaceholder\")","188:$t(\"search.instantOffTitle\")","189:$t(\"search.instantOffBody\")","191:$t(\"search.buildingIndex\", { count: buildStats?.dirs_scanned ?? 0 })","193:buildError","196:$t(\"search.buildIndex\")","198:$t(\"search.instantOpenFolderFirst\")","201:$t(\"search.searching\")","203:error","205:$t(\"search.instantTypeHint\")","207:$t(\"search.instantNoMatches\")"],
  "ArchiveSafetyDialog.svelte": ["75:$t(\"arcsafe.title\")","78:$t(\"arcsafe.title\")","80:$t(\"common.close\")","86:$t(\"arcsafe.scanning\")","88:error","89:$t(\"arcsafe.retry\")","96:$t(\"arcsafe.unreadable\")","107:$t(\"arcsafe.encrypted\", { count: result.unreadable_entries })","115:$t(\"arcsafe.dangerous\")","121:$t(\"arcsafe.encrypted\", { count: result.unreadable_entries })","126:$t(\"arcsafe.safe\")","131:$t(\"arcsafe.ratio\")","132:ratioLabel(result.report.overall_ratio)","133:$t(\"arcsafe.sizes\")","134:sizeLabel(result.report.total_compressed)","134:sizeLabel(result.report.total_uncompressed)","135:$t(\"arcsafe.entries\")","137:result.entries_scanned.toLocaleString()","138:$t(\"arcsafe.capped\")","141:$t(\"arcsafe.unreadableEntries\")","142:result.unreadable_entries.toLocaleString()","147:$t(\"arcsafe.flaggedHead\", { count: result.report.flagged.length })","155:ratioLabel(f.ratio)","160:$t(\"arcsafe.noneFlagged\")"],
  "PreviewPane.svelte": ["1015:$t(action.labelKey)","1017:$t(action.labelKey)","1024:actionMessage","1031:$t(\"pv.model.title\")","1033:$t(\"pv.model.format\")","1033:modelFormatLabel","1036:$t(\"pv.model.encoding\")","1037:modelInfo.ascii ? $t(\"pv.model.ascii\") : $t(\"pv.model.binary\")","1041:$t(\"pv.model.meshes\")","1041:modelInfo.mesh_count.toLocaleString()","1043:modelCountLabel","1043:modelInfo.triangle_count.toLocaleString()","1044:$t(\"pv.model.vertices\")","1044:modelInfo.vertex_count.toLocaleString()","1048:$t(\"pv.model.dimensions\")","1049:fmtDim(modelDims.d)","1049:fmtDim(modelDims.h)","1049:fmtDim(modelDims.w)","1057:$t(\"pv.dicom.title\")","1060:name","1060:value","1069:$t(\"pv.loading\")","1071:$t(\"pv.cantImage\")","1077:$t(\"pv.loading\")","1085:$t(\"pv.loading\")","1093:$t(\"pv.loading\")","1113:$t(\"pv.loading\")","1136:$t(\"pv.loading\")","1138:$t(\"pv.cantArchive\")","1146:e.is_dir ? \"\" : formatSize(e.size)","1151:entries.length === 1 ? $t(\"pv.itemOne\", { count: entries.length }) : $t(\"pv.itemMany\", { count: entries.length })","1156:$t(\"pv.loading\")","1158:$t(\"pv.cantFile\")","1160:info","1202:$t(\"pv.loading\")","1204:$t(\"pv.cantFile\")","1208:saving ? $t(\"pv.saving\") : $t(\"pv.save\")","1209:$t(\"common.cancel\")","1210:saveError","1229:$t(\"pv.json.viewTree\")","1234:$t(\"pv.json.viewRaw\")","1239:$t(\"pv.edit\")","1247:cell","1252:$t(\"pv.showingRows\", { cap: CSV_ROW_CAP, total: tableRows.length })","1263:prettyJson(text)","1267:mdHtml","1276:breadcrumbSym.name","1285:`Jump to ${sym.kind} ${sym.name}, line ${sym.line}`","1286:`${sym.name} — line ${sym.line}`","1290:sym.name","1306:foldCollapsed.has(i + 1) ? `Expand lines ${i + 1}–${foldByStart.get(i + 1)?.end_line}` : `Collapse lines ${i + 1}–${foldByStart.get(i + 1)?.end_line}`","1306:foldLen(i + 1)","1306:line","1334:$t(\"menu.cut\")","1335:$t(\"menu.copy\")","1336:$t(\"menu.paste\")","1338:$t(\"ctx.selectAll\")"],
  "QuickLook.svelte": ["33:images.length","33:index + 1"],
  "DiskSpaceView.svelte": ["147:formatSize(total)","147:loading ? \" · scanning…\" : \"\"","157:error","172:formatSize(c?.size ?? 0)","172:pct(c?.size ?? 0)","182:formatSize(c?.size ?? 0)","202:formatSize(c.size)"],
  "DropStackPanel.svelte": ["44:open ? \"Hide Drop Stack\" : \"Show Drop Stack\"","51:$dropStackEntries.length","77:canTransfer ? \"Move every shelved item into the current folder\" : \"Not a valid destination for the Drop Stack\"","85:canTransfer ? \"Copy every shelved item into the current folder\" : \"Not a valid destination for the Drop Stack\""],
  "FolderBrowser.svelte": ["121:$t(\"pv.loading\")","123:$t(\"pv.folder.cantOpen\")","125:$t(\"fl.empty\")","140:formatSize(entry.size)"],
  "SidebarNode.svelte": ["42:open ? \"Collapse\" : \"Expand\""],
  "RunCommandConfirm.svelte": ["66:commands.length","66:commands.length === 1 ? \"command\" : \"commands\"","67:cwd ? ` in ${displaySafePath(cwd)}` : \"\"","80:running ? \"Running…\" : \"Run\"","86:r.command","88:r.error","90:r.code ?? \"signal\"","90:r.truncated ? \" · output truncated\" : \"\"","91:r.stdout","92:r.stderr"],
  "ContentSearchDialog.svelte": ["110:$t(\"search.inFilesTitle\")","112:$t(\"search.docsTitle\")","113:$t(\"common.close\")","130:$t(\"search.matchCase\")","131:$t(\"search.button\")","136:$t(\"search.searching\")","138:error","140:$t(\"search.noMatchesInFolder\")","143:$t(\"search.filterResultsAria\")","146:$t(\"search.matchesInFiles\", { matches: result.matches.length === 1 ? $t(\"search.matchOne\", { count: result.matches.length }) : $t(\"search.matchMany\", { count: result.matches.length }), files: groups.length === 1 ? $t(\"search.fileOne\", { count: groups.length }) : $t(\"search.fileMany\", { count: groups.length }), })","150:$t(\"search.shown\", { count: shownGroups.length })","151:$t(\"search.truncated\")","154:$t(\"search.noFilesMatch\", { query: resultFilter.trim() })","159:$t(\"search.toggleFile\")","159:collapsedFiles.has(g.path) ? \"▸\" : \"▾\"","159:collapsedFiles.has(g.path) ? $t(\"home.expand\") : $t(\"home.collapse\")","162:g.matches.length","168:mt.line_number","169:seg.text"],
  "DuplicatesDialog.svelte": ["107:$t(\"dup.title\")","109:$t(\"common.close\")","114:$t(\"dup.intro\")","115:$t(\"dup.scan\")","118:$t(\"dup.scanning\")","120:error","122:$t(\"dup.none\", { count: result.files_scanned.toLocaleString() })","126:result.groups.length === 1 ? $t(\"dup.summaryOne\", { count: result.groups.length, size: formatSize(reclaimable) || \"0 B\" }) : $t(\"dup.summaryMany\", { count: result.groups.length, size: formatSize(reclaimable) || \"0 B\" })","129:$t(\"dup.capped\")","132:$t(\"dup.selectRedundant\")","132:$t(\"dup.selectRedundantTip\")","134:deleting ? $t(\"dup.removing\") : $t(\"dup.moveToBin\", { count: selected.size })","143:$t(\"dup.copiesEach\", { count: g.paths.length, size: formatSize(g.size) || \"0 B\" })","144:$t(\"dup.extra\", { size: formatSize(g.size * (g.paths.length - 1)) || \"0 B\" })","148:$t(\"dup.markForBin\")"],

  // --- The ticket's originally-disclosed "not yet covered" dialogs — pinned exactly, not fixed here ---
  "ContentIndexSearchDialog.svelte": ["152:$t(\"search.byContentTitle\")","153:baseName(root) || root","153:root","155:$t(\"search.rebuildContentIndex\")","156:$t(\"search.rebuildContentIndex\")","159:$t(\"search.docsTitle\")","160:$t(\"common.close\")","172:$t(\"search.byContentPlaceholder\")","179:$t(\"search.byContentNeedsBuildTitle\")","180:$t(\"search.byContentNeedsBuildBody\")","182:$t(\"search.buildingContentIndex\", { count: buildProgress?.files_indexed ?? 0 })","183:buildProgress.current_path","185:buildError","188:$t(\"search.buildContentIndex\")","192:$t(\"search.checkingContentIndex\")","194:$t(\"search.searching\")","196:error","198:$t(\"search.byContentTypeHint\")","200:$t(\"search.byContentNoMatches\")","203:$t(\"search.buildingContentIndex\", { count: buildProgress?.files_indexed ?? 0 })","206:hits.length === 1 ? $t(\"search.matchOne\", { count: hits.length }) : $t(\"search.matchMany\", { count: hits.length })","210:h.path","213:baseName(h.path)","214:relativeToRoot(h.path, root)","215:$t(\"search.byContentScoreTitle\")","217:scorePercent(h.score)","221:seg.text"],
  "FileHealthDialog.svelte": ["424:$t(\"fh.title\")","426:$t(\"fh.title\")","427:baseName(root) || root","427:root","428:$t(\"common.close\")","444:$t(tab.labelKey)","454:$t(\"fh.excludeLabel\")","459:pattern","463:$t(\"fh.excludeRemove\")","464:$t(\"fh.excludeRemove\")","472:$t(\"fh.excludeEmpty\")","480:$t(\"fh.excludeAddLabel\")","486:$t(\"fh.excludeSuggest\")","488:s","490:$t(\"fh.excludeHint\")","497:$t(\"fh.intro\")","498:$t(\"fh.scan\")","501:$t(\"fh.scanning\")","503:error","504:$t(\"fh.scan\")","506:$t(\"fh.none\", { count: scanned.toLocaleString() })","507:$t(\"fh.scan\")","511:links.length === 1 ? $t(\"fh.summaryOne\", { count: links.length }) : $t(\"fh.summaryMany\", { count: links.length })","514:$t(\"fh.scanned\", { count: scanned.toLocaleString() })","515:$t(\"fh.capped\")","517:$t(\"fh.scan\")","522:l.path","524:baseName(l.path)","525:parentDir(l.path)","526:reasonLabel(l.reason)","535:$t(\"fh.introMismatch\")","536:$t(\"fh.scan\")","539:$t(\"fh.scanning\")","541:mismatchError","542:$t(\"fh.scan\")","544:$t(\"fh.noneMismatch\", { count: mismatchScanned.toLocaleString() })","545:$t(\"fh.scan\")","549:mismatchHits.length === 1 ? $t(\"fh.summaryOneMismatch\", { count: mismatchHits.length }) : $t(\"fh.summaryManyMismatch\", { count: mismatchHits.length })","552:$t(\"fh.scanned\", { count: mismatchScanned.toLocaleString() })","553:$t(\"fh.capped\")","555:$t(\"fh.scan\")","576:h.path","585:baseName(h.path)","586:parentDir(h.path)","589:$t(\"fh.mismatchBadge\", { claimed: h.claimedExt, detected: h.detectedLabel })","592:h.fixError","600:$t(\"fh.mismatchFix\", { ext: h.detectedExt })","605:h.fixing ? $t(\"fh.mismatchFixing\") : $t(\"fh.mismatchFix\", { ext: h.detectedExt })","616:$t(\"fh.introOrphan\")","617:$t(\"fh.scan\")","620:$t(\"fh.scanning\")","622:orphanError","623:$t(\"fh.scan\")","625:$t(\"fh.noneOrphan\", { count: orphanScanned.toLocaleString() })","626:$t(\"fh.scan\")","630:orphans.length === 1 ? $t(\"fh.summaryOneOrphan\", { count: orphans.length }) : $t(\"fh.summaryManyOrphan\", { count: orphans.length })","633:$t(\"fh.scanned\", { count: orphanScanned.toLocaleString() })","634:$t(\"fh.capped\")","636:$t(\"fh.scan\")","641:o.path","643:baseName(o.path)","644:parentDir(o.path)","647:$t(\"fh.orphanBadge\")","656:$t(\"fh.introEmpty\")","657:$t(\"fh.scan\")","660:$t(\"fh.scanning\")","662:emptyError","663:$t(\"fh.scan\")","665:$t(\"fh.noneEmpty\", { count: emptyScanned.toLocaleString() })","666:$t(\"fh.scan\")","670:emptyDirs.length === 1 ? $t(\"fh.summaryOneEmpty\", { count: emptyDirs.length }) : $t(\"fh.summaryManyEmpty\", { count: emptyDirs.length })","673:$t(\"fh.scanned\", { count: emptyScanned.toLocaleString() })","674:$t(\"fh.capped\")","676:$t(\"fh.scan\")","681:d.path","683:baseName(d.path)","684:parentDir(d.path)"],
  "NearDuplicatesDialog.svelte": ["152:title","154:title","155:baseName(root) || root","155:root","156:$t(\"common.close\")","163:$t(\"nd.intro\")","164:$t(\"nd.scan\")","167:$t(\"nd.scanning\")","169:error","170:$t(\"nd.scan\")","172:$t(\"nd.none\", { count: scannedCount.toLocaleString() })","173:$t(\"nd.scan\")","177:groups.length === 1 ? $t(\"nd.summaryOne\", { count: groups.length }) : $t(\"nd.summaryMany\", { count: groups.length })","180:$t(\"sim.scanned\", { count: scannedCount.toLocaleString() })","181:$t(\"sim.capped\")","184:$t(\"nd.scan\")","185:$t(\"nd.selectExtrasTip\")","185:$t(\"sim.selectExtras\")","187:deleting ? $t(\"sim.removing\") : $t(\"sim.moveToBin\", { count: selected.size })","196:$t(\"nd.groupHead\", { count: g.paths.length })","201:$t(\"nd.markForBin\")","204:p","206:baseName(p)","207:parentDir(p)"],
  "SimilarImagesDialog.svelte": ["152:$t(\"sim.title\")","154:$t(\"sim.title\")","155:baseName(root) || root","155:root","156:$t(\"common.close\")","163:$t(\"sim.intro\")","164:$t(\"sim.scan\")","167:$t(\"sim.scanning\")","169:error","170:$t(\"sim.scan\")","172:$t(\"sim.none\", { count: filesScanned.toLocaleString() })","173:$t(\"sim.scan\")","177:groups.length === 1 ? $t(\"sim.summaryOne\", { count: groups.length }) : $t(\"sim.summaryMany\", { count: groups.length })","180:$t(\"sim.scanned\", { count: filesScanned.toLocaleString() })","181:$t(\"sim.capped\")","184:$t(\"sim.scan\")","185:$t(\"sim.selectExtras\")","185:$t(\"sim.selectExtrasTip\")","187:deleting ? $t(\"sim.removing\") : $t(\"sim.moveToBin\", { count: selected.size })","196:$t(\"sim.groupHead\", { count: g.paths.length })","201:$t(\"sim.markForBin\")","204:p","206:baseName(p)","207:parentDir(p)"],
  "DeclutterDialog.svelte": ["178:$t(\"dc.title\")","180:$t(\"dc.title\")","181:baseName(root) || root","181:root","182:$t(\"common.close\")","189:$t(\"dc.intro\")","190:$t(\"dc.scan\")","193:$t(\"dc.scanning\")","195:error","196:$t(\"dc.scan\")","198:$t(\"dc.none\")","199:$t(\"dc.scan\")","203:findings.length === 1 ? $t(\"dc.summaryOne\", { count: findings.length }) : $t(\"dc.summaryMany\", { count: findings.length })","208:$t(\"dc.scan\")","210:deleting ? $t(\"dc.removing\") : $t(\"dc.moveToBin\", { count: selected.size })","219:g.rows.length","219:reasonLabel(g.reason)","224:$t(\"dc.markForBin\")","227:f.path","228:f.name"],
  "BatchMediaDialog.svelte": ["432:paths.length","432:paths.length === 1 ? \"\" : \"s\"","469:watermarkImage || \"No image chosen — no watermark\"","470:watermarkImage ? baseName(watermarkImage) : \"No image chosen (no watermark)\"","488:$t(\"bm.renameEscapes\")","494:$t(\"bm.convertEscapes\")","501:mediaOpLabel(op)","521:RENAME_DEFAULT_TEMPLATE","534:baseName(it.input)","534:it.input","536:baseName(it.output)","536:it.output","537:it.summary","541:MAX_PREVIEW","541:previewCappedTotal","548:planError","550:applyError","556:planned.length","556:planned.length === 1 ? \"\" : \"s\"","563:done","563:failed > 0 ? `, ${failed} failed` : \"\"","563:total","569:completed.skipped.length","569:completed.written","573:s.name","574:s.reason","592:checkpointFailures.length","593:checkpointFailures.length === 1 ? \"\" : \"s\"","597:baseName(dir) || dir","597:dir","600:checkpointFailures.length === 1 ? \"that folder\" : \"those folders\"","612:checkpointPartial.length","613:checkpointPartial.length === 1 ? \"\" : \"s\"","617:p.dir","618:baseName(p.dir) || p.dir","618:p.skippedCount","618:p.skippedCount === 1 ? \"\" : \"s\"","622:checkpointPartial.length === 1 ? \"that folder\" : \"those folders\"","634:overwriteItems.length","634:overwriteItems.length === 1 ? \"\" : \"s\"","637:uniqueParentDirs(overwriteItems.map((it) => it.input)).length === 1 ? \"\" : \"s\"","645:overwriteItems.length","645:overwriteItems.length === 1 ? \"\" : \"s\"","655:applying ? \"Applying…\" : \"Apply\""],
  "SplitFileDialog.svelte": ["101:baseName(path)","101:result.part_count","101:result.part_count === 1 ? \"\" : \"s\"","104:result.part_count","105:formatSize(result.part_size)","106:formatSize(result.total_size)","107:outDir","114:baseName(path)","129:p.label","168:outDir","176:error","183:busy ? \"Splitting…\" : \"Split\""],
  "JoinPartsDialog.svelte": ["131:baseName(joinedPath)","133:joinedPath","140:baseName(path)","146:preview.partCount","147:formatSize(preview.totalSize)","151:previewError","165:outPath","173:error","180:busy ? \"Joining…\" : \"Join\""],
  "ExplorerPane.svelte": ["505:$t(\"menu.view\")","507:$t(\"view.details\")","508:$t(\"view.list\")","509:$t(\"tb.icons\")","510:$t(\"view.gallery\")","514:$t(\"tb.sortBy\")","516:$t(\"sort.name\")","517:$t(\"tb.modified\")","518:$t(\"sort.type\")","519:$t(\"sort.size\")","523:$t(\"tb.direction\")","525:$t(\"cmd.ascending\")","526:$t(\"cmd.descending\")","530:$t(\"cmd.showHidden\")","537:$t(\"tb.fileList\")","573:$t(\"agent.watch\", { name: watchedAgentName })","575:baseName(c.path)","575:c.kind === \"removed\" ? \"−\" : c.kind === \"created\" ? \"+\" : \"~\"","575:c.path","578:$t(\"agent.watching\")","580:$t(\"agent.showLog\")","581:$agentTimeline.length ? `(${$agentTimeline.length})` : \"\"","581:$t(\"agent.log\")","588:selectedTag","589:visible.length"],
  "TerminalPanel.svelte": ["183:t.cwd","185:basename(t.cwd) || \"shell\"","216:c.label","234:openError"],

  // --- CPE-1768: newly-registered candidates discovered by the membership-rule sweep (45 files) ---
  "AboutDialog.svelte": ["40:version || \"—\"","50:s.id","50:s.name","51:s.version || \"—\"","52:s.contract || \"—\"","53:h.label"],
  "AttributesDialog.svelte": ["181:heading","182:targets.length","187:error","198:modePreview","209:ch","213:error","214:notice"],
  "BackupDashboard.svelte": ["128:job.name","138:progress","138:total ? ` / ${total}` : \"\"","141:fmtTime(lastRun[job.id].when)","141:lastRun[job.id].failed ? `, ${lastRun[job.id].failed} failed` : \"\"","141:lastRun[job.id].label","141:lastRun[job.id].ok","146:history[job.id].length","146:history[job.id].length === 1 ? \"\" : \"s\"","146:showHistory === job.id ? \"▾\" : \"▸\"","152:fmtTime(run.when)","152:run.failed ? `, ${run.failed} failed` : \"\"","152:run.label","152:run.ok","168:error","172:plan.copy.length","172:plan.delete.length","172:plan.unchanged","172:plan.update.length"],
  "BinaryPreview.svelte": ["186:loadError","198:fmtCount(info.sections.length)","201:fmtCount(info.imports.length)","204:fmtCount(info.exports.length)","207:fmtCount(info.symbols.length)","222:formatLabel(info.format)","223:info.arch ?? \"Unknown\"","224:info.is_64 ? \"64-bit\" : \"32-bit\"","225:formatSize(size)","226:fmtCount(info.sections.length)","227:fmtCount(info.imports.length)","228:fmtCount(info.exports.length)","229:fmtCount(info.symbols.length)","246:formatSize(s.size)","246:hexAddress(s.address)","246:s.name","253:fmtCount(BINARY_TABLE_ROW_CAP)","253:fmtCount(sectionsCap.total)","266:i.library ?? \"—\"","266:i.name","273:fmtCount(BINARY_TABLE_ROW_CAP)","273:fmtCount(importsCap.total)","286:e.name","286:hexAddress(e.address)","293:fmtCount(BINARY_TABLE_ROW_CAP)","293:fmtCount(exportsCap.total)","300:info.format === \"Pe\" ? \"No symbol table — a typical PE EXE/DLL doesn't carry one (only object files and PDBs do).\" : \"No symbols found.\"","310:hexAddress(s.address)","310:s.name","317:fmtCount(BINARY_TABLE_ROW_CAP)","317:fmtCount(symbolsCap.total)","338:dotnetMeta.assembly.name","339:dotnetMeta.assembly.version","340:cultureLabel(dotnetMeta.assembly.culture)","341:hexOrDash(dotnetMeta.assembly.public_key)","345:rawAssemblyFlags(dotnetMeta.assembly.flags)","352:f","361:dotnetMeta.runtime_version","365:fmtCount(dotnetMeta.assembly_refs.length)","374:cultureLabel(r.culture)","374:hexOrDash(r.public_key_token)","374:r.name","374:r.version","381:fmtCount(assemblyRefsCap.total)","381:fmtCount(BINARY_TABLE_ROW_CAP)","388:fmtCount(dotnetMeta.types.length)","397:t.name","397:t.namespace || \"—\"","404:fmtCount(BINARY_TABLE_ROW_CAP)","404:fmtCount(typesCap.total)","411:fmtCount(dotnetMeta.methods.length)","420:m.name","427:fmtCount(BINARY_TABLE_ROW_CAP)","427:fmtCount(methodsCap.total)","469:hexAddress(ins.address)","469:ins.bytes","469:ins.text","475:fmtCount(disasm.length)"],
  "CertPreview.svelte": ["72:loadError","80:data.error","84:cert.subject","85:cert.issuer","86:cert.serial","87:cert.version","91:humanIso(cert.not_before)","98:humanIso(cert.not_after)","102:cert.signature_algorithm","103:keyLabel(cert.public_key)","104:cert.is_ca ? \"Yes\" : \"No\"","115:san","125:ku","134:eku","143:cert.sha256_fingerprint","146:copiedKey === \"sha256\" ? \"Copied\" : \"Copy\"","151:cert.sha1_fingerprint","154:copiedKey === \"sha1\" ? \"Copied\" : \"Copy\"","161:csr.subject","162:keyLabel(csr.public_key)","170:san","178:keyLabel(pubKey)","188:keyLabel(privKey)","196:data.encoding.toUpperCase()"],
  "CommandBar.svelte": ["57:$t('cmd.new')","74:selectionCount > 1 ? \"Rename one item at a time\" : \"Rename (F2)\"","87:$t('cmd.open')","94:$t('cmd.sort')","102:$t(s.labelKey)","108:$t('cmd.ascending')","112:$t('cmd.descending')","120:$t('cmd.view')","128:$t(v.labelKey)","134:$t('cmd.showHidden')","138:$t('cmd.groupFolders')","146:FILE_FILTERS.find((f) => f.key === fileFilter) ? $t('filter.' + fileFilter) : $t('cmd.filter')","155:$t('filter.' + f.key)","169:`${$t('palette.ariaPalette')} (Ctrl+Shift+P)`","180:`${c.name} (user command)`","181:c.name","188:showDetails ? \"Hide details pane (Alt+P)\" : \"Show details pane (Alt+P)\"","199:showTerminal ? \"Hide terminal\" : \"Show terminal\""],
  "CompareDialog.svelte": ["147:summary.added","148:summary.removed","149:summary.changed","150:summary.identical","159:error","166:textDiff.added","166:textDiff.removed","171:row.op === \"add\" ? \"+\" : row.op === \"del\" ? \"−\" : \" \"","171:row.text","180:(fileDiff.firstDiff ?? 0).toString(16).toUpperCase()","180:fileDiff.firstDiff","181:fileDiff.ranges.length","182:fileDiff.lengthDiffers ? \"differ\" : \"match\"","197:row.hasChildren ? (collapsed.has(row.path) ? \"▸\" : \"▾\") : \"\"","199:STATUS_LABEL[row.node.status]"],
  "ContextMenu.svelte": ["191:selectionCount > 1 ? \"Rename one item at a time\" : \"Rename (F2)\"","198:$t('ctx.open')","202:$t('ctx.execute')","205:$t('ctx.executeAdmin')","210:$t('ctx.openNewTab')","214:$t('ctx.openInTerminal')","220:$t('ctx.workOnThis')","227:$t('ctx.repairLink')","237:$t('ctx.folder')","241:$t('ctx.textFile')","247:$t(ft.labelKey)","254:$t('ctx.duplicate')","258:$t('ctx.copyAsPath')","263:$t('ctx.copyToFolder')","266:$t('ctx.moveToFolder')","270:$t('ctx.copyName')","282:$t('ctx.rename')","287:$t('ctx.batchMedia')","292:$t('ctx.compareFiles')","297:$t('ctx.selectAllExt', { ext: sameTypeExt })","302:$t('ctx.extract')","305:$t('ctx.extractTo')","313:$t('ctx.archiveSafety')","331:$t('ctx.compressZip')","334:$t('ctx.compressTarGz')","337:$t('ctx.compressWithPassword')","383:pinned ? $t('ctx.unpinFromHome') : $t('ctx.pinToHome')","388:favorited ? $t('ctx.removeFavorite') : $t('ctx.addFavorite')","393:$t('ctx.tags')","401:name","413:c.name","420:$t('ctx.reveal')","423:$t('ctx.properties')","427:$t('studio.menu')","436:$t('ctx.shred')","448:$t('ctx.open')","453:$t('ctx.folder')","456:$t('ctx.textFile')","462:$t(ft.labelKey)","469:$t('ctx.copyAsPath')","472:$t('ctx.openInTerminal')","476:$t('ctx.properties')","481:$t('ctx.ejectDrive')","503:$t('ctx.open')","506:$t('ctx.openNewTab')","510:$t('ctx.copyAsPath')","513:$t('ctx.properties')","518:$t('home.removeNetworkLocation')","522:$t('home.disconnectShare')","531:$t('ctx.open')","535:$t('ctx.openNewTab')","540:$t('ctx.reveal')","543:$t('ctx.copy')","546:$t('ctx.copyAsPath')","549:$t('ctx.rename')","556:$t('ctx.folder')","559:$t('ctx.textFile')","565:$t(ft.labelKey)","572:$t('ctx.properties')","577:$t('ctx.delete')","583:$t('ctx.addFavorite')","587:$t('home.pinToQuickAccess')","595:$t('home.removeFromFavorites')","596:$t('home.removeFromRecentFolders')","597:$t('home.removeFromRecent')","601:$t('home.clearAll')","613:$t('ctx.folder')","617:$t('ctx.textFile')","623:$t(ft.labelKey)","632:$t('ctx.newLink')","637:$t('ctx.paste')","641:$t('ctx.undo')","641:undoLabel ? ` ${undoLabel}` : ''","648:$t('view.details')","652:$t('view.list')","656:$t('view.icons')","660:$t('view.gallery')","667:$t('sort.name')","671:$t('sort.modified')","675:$t('sort.type')","679:$t('sort.size')","684:$t('cmd.ascending')","688:$t('cmd.descending')","694:$t('ctx.selectAll')","698:$t('ctx.invertSelection')","701:$t('ctx.selectByPattern')","704:$t('ctx.refresh')","710:$t('ctx.openInTerminal')","713:$t('ctx.workOnFolder')","717:$t('ctx.reveal')","721:$t('ctx.properties')","736:$t('palette.ariaPalette')"],
  "DataBrowser.svelte": ["92:isSqlite ? \"Table / view\" : \"Sheet\"","93:s","98:offset + 1","98:offset + page.rows.length","98:page.total","115:error","121:c.type || \"column\"","122:c.name","122:sortDir === 1 ? \"▲\" : \"▼\"","128:cell","132:loading ? \"Loading…\" : \"No rows.\"","134:loading ? \"Loading…\" : \"\""],
  "DocsView.svelte": ["99:expanded ? \"Collapse section\" : \"Expand section\"","102:g.name","103:g.docs.length","114:d.title","126:html"],
  "EmailPreview.svelte": ["47:loadError","55:data.error","60:data.from ?? \"—\"","62:data.to.join(\", \")","65:data.cc.join(\", \")","67:data.subject ?? \"—\"","69:dateText","76:data.attachments.length === 1 ? \"1 attachment\" : `${data.attachments.length} attachments`","79:`${displaySafeName(att.filename)} — ${att.content_type}`","82:formatSize(att.size)","94:data.body"],
  "FloatPreview.svelte": [],
  "FontPreview.svelte": ["199:$t(\"pv.cantFont\")","213:sampleText","221:format ?? formatLabelForExt(extension)","222:metadata.family","223:metadata.style","224:metadata.version","226:metadata.numGlyphs.toLocaleString()","228:formatSize(size)","239:codepointLabel(selectedGlyph)","239:glyphChar(selectedGlyph)","249:codepointLabel(cp)","250:`Glyph ${codepointLabel(cp)}`","253:glyphChar(cp)","259:glyphGrid.shown.length","259:glyphGrid.total.toLocaleString()","265:glyphGrid.total.toLocaleString()","266:glyphGrid.total === 1 ? \"character\" : \"characters\"","272:glyphGrid.total"],
  "HexView.svelte": ["69:sig.ext","69:sig.name","72:(pageOffset + bytes.length).toString(16).toUpperCase()","72:pageOffset.toString(16).toUpperCase()","72:size","77:error","83:row.offset","87:hex2(b)","90:row.ascii","97:cursor.toString(16).toUpperCase()","101:row.type","101:row.value"],
  "IcalPreview.svelte": ["54:loadError","59:data.calendar_name","59:data.method","64:data.error","70:ev.summary ?? \"(no title)\"","72:componentBadge(ev.component)","79:whenText(ev)","82:ev.location","85:ev.organizer","88:ev.status","94:ev.attendees.length === 1 ? \"1 attendee\" : `${ev.attendees.length} attendees`","97:att","104:ev.recurrence","110:ev.description"],
  "Icon.svelte": [],
  "JwtPreview.svelte": ["61:loadError","69:data.error","75:data.alg ?? \"—\"","76:data.typ ?? \"—\"","77:data.kid","86:human(data.iat.raw)","92:human(data.nbf.raw)","101:human(data.exp.raw)","114:data.signature_len === 1 ? \"byte\" : \"bytes\"","114:data.signature_len.toLocaleString()","116:data.alg === \"none\" ? \"alg: none\" : \"empty or malformed\"","125:payloadJson","132:headerJson"],
  "LinkBadge.svelte": ["89:title"],
  "LogPreview.svelte": ["194:loadError","210:LEVEL_LABEL[level]","210:log.counts[level]","223:unleveledCount","227:log.lines.length","227:log.lines.length === 1 ? \"\" : \"s\"","227:visibleLines.length","234:formatSize(win.file_len)","234:formatSize(win.window_end - win.window_start)","235:win.file_len.toLocaleString()","235:win.window_end.toLocaleString()","235:win.window_start.toLocaleString()","237:win.window_end.toLocaleString()","237:win.window_start.toLocaleString()","238:formatSize(win.file_len)","238:win.file_len.toLocaleString()","273:log.lines.length.toLocaleString()","273:log.totalLines.toLocaleString()","292:line.index + 1","293:line.level ? LEVEL_LABEL[line.level] : \"\"","294:line.text","294:line.truncated ? \"…\" : \"\""],
  "MacroRunConfirm.svelte": ["81:macro.name","89:macro.steps.length","89:macro.steps.length === 1 ? \"\" : \"s\"","90:inputs.length","90:inputs.length === 1 ? \"\" : \"s\"","93:planError","99:op.detail","99:op.input","99:op.kind","104:runError","113:running ? \"Running…\" : \"Run\"","119:macro.name","119:run.ops.length","119:run.ops.length === 1 ? \"\" : \"s\"","124:undoError","130:undoing ? \"Undoing…\" : \"Undo\""],
  "MacrosDialog.svelte": ["254:m.name","255:m.steps","255:m.steps === 1 ? \"\" : \"s\"","304:STEP_LABEL[kindOf(step)]","330:STEP_LABEL[k]","361:error","362:note","363:macros.length","363:macros.length === 1 ? \"\" : \"s\""],
  "MediaPlayer.svelte": ["163:state.playing ? \"Pause\" : \"Play\"","165:state.playing ? \"Pause\" : \"Play\"","177:mt.formatTime(state.currentTime)","192:mt.formatTime(state.duration)","198:state.muted ? \"Unmute\" : \"Mute\"","200:state.muted ? \"Unmute\" : \"Mute\"","231:state.rate"],
  "MediaQuickLook.svelte": ["87:count","87:position + 1","121:repeatLabel","123:repeatLabel","129:shuffled ? \"Shuffle on\" : \"Shuffle off\"","131:shuffled ? \"on\" : \"off\""],
  "MenuBar.svelte": ["125:$t(menu.labelKey)","134:$t(menu.labelKey)","149:item.label ?? (item.labelKey ? $t(item.labelKey) : \"\")","150:item.hint","169:$t(\"menu.language\")","173:$t(\"menu.language\")","177:$t(\"menu.language\")","184:$locale === l.code ? \"✓\" : \"\"","185:l.english","185:l.name","189:cov === 0 ? \"Not yet translated — shows in English\" : `${Math.round(cov * 100)}% translated — the rest shows in English`","190:cov === 0 ? \"English\" : `${Math.round(cov * 100)}%`"],
  "MetadataStudioDialog.svelte": ["206:$t(\"studio.title\")","208:$t(\"studio.title\")","209:$t(\"common.close\")","213:$t(\"studio.noFile\")","218:$t(\"studio.viewOnly\")","222:$t(\"studio.loading\")","224:error","226:$t(\"studio.noMeta\")","238:groupLabel(g)","247:f.key","261:$t(\"studio.revertFieldHint\")","262:$t(\"studio.revertFieldAria\", { field: f.key })","270:writable ? $t(\"studio.fieldReadonly\") : $t(\"studio.viewOnly\")","271:currentValue(f, edited) || \"—\"","285:$t(\"studio.applyAll\", { n: files.length })","291:$t(\"studio.stripEditableHint\")","294:$t(\"studio.stripEditable\")","299:$t(\"studio.copyFromFirstHint\")","302:$t(\"studio.copyFromFirst\")","310:$t(\"studio.resetAllHint\")","313:$t(\"studio.resetAll\")","316:notice","319:$t(\"common.close\")","322:saving ? $t(\"studio.saving\") : $t(\"studio.save\")"],
  "NetworkConnectionForm.svelte": ["93:editing ? `Edit connection ${editing.name}` : \"Add a connection\"","98:editing ? `Edit “${editing.name}”` : \"Add a connection\"","109:s","114:hints.hostLabel","118:hints.userLabel","126:hints.pathLabel","133:AUTH_LABELS[kind]","160:error","163:editing ? \"Save\" : \"Add\""],
  "NetworkConnectionMenu.svelte": ["51:`${name} actions`"],
  "NetworkSecretPrompt.svelte": ["41:`${label} for ${name}`","42:label","42:name","52:label"],
  "NotebookPreview.svelte": ["106:loadError","109:parseError","111:rawFallback","113:RAW_FALLBACK_CHARS.toLocaleString()","121:notebook.cells.length","121:notebook.totalCells","131:cell.type","133:cell.executionCount != null ? `In [${cell.executionCount}]` : \"In [ ]\"","139:cellHtml[cell.index] ?? \"\"","143:cellHtml[cell.index] ?? \"\"","145:cell.source","158:output.text","161:output.ename","161:output.evalue","162:output.traceback","171:output.text","174:output.otherMimeTypes.join(\", \")","174:output.otherMimeTypes.length","184:cell.outputs.length","184:cell.outputsTotal"],
  "OrganizeDialog.svelte": ["99:$t(\"org.title\")","101:$t(\"org.title\")","109:$t(r.labelKey)","113:error","117:$t(\"org.result\", { moved: movedCount, skipped: skippedCount })","119:$t(\"org.checkpointNote\", { label: outcome.checkpoint.checkpoint.label || outcome.checkpoint.checkpoint.manifest_id.slice(0, 12) })","121:$t(\"org.undo\")","126:$t(\"org.loading\")","128:$t(\"org.empty\")","131:$t(\"org.willMove\", { count: plan.length, groups: groups.length })","137:g.items.length","137:g.subdir","152:$t(\"common.cancel\")","155:applying ? $t(\"org.applying\") : $t(\"org.apply\")"],
  "ScheduledSnapshots.svelte": ["119:error","139:rule.enabled ? \"on\" : \"paused\"","180:key"],
  "SidecarManager.svelte": ["101:$t(\"mgr.checking\")","103:$t(\"mgr.none\")","110:row.running ? $t(\"mgr.running\") : $t(\"mgr.stopped\")","111:row.name","112:row.version","113:$t(\"mgr.\" + health.key)","114:$t(\"mgr.contractTip\")","115:row.compatible ? $t(\"mgr.contractOk\", { v: row.contract }) : $t(\"mgr.contractBad\", { v: row.contract })","118:row.enabled ? $t(\"mgr.enabled\") : $t(\"mgr.disabled\")","120:row.enabled ? $t(\"mgr.enabled\") : $t(\"mgr.disabled\")","128:CAPABILITY_INFO[cap].label","130:$t(\"mgr.revoke\")","132:$t(\"mgr.grant\")","132:$t(\"mgr.grantTip\")","136:$t(\"mgr.noCapabilities\")","141:$t(\"mgr.lastError\")","141:diag.last_error","143:$t(\"mgr.healthy\")","145:$t(\"mgr.notRunning\")","148:$t(\"mgr.repair\")","151:logsOpen[row.id] ? $t(\"mgr.hideLogs\") : $t(\"mgr.viewLogs\", { count: diag.logs.length })","154:$t(\"mgr.noLogs\")","159:$t(\"mgr.repairDid\")","159:repairMsg[row.id]","163:line.level","163:line.message","174:$t(\"mgr.open\")","177:$t(\"mgr.stop\")"],
  "SmartFolderMenu.svelte": ["44:name","52:$t(\"ctx.rename\")","57:$t(\"smart.moveUp\")","58:$t(\"smart.moveUp\")","64:$t(\"smart.moveDown\")","65:$t(\"smart.moveDown\")","71:$t(\"common.apply\")","72:$t(\"menu.delete\")","73:$t(\"common.cancel\")"],
  "SyncDialog.svelte": ["101:status?.branch ? `“${status.branch}”` : \"repository\"","104:status.upstream","109:status.behind","110:status.ahead","132:status.blocked","138:syncActionLabel(action)","147:w","161:m < 60 ? `${m} min` : `${m / 60} h`","174:line","184:running ? \"Syncing…\" : \"Run sync\""],
  "TagEditor.svelte": ["138:$t(\"tags.title\")","139:$t(\"tags.title\")","140:$t(\"status.items\", { count })","146:tag","147:$t(\"tags.remove\")","153:$t(\"tags.none\")","160:$t(\"tags.addLabel\")","167:$t(\"tags.colorLabel\")","175:$t(`tags.color.${key === \"\" ? \"none\" : key}`)","176:$t(`tags.color.${key === \"\" ? \"none\" : key}`)","188:nativeName","190:$t(\"tags.pullNative\")","191:$t(\"tags.pushNative\")","192:syncNote","198:$t(\"tags.cancel\")","199:$t(\"tags.apply\")"],
  "TemplatesDialog.svelte": ["132:path ? `Capture ${displaySafeName(base(path))}` : \"No folder\"","145:t.name","146:t.dirs","146:t.files","160:path ? `Stamp into ${displaySafeName(base(path))}` : \"No folder\"","171:error","172:note","173:templates.length","173:templates.length === 1 ? \"\" : \"s\""],
  "ThumbnailImage.svelte": [],
  "TransferPanel.svelte": ["38:label(t)","51:percent(t)","52:t.done_items","52:t.total_items","53:t.report.errors.join(\"\\n\")","53:t.report.errors.length","53:t.report.errors.length === 1 ? \"\" : \"s\""],
  "UserCommandsDialog.svelte": ["87:c.name","88:c.template","90:c.mode","91:s","119:s"],
  "VaultBadge.svelte": ["34:title"],
  "VaultBanner.svelte": ["36:locking ? $t(\"vault.lockingTitle\") : $t(\"vault.lockTitle\")","40:locking ? $t(\"vault.locking\") : $t(\"vault.lock\")"],
  "VcardPreview.svelte": ["53:loadError","57:data.cards.length","61:data.error","67:heading(card)","69:subheading(card)","74:formatSize(card.photo_size)","82:tel.number","83:t","91:em.address","92:t","100:adr.label","101:t","106:url","109:card.birthday"],
  "WatchRulesDialog.svelte": ["171:rule.name","172:condSummary(rule.when)","173:rule.actions.map(actSummary).join(\", \")","212:actSummary(a)","222:preview.actions.map((a) => a.resolved).join(\", \")","222:preview.rule.name","241:f","247:fire.summary"],
  "WorkspacesDialog.svelte": ["73:w.name","74:w.tabs.length","74:w.tabs.length === 1 ? '' : 's'"],
  "YamlTomlPreview.svelte": ["155:loadError","165:parseErrorMessage","170:format === \"yaml\" ? \"YAML\" : \"TOML\"","170:parseErrorMessage","175:rawFallback","177:RAW_FALLBACK_CHARS.toLocaleString()"],

  // --- B1 (reviewer, round 2): 6 more candidates surfaced by the widened CANDIDATE_PATTERN ---
  "CardDetailDialog.svelte": ["122:id","123:title","137:error","143:k","143:v","150:bodyHtml","162:sending ? \"…\" : \"Send ▸\"","167:detail?.location || \"Tickets\"","168:bodyLines","168:bodyLines === 1 ? \"\" : \"s\"","168:metaFields.length","168:metaFields.length === 1 ? \"\" : \"s\""],
  "CreateCertDialog.svelte": ["183:v","188:`Remove ${v}`","210:v","215:`Remove ${v}`","256:kt.label","313:error","320:busy ? \"Creating…\" : \"Create\""],
  "NewLinkDialog.svelte": ["114:$t(\"link.newLinkTitle\")","115:$t(\"link.newLinkTitle\")","117:$t(\"link.kindLabel\")","119:$t(\"link.kindSymlink\")","120:$t(\"link.kindHardlink\")","122:$t(\"link.kindJunction\")","126:$t(\"link.targetLabel\")","137:$t(\"link.browse\")","141:$t(\"link.junctionTargetHint\")","144:$t(\"link.nameLabel\")","155:error","159:$t(\"common.cancel\")","162:$t(\"link.create\")"],
  "RepairLinkDialog.svelte": ["106:$t(\"link.repairTitle\")","107:$t(\"link.repairTitle\")","108:$t(\"link.repairIntro\")","111:$t(\"link.repairLoading\")","115:$t(\"link.repairSuggestionLabel\")","119:$t(\"link.repairNoSuggestion\")","124:translate($locale, \"link.repairConfirm\", { target: displaySafePath(chosenTarget ?? \"\") })","128:$t(\"common.cancel\")","131:$t(\"link.repairConfirmYes\")","135:error","138:$t(\"common.close\")","141:$t(\"link.repairBrowse\")","148:$t(\"link.repairAccept\")"],
  "Spotlight.svelte": ["186:$t(\"spotlight.title\")","197:$t(\"spotlight.ariaSearch\")","201:query.trim() ? $t(\"spotlight.noMatches\") : $t(\"spotlight.typeHint\")","205:$t(GROUP_LABEL[section.kind])"],
  "WorkbenchView.svelte": ["92:branch || \"detached\"","96:stats.added","96:stats.files","96:stats.files === 1 ? \"\" : \"s\"","96:stats.removed","130:error","132:branch || \"the working tree\"","140:isCollapsed ? \"Expand\" : \"Collapse\"","141:isCollapsed ? \"▸\" : \"▾\"","143:fs.added","143:fs.removed","144:copiedFile === key ? \"✓ Copied\" : \"Copy\"","149:h.header","151:l.kind === \"add\" ? \"+\" : l.kind === \"del\" ? \"−\" : \" \"","151:l.newLine ?? \"\"","151:l.oldLine ?? \"\"","151:l.text","151:s.text"],

  // --- CPE-1790: the confirm/password-prompt dialogs, previously invisible to isCandidateComponent
  // because their own props (`title`/`message`/`error`) don't match any name/path SHAPE — see the
  // ticket and bidiRenderScan.ts's CANDIDATE_PATTERN doc for why generic-prop leaves needed their own
  // membership trigger (a call to displaySafeName/displaySafePath), not just a wider vocabulary list.
  // Both dialogs now escape `title`/`message`(/`error`) on arrival — CPE-1760's "leaf escapes what it
  // renders" model — so every App.svelte call site is covered whether or not it remembers to wrap its
  // own name first. The one remaining offender in each (`confirmLabel`) is a caller-chosen static verb
  // ("OK"/"Delete"/"Extract"/"Compress"/"Unlock"/"Delete permanently"/"Close all" — never a filesystem
  // name), the same "harmless, unprovable-but-not-a-name" shape most REGISTRY entries carry.
  "ConfirmDialog.svelte": ["39:confirmLabel"],
  "PasswordPromptDialog.svelte": ["78:confirmLabel"],
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
const APP_MARKUP_OFFENDERS = ["6391:$t(\"palette.openAgentBoardWindow\")","6403:$t(\"sidebar.repositories\")","6406:$t(\"sidebar.repositories\")","6412:$agentSessions.length === 0 ? $t(\"tb.openConsole\") : $agentSessions.length === 1 ? $t(\"tb.openConsoleOne\") : $t(\"tb.openConsoleMany\", { count: $agentSessions.length })","6420:$t(\"tb.aiConsole\")","6422:$agentSessions.length","6422:$t(\"tb.agentsRunning\", { count: $agentSessions.length })","6429:$t(\"tb.showDetailsPane\")","6434:$t(\"cmd.showHidden\")","6439:$t(\"cmd.folderSizes\")","6444:$t(\"tb.resetSettings\")","6531:$t(\"tb.paneWidth\")","6610:$t(\"tb.resizeNav\")","6611:$t(\"tb.resizeTip\")","6773:$t(\"tb.resizeDetails\")","6774:$t(\"tb.resizeTip\")","6783:$t(\"tb.popoutTip\")","6784:$t(\"tb.popoutAria\")","6789:$t(\"tb.defaultTab\")","6797:$t(\"tb.preview\")","6798:$t(\"view.details\")","6802:$t(\"tb.paneWidth\")","6818:$t(\"tb.previewOrDetails\")","6819:$t(\"tb.dragPopoutTip\")","6830:$t(\"tb.preview\")","6836:$t(\"view.details\")","7059:confirm.title","7074:passwordPrompt.title","7412:runConfirm.title","7430:macroParamPromptFor.macro.name","7696:$t(\"dnd.dropToImport\")"];

/** App.svelte's two already-disclosed SplitFileDialog/JoinPartsDialog completion notices
 *  (`showNotice($t(..., { name: baseName(path) }))`) are built in `<script>` code, not markup — the one
 *  shape `findUnsafeRenderLines` (markup-only, see its module doc) genuinely cannot see, since the
 *  eventual DOM render happens through a separate `{notice}`-style span elsewhere, not at this call
 *  site. Checked here with a narrow, targeted scan instead: every `baseName(`/`basename(` call anywhere
 *  in App.svelte's source not immediately wrapped in `displaySafeName(`/`displaySafePath(` must be one
 *  of these two allowlisted lines. */
const APP_SCRIPT_BASENAME_ALLOWLIST = [2749, 2764];

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
  it("every registered component's raw-render set matches its recorded lines EXACTLY", () => {
    const mismatches: string[] = [];
    for (const [file, recorded] of Object.entries(REGISTRY)) {
      const src = readFileSync(join(COMPONENTS, file), "utf8");
      const found = findUnsafeRenderLines(src, file);
      const recordedSorted = [...recorded].sort(compareOffenders);
      if (JSON.stringify(found) !== JSON.stringify(recordedSorted)) {
        const newlyRaw = found.filter((l) => !recordedSorted.includes(l));
        const stale = recordedSorted.filter((l) => !found.includes(l));
        // F5 (reviewer, CPE-1761 attempt 2): the useful delta goes FIRST — a developer reading a failed
        // registry file must see what actually changed before wading through the full recorded/found
        // dumps (which, for a file like AgentTimeline.svelte, run to several KB and bury the diff).
        mismatches.push(
          `${file}:` +
            (newlyRaw.length ? ` NEW raw offender(s) (line:expr): ${newlyRaw.join(",")}` : "") +
            (stale.length ? ` STALE recorded entry(ies), expression no longer matches (line:expr): ${stale.join(",")}` : "") +
            ` — full found [${found.join(",")}] vs recorded [${recordedSorted.join(",")}]`,
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
    expect(REGISTRY["PreviewPane.svelte"], "PreviewPane.svelte:1015 must be a currently-recorded offender for this demonstration to mean anything").toContain("1015:$t(action.labelKey)");

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
    // And the actual guard comparison (REGISTRY's recorded array vs. the freshly computed set) must
    // therefore differ — this is what makes the real "every registered component..." test above red.
    const recordedSorted = [...REGISTRY["PreviewPane.svelte"]].sort(compareOffenders);
    expect(JSON.stringify(found)).not.toBe(JSON.stringify(recordedSorted));
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
  // hand-added keys above, now catches this exact class of component.
  it("CPE-1790: ConfirmDialog.svelte and PasswordPromptDialog.svelte are detected as candidates now that they escape on arrival", () => {
    for (const f of ["ConfirmDialog.svelte", "PasswordPromptDialog.svelte"]) {
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
    const recordedSorted = [...REGISTRY["ConfirmDialog.svelte"]].sort(compareOffenders);

    expect(found, "the un-escaped message render must be flagged").toContain("35:message");
    expect(JSON.stringify(found), "the mutated file's offender set must no longer equal what's recorded — this is what makes the real REGISTRY-equality test above red").not.toBe(
      JSON.stringify(recordedSorted),
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
