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
// raw lines are pinned exactly rather than left completely unchecked. That's 41 of the 135 `.svelte`
// files under src/lib/components/, plus App.svelte on its own (see below) — NOT `readdir(components)` in
// full (review round 2's B5, explicitly hedged "Consider"). Running this engine over the other ~94 files
// found the overwhelming majority of their `{…}` renders are non-filesystem UI text (macro/workspace/
// rule/template names, i18n strings, counts) that the ticket's own scope excludes — auditing every one of
// them individually is a different, much larger undertaking than this ticket's residual-call-site list,
// and doing it hastily to tick a box would produce exactly the "guard that passes while the property is
// broken" the review warned against. Left as a natural follow-up, not silently declared done.
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
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { findUnsafeRenderLines, compareOffenders } from "./bidiRenderScan";

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
  "ConflictDialog.svelte": ["117:unresolved","121:f.label","136:showBase ? \"Hide\" : \"Show\"","145:versions.base ?? \"— absent —\"","146:versions.ours ?? \"— absent —\"","147:versions.theirs ?? \"— absent —\"","163:error || note || `${opLabel || \"No\"} operation in progress`","171:unresolved > 0 ? \"Resolve every file first\" : `Continue the ${opLabel.toLowerCase()}`"],
  "FileNameSearchDialog.svelte": ["96:$t(\"search.findByNameTitle\")","98:$t(\"search.docsTitle\")","99:$t(\"common.close\")","116:$t(\"search.button\")","121:$t(\"search.searching\")","123:error","125:$t(\"search.noNameMatches\")","128:hits.length === 1 ? $t(\"search.matchOne\", { count: hits.length }) : $t(\"search.matchMany\", { count: hits.length })","129:$t(\"search.truncated\")"],
  "RepoBrowser.svelte": ["287:isGeneric ? \"Git URL\" : \"Repository\"","301:loading ? \"Browsing…\" : \"Browse\"","305:cloning ? \"Cloning…\" : \"Clone\"","319:statusText","324:consent.host","336:repo","364:fmtSize(e.size)","373:loaded ? repo : \"No repository open\""],
  "AgentTimeline.svelte": ["605:entries.length","665:diff ? `${displaySafePath(e.path)} — hover to see what changed` : displaySafePath(e.path)","670:KIND_LABEL[e.kind]","673:clock(e.at)","710:playing ? \"Pause\" : \"Play\"","713:playing ? \"Pause\" : \"Play\"","734:s","760:cpMarkerTitle(m)","761:`Checkpoint ${m.cp.label || cpShortId(m.cp.manifest_id)}`","769:new Date(t).toLocaleTimeString()","781:selectedCheckpoint.manifest_id","782:selectedCheckpoint.label || cpShortId(selectedCheckpoint.manifest_id)","783:cpTime(selectedCheckpoint.ts)","796:formatBytes(revertPreview.bytes_written)","798:revertPreview.drift_count","802:revertPreview.drift_count","816:revertError","822:selectedCheckpoint.label || cpShortId(selectedCheckpoint.manifest_id)","826:revertPreview.drift_count","852:KIND_LABEL[replayCurrent.kind]","854:clock(replayCurrent.at)","905:replayKindLabel(re.kind)","906:clock(re.ts)","920:KIND_LABEL[e.kind]","922:clock(e.at)","943:c.sessionId","946:formatTokens(c.inputTokens)","947:formatTokens(c.outputTokens)","948:formatTokens(c.totalTokens)","949:formatUsd(c.costUsd)","951:formatTokens(c.filesTouched)","952:formatTokens(c.editCount)","953:formatBytes(c.churnBytes)","954:formatDuration(c.wallClockMs)","958:formatPerMinute(c.tokensPerMinute)","961:formatUsd(c.usdPerFile)","964:formatBytes(c.churnPer1kTokens)","988:relativeLabel(o.lastAt, Date.now())","992:friendlyActor(a, sessions)","1015:rc.kind === \"divergence\" ? \"diverged\" : \"collided\"","1017:relativeLabel(rc.lastAt, Date.now())","1021:friendlyActor(a, sessions)","1024:renameConflictNote(rc.kind)","1047:formatTokens(historyRollup.totals.sessions)","1048:formatUsd(historyRollup.totals.costUsd)","1049:formatTokens(historyRollup.totals.totalTokens)","1050:formatDuration(historyRollup.totals.wallClockMs)","1051:formatTokens(historyRollup.totals.filesTouched)","1052:formatBytes(historyRollup.totals.churnBytes)","1061:historyUnclean","1073:formatPerMinute(historyRollup.ratios.tokensPerMinute)","1076:formatUsd(historyRollup.ratios.usdPerSession)","1079:formatUsd(historyRollup.ratios.usdPerFile)","1082:formatBytes(historyRollup.ratios.churnPer1kTokens)","1096:row.model","1097:formatTokens(row.sessions)","1098:formatTokens(row.totalTokens)","1099:formatUsd(row.costUsd)","1100:historyShare(row.costUsd, historyRollup.totals.costUsd)","1116:row.agentName","1117:formatTokens(row.sessions)","1118:formatTokens(row.totalTokens)","1119:formatUsd(row.costUsd)","1120:historyShare(row.costUsd, historyRollup.totals.costUsd)","1141:new Date(row.startedAt).toLocaleString()","1142:row.agentName || row.agentId","1142:row.agentName || row.agentId || \"(unknown)\"","1143:historyDurationLabel(row)","1151:isSessionEndedCleanly(row) ? \"Clean\" : \"Ended unexpectedly\"","1185:historyMetric === 'cost' ? 'Cost' : 'Tokens'","1197:historyBarDate(p.bucketStart)"],
  "ConsultedFiles.svelte": ["31:$agentConsulted.length","40:e.count"],
  "SessionHistoryDialog.svelte": ["101:s","109:k","116:error","123:formatDate(e.ts)","124:e.kind","132:filtered.length"],
  "IntegrityDialog.svelte": ["84:hasBaseline ? `Baseline: ${baseline.length} files` : \"No baseline stored\"","85:note","90:error","107:label"],
  "CheckpointDialog.svelte": ["217:error","218:note","233:cp.label || shortId(cp.manifest_id)","234:fmtTime(cp.ts)","248:$t('ckpt.failedTitle')","248:cf.reason","251:$t(\"ckpt.failedTitle\")","252:fmtTime(cf.ts)","253:cf.reason","277:fmtBytes(preview.bytes_written)","279:preview.drift_count","288:diffOpenPath === p ? \"Close diff\" : \"Open diff\"","295:diffError","320:selected.label || shortId(selected.manifest_id)","323:selected.label || shortId(selected.manifest_id)"],
  "DiffSideBySide.svelte": ["38:r.left ?? \"\"","39:r.right ?? \"\""],
  "InspectCryptoDialog.svelte": [],
  "BoardView.svelte": ["265:error","297:col","298:list.length","301:showArchived ? \"hide\" : `+${archivedEpicList.length} archived`","311:\"Open \" + e.id + \" — details\"","313:e.id","314:\"Copy \" + e.id","315:copiedId === e.id ? \"✓\" : \"⧉\"","316:e.status","318:e.title","321:bar.state === \"empty\" ? \"No sub-tickets yet\" : bar.state === \"complete\" && p.total === 0 ? \"Epic complete\" : p.done + \" of \" + p.total + \" tickets done\"","328:bar.label","350:col","351:list.length","354:showArchived ? \"hide\" : `+${archived.length} archived`","363:\"Open \" + c.id + \" — details\"","365:c.id","366:\"Copy \" + c.id","367:copiedId === c.id ? \"✓\" : \"⧉\"","368:c.priority","370:c.title","373:c.epic","374:c.sprint","375:t","394:grouped[l].length","394:l","396:error || note || \"\""],
  "CopilotDialog.svelte": ["196:planError","199:phase === \"planning\" ? \"Planning…\" : \"Plan\"","206:instruction","213:v","219:planResult.summary.moves","220:planResult.summary.renames","221:planResult.summary.deletes","222:planResult.summary.mkdirs","223:planResult.summary.copies","232:opKind(op)","243:execError","267:v","273:execResult.results.filter((r) => r.ok).length","280:r.error","287:execResult.checkpoint.checkpoint.manifest_id","292:undoing ? \"Undoing…\" : \"Undo\"","295:undoError"],

  // --- B4: the 19 components CPE-1712 itself originally escaped ---------------------------------
  "FileList.svelte": ["646:$t(\"fl.columnsButton\")","647:$t(\"fl.columnsButton\")","656:$t(\"fl.sortBy\", { col: $t(col.labelKey) })","658:$t(col.labelKey)","672:$t(\"fl.sortBy\", { col: ac.col.label })","674:ac.col.label","691:$t(\"fl.resizeColumn\", { col: handleLabel(i) })","694:$t(\"fl.resizeTip\")","705:error","708:$t(\"fl.loading\")","713:searching ? $t(\"fl.noMatch\") : $t(\"fl.empty\")","779:tagEntry.label","795:ruleStyle.label","800:tag","817:$t(ACTIVITY_LABEL_KEY[act.kind])","819:$t(\"fl.agentInside\")","824:formatDate(entry.modified)","825:typeName(entry)","828:folderSizes.has(entry.path) ? formatSize(folderSizes.get(entry.path) ?? 0) : \"…\"","830:formatSize(entry.size)","845:cell.display","863:$t(\"fl.agentLegend\")","867:friendlyActor(a, sessions)"],
  "Sidebar.svelte": ["415:agentsOpen ? \"Collapse\" : \"Expand\"","419:$t(\"sidebar.agents\")","428:`${s.agentName}${s.provider ? \" · \" + s.provider : \"\"}${s.model ? \" · \" + s.model : \"\"} · ${s.cwd} (double-click to open its tab · right-click for more)`","440:sessionNum(s.sessionId)","442:s.agentName || s.agentId || \"Agent\"","443:baseName(s.cwd)","471:favOpen ? \"Collapse\" : \"Expand\"","515:tagsOpen ? \"Collapse\" : \"Expand\"","527:`${count} item${count === 1 ? \"\" : \"s\"} tagged “${tag}” — click to filter, right-click to rename/delete`","533:tag","534:count","561:smartOpen ? \"Collapse\" : \"Expand\"","565:$t(\"smart.section\")","573:$t(\"smart.itemTip\", { tag: sf.tag })","579:sf.name","606:savedSearchOpen ? \"Collapse\" : \"Expand\"","610:$t(\"smart.searchSection\")","618:$t(\"smart.searchItemTip\")","624:ss.name","650:exploreOpen ? \"Collapse\" : \"Expand\"","654:$t(\"sidebar.explore\")","670:$t(\"sidebar.repositories\")","706:placesOpen ? \"Collapse\" : \"Expand\"","710:$t(\"sidebar.quickAccess\")","738:drivesOpen ? \"Collapse\" : \"Expand\"","742:$t(\"sidebar.drives\")","771:open ? \"Collapse\" : \"Expand\"","799:`${formatSize(u.free)} free of ${formatSize(u.total)}`","803:formatSize(u.free)","864:networkOpen ? \"Collapse\" : \"Expand\"","884:`${conn.scheme}://${conn.host} — ${stateTitle(state, connectionErrors[conn.name])} (right-click for more)`","919:savable ? `${displaySafePath(s.path)} — discovered on your network; click to add it as a connection` : `${displaySafePath(s.path)} — discovered on your network; ${prefill.scheme.toUpperCase()} isn't supported yet`","958:trashOpen ? \"Collapse\" : \"Expand\"","962:$t(\"sidebar.trash\")","969:$t(\"trash.openTip\")","974:$t(\"trash.open\")","981:$t(\"trash.macMessage\")","984:$t(\"trash.macLabel\")"],
  "TabBar.svelte": ["40:$t(\"app.closeTab\")","49:$t(\"app.newTab\")"],
  "HomeView.svelte": ["126:quickOpen ? $t(\"home.collapse\") : $t(\"home.expand\")","131:$t(\"home.quickAccess\")","163:$t(\"home.unpinQuick\")","180:recentOpen ? $t(\"home.collapse\") : $t(\"home.expand\")","185:tab === \"favorites\" ? $t(\"home.favorites\") : tab === \"folders\" ? $t(\"home.recentFolders\") : tab === \"shared\" ? $t(\"home.shared\") : $t(\"home.recent\")","187:$t(\"home.clear\")","190:$t(\"home.addNetworkLocation\")","197:$t(\"home.recent\")","200:$t(\"home.favorites\")","203:$t(\"home.folders\")","206:$t(\"home.shared\")","214:$t(\"home.noRecent\")","215:$t(\"home.noRecentSub\")","220:$t(\"home.dateOpened\")","220:$t(\"home.name\")","233:formatDate(r.opened)","239:$t(\"home.removeFromRecent\")","240:$t(\"home.removeFromRecent\")","253:$t(\"home.noFavorites\")","254:$t(\"home.noFavoritesSub\")","275:$t(\"home.removeFromFavorites\")","288:$t(\"home.noRecentFolders\")","289:$t(\"home.noRecentFoldersSub\")","310:$t(\"home.removeFromRecentFolders\")","311:$t(\"home.removeFromRecentFolders\")","335:$t(\"home.add\")","336:$t(\"common.cancel\")","342:$t(\"home.sharedLoading\")","347:$t(\"home.noShared\")","348:$t(\"home.noSharedSub\")","370:$t(\"home.removeNetworkLocation\")","371:$t(\"home.removeNetworkLocation\")"],
  "DetailsPane.svelte": ["28:typeName(one)","31:formatSize(one.size) || \"0 B\"","36:formatDate(one.modified) || \"—\"","42:selected.length","46:selected.filter((e) => e.is_dir).length","50:selected.filter((e) => !e.is_dir).length","54:formatSize(totalSize) || \"0 B\""],
  "TrashView.svelte": ["149:$t(\"trash.title\")","150:itemCountLabel","155:allSelected ? $t(\"trash.deselectAll\") : $t(\"trash.selectAll\")","158:$t(\"trash.restoreSelected\")","161:$t(\"trash.emptySelected\")","164:$t(\"trash.emptyAll\")","167:$t(\"trash.refresh\")","178:$t(\"trash.restoreFailed\", { name: displaySafeName(f.name), error: f.error })","186:$t(\"trash.loading\")","188:$t(\"trash.error\", { error })","190:$t(\"trash.empty\")","194:$t(\"trash.selectAll\")","196:$t(\"trash.columnsName\")","197:$t(\"trash.columnsOriginalPath\")","198:$t(\"trash.columnsDeleted\")","215:formatSize(e.size)","218:formatDate(e.time_deleted * 1000)","228:$t(\"trash.emptyConfirmTitle\")"],
  "NavToolbar.svelte": ["85:$t('nav.back')","88:$t('nav.forward')","91:$t('nav.up')","94:$t('nav.refresh')","114:density === \"compact\" ? \"Switch to comfortable density\" : \"Switch to compact density\"","115:density === \"compact\" ? \"Switch to comfortable density\" : \"Switch to compact density\"","169:$t('nav.search')","169:searchScope","170:$t(\"nav.searchHint\")"],
  "PropertiesDialog.svelte": ["220:$t(\"prop.title\")","221:$t(\"common.close\")","225:error","232:$t(\"prop.type\")","232:typeName(single)","233:$t(\"prop.location\")","236:$t(\"prop.size\")","238:$t(\"prop.calculating\")","239:$t(\"prop.sizeBytes\", { size: formatSize(folderSize) || \"0 B\", bytes: folderSize.toLocaleString() })","240:$t(\"prop.unavailable\")","245:$t(\"prop.size\")","246:$t(\"prop.sizeBytes\", { size: formatSize(single.size) || \"0 B\", bytes: single.size.toLocaleString() })","250:$t(\"prop.created\")","250:formatDate(info.created) || \"—\"","251:$t(\"prop.modified\")","251:formatDate(info.modified) || \"—\"","253:$t(\"prop.attributes\")","255:[info.readonly ? $t(\"prop.readonly\") : null, info.hidden ? $t(\"prop.hidden\") : null] .filter(Boolean) .join(\", \") || $t(\"prop.none\")","263:label","263:value","268:label","268:value","273:$t(\"prop.typeMismatch\")","282:checksum","283:$t(\"prop.copyChecksumTip\")","285:copied ? $t(\"prop.copied\") : $t(\"prop.copy\")","296:$t(\"prop.match\")","296:$t(\"prop.matchTip\")","298:$t(\"prop.noMatch\")","298:$t(\"prop.noMatchTip\")","302:$t(\"prop.computing\")","304:hashError","306:$t(\"prop.compute\")","313:$t(\"prop.contents\")","316:$t(\"prop.contentStats\", { lines: stats.lines.toLocaleString(), words: stats.words.toLocaleString(), chars: stats.chars.toLocaleString() })","318:$t(\"prop.counting\")","320:statError","322:$t(\"prop.count\")","331:$t(\"prop.itemsSelected\", { count: entries.length })","334:$t(\"prop.folders\")","334:folderCount","335:$t(\"prop.files\")","335:fileCount","337:$t(\"prop.sizeOfFiles\")","338:$t(\"prop.sizeBytes\", { size: formatSize(totalSize) || \"0 B\", bytes: totalSize.toLocaleString() })","341:$t(\"prop.folderNote\")","341:$t(\"prop.note\")","349:nativeStoreName","355:tag","364:nativeEntry.label || \"None\"","368:nativePulling ? \"Pulling…\" : \"Pull\"","370:nativeError","376:$t(\"common.close\")"],
  "InstantSearch.svelte": ["165:$t(\"search.instantTitle\")","167:$t(\"search.instantTitle\")","168:$t(\"search.docsTitle\")","169:$t(\"common.close\")","182:$t(\"search.instantPlaceholder\")","188:$t(\"search.instantOffTitle\")","189:$t(\"search.instantOffBody\")","191:$t(\"search.buildingIndex\", { count: buildStats?.dirs_scanned ?? 0 })","193:buildError","196:$t(\"search.buildIndex\")","198:$t(\"search.instantOpenFolderFirst\")","201:$t(\"search.searching\")","203:error","205:$t(\"search.instantTypeHint\")","207:$t(\"search.instantNoMatches\")"],
  "ArchiveSafetyDialog.svelte": ["75:$t(\"arcsafe.title\")","78:$t(\"arcsafe.title\")","80:$t(\"common.close\")","86:$t(\"arcsafe.scanning\")","88:error","89:$t(\"arcsafe.retry\")","96:$t(\"arcsafe.unreadable\")","107:$t(\"arcsafe.encrypted\", { count: result.unreadable_entries })","115:$t(\"arcsafe.dangerous\")","121:$t(\"arcsafe.encrypted\", { count: result.unreadable_entries })","126:$t(\"arcsafe.safe\")","131:$t(\"arcsafe.ratio\")","132:ratioLabel(result.report.overall_ratio)","133:$t(\"arcsafe.sizes\")","134:sizeLabel(result.report.total_compressed)","135:$t(\"arcsafe.entries\")","137:result.entries_scanned.toLocaleString()","138:$t(\"arcsafe.capped\")","141:$t(\"arcsafe.unreadableEntries\")","142:result.unreadable_entries.toLocaleString()","147:$t(\"arcsafe.flaggedHead\", { count: result.report.flagged.length })","155:ratioLabel(f.ratio)","160:$t(\"arcsafe.noneFlagged\")"],
  "PreviewPane.svelte": ["1015:$t(action.labelKey)","1017:$t(action.labelKey)","1024:actionMessage","1031:$t(\"pv.model.title\")","1033:$t(\"pv.model.format\")","1033:modelFormatLabel","1036:$t(\"pv.model.encoding\")","1037:modelInfo.ascii ? $t(\"pv.model.ascii\") : $t(\"pv.model.binary\")","1041:$t(\"pv.model.meshes\")","1041:modelInfo.mesh_count.toLocaleString()","1043:modelCountLabel","1043:modelInfo.triangle_count.toLocaleString()","1044:$t(\"pv.model.vertices\")","1044:modelInfo.vertex_count.toLocaleString()","1048:$t(\"pv.model.dimensions\")","1049:fmtDim(modelDims.w)","1057:$t(\"pv.dicom.title\")","1060:name","1060:value","1069:$t(\"pv.loading\")","1071:$t(\"pv.cantImage\")","1077:$t(\"pv.loading\")","1085:$t(\"pv.loading\")","1093:$t(\"pv.loading\")","1113:$t(\"pv.loading\")","1136:$t(\"pv.loading\")","1138:$t(\"pv.cantArchive\")","1146:e.is_dir ? \"\" : formatSize(e.size)","1151:entries.length === 1 ? $t(\"pv.itemOne\", { count: entries.length }) : $t(\"pv.itemMany\", { count: entries.length })","1156:$t(\"pv.loading\")","1158:$t(\"pv.cantFile\")","1160:info","1202:$t(\"pv.loading\")","1204:$t(\"pv.cantFile\")","1208:saving ? $t(\"pv.saving\") : $t(\"pv.save\")","1209:$t(\"common.cancel\")","1210:saveError","1229:$t(\"pv.json.viewTree\")","1234:$t(\"pv.json.viewRaw\")","1239:$t(\"pv.edit\")","1247:cell","1252:$t(\"pv.showingRows\", { cap: CSV_ROW_CAP, total: tableRows.length })","1263:prettyJson(text)","1267:mdHtml","1276:breadcrumbSym.name","1285:`Jump to ${sym.kind} ${sym.name}, line ${sym.line}`","1286:`${sym.name} — line ${sym.line}`","1290:sym.name","1306:foldCollapsed.has(i + 1) ? `Expand lines ${i + 1}–${foldByStart.get(i + 1)?.end_line}` : `Collapse lines ${i + 1}–${foldByStart.get(i + 1)?.end_line}`","1306:line","1334:$t(\"menu.cut\")","1335:$t(\"menu.copy\")","1336:$t(\"menu.paste\")","1338:$t(\"ctx.selectAll\")"],
  "QuickLook.svelte": ["33:index + 1"],
  "DiskSpaceView.svelte": ["147:formatSize(total)","147:loading ? \" · scanning…\" : \"\"","182:formatSize(c?.size ?? 0)","202:formatSize(c.size)"],
  "DropStackPanel.svelte": ["44:open ? \"Hide Drop Stack\" : \"Show Drop Stack\"","51:$dropStackEntries.length","77:canTransfer ? \"Move every shelved item into the current folder\" : \"Not a valid destination for the Drop Stack\"","85:canTransfer ? \"Copy every shelved item into the current folder\" : \"Not a valid destination for the Drop Stack\""],
  "FolderBrowser.svelte": ["121:$t(\"pv.loading\")","123:$t(\"pv.folder.cantOpen\")","125:$t(\"fl.empty\")","140:formatSize(entry.size)"],
  "SidebarNode.svelte": ["42:open ? \"Collapse\" : \"Expand\""],
  "RunCommandConfirm.svelte": ["66:commands.length","80:running ? \"Running…\" : \"Run\"","86:r.command","90:r.truncated ? \" · output truncated\" : \"\"","91:r.stdout","92:r.stderr"],
  "ContentSearchDialog.svelte": ["110:$t(\"search.inFilesTitle\")","112:$t(\"search.docsTitle\")","113:$t(\"common.close\")","130:$t(\"search.matchCase\")","131:$t(\"search.button\")","136:$t(\"search.searching\")","138:error","140:$t(\"search.noMatchesInFolder\")","143:$t(\"search.filterResultsAria\")","146:$t(\"search.matchesInFiles\", { matches: result.matches.length === 1 ? $t(\"search.matchOne\", { count: result.matches.length }) : $t(\"search.matchMany\", { count: result.matches.length }), files: groups.length === 1 ? $t(\"search.fileOne\", { count: groups.length }) : $t(\"search.fileMany\", { count: groups.length }), })","151:$t(\"search.truncated\")","154:$t(\"search.noFilesMatch\", { query: resultFilter.trim() })","159:$t(\"search.toggleFile\")","159:collapsedFiles.has(g.path) ? \"▸\" : \"▾\"","159:collapsedFiles.has(g.path) ? $t(\"home.expand\") : $t(\"home.collapse\")","162:g.matches.length","168:mt.line_number","169:seg.text"],
  "DuplicatesDialog.svelte": ["107:$t(\"dup.title\")","109:$t(\"common.close\")","114:$t(\"dup.intro\")","115:$t(\"dup.scan\")","118:$t(\"dup.scanning\")","120:error","122:$t(\"dup.none\", { count: result.files_scanned.toLocaleString() })","126:result.groups.length === 1 ? $t(\"dup.summaryOne\", { count: result.groups.length, size: formatSize(reclaimable) || \"0 B\" }) : $t(\"dup.summaryMany\", { count: result.groups.length, size: formatSize(reclaimable) || \"0 B\" })","129:$t(\"dup.capped\")","132:$t(\"dup.selectRedundant\")","132:$t(\"dup.selectRedundantTip\")","134:deleting ? $t(\"dup.removing\") : $t(\"dup.moveToBin\", { count: selected.size })","143:$t(\"dup.copiesEach\", { count: g.paths.length, size: formatSize(g.size) || \"0 B\" })","144:$t(\"dup.extra\", { size: formatSize(g.size * (g.paths.length - 1)) || \"0 B\" })","148:$t(\"dup.markForBin\")"],

  // --- The ticket's originally-disclosed "not yet covered" dialogs — pinned exactly, not fixed here ---
  "ContentIndexSearchDialog.svelte": ["152:$t(\"search.byContentTitle\")","153:baseName(root) || root","153:root","155:$t(\"search.rebuildContentIndex\")","156:$t(\"search.rebuildContentIndex\")","159:$t(\"search.docsTitle\")","160:$t(\"common.close\")","172:$t(\"search.byContentPlaceholder\")","179:$t(\"search.byContentNeedsBuildTitle\")","180:$t(\"search.byContentNeedsBuildBody\")","182:$t(\"search.buildingContentIndex\", { count: buildProgress?.files_indexed ?? 0 })","183:buildProgress.current_path","185:buildError","188:$t(\"search.buildContentIndex\")","192:$t(\"search.checkingContentIndex\")","194:$t(\"search.searching\")","196:error","198:$t(\"search.byContentTypeHint\")","200:$t(\"search.byContentNoMatches\")","203:$t(\"search.buildingContentIndex\", { count: buildProgress?.files_indexed ?? 0 })","206:hits.length === 1 ? $t(\"search.matchOne\", { count: hits.length }) : $t(\"search.matchMany\", { count: hits.length })","210:h.path","213:baseName(h.path)","214:relativeToRoot(h.path, root)","215:$t(\"search.byContentScoreTitle\")","217:scorePercent(h.score)","221:seg.text"],
  "FileHealthDialog.svelte": ["424:$t(\"fh.title\")","426:$t(\"fh.title\")","427:baseName(root) || root","427:root","428:$t(\"common.close\")","444:$t(tab.labelKey)","454:$t(\"fh.excludeLabel\")","459:pattern","463:$t(\"fh.excludeRemove\")","464:$t(\"fh.excludeRemove\")","472:$t(\"fh.excludeEmpty\")","480:$t(\"fh.excludeAddLabel\")","486:$t(\"fh.excludeSuggest\")","490:$t(\"fh.excludeHint\")","497:$t(\"fh.intro\")","498:$t(\"fh.scan\")","501:$t(\"fh.scanning\")","503:error","504:$t(\"fh.scan\")","506:$t(\"fh.none\", { count: scanned.toLocaleString() })","507:$t(\"fh.scan\")","511:links.length === 1 ? $t(\"fh.summaryOne\", { count: links.length }) : $t(\"fh.summaryMany\", { count: links.length })","515:$t(\"fh.capped\")","517:$t(\"fh.scan\")","522:l.path","524:baseName(l.path)","525:parentDir(l.path)","526:reasonLabel(l.reason)","535:$t(\"fh.introMismatch\")","536:$t(\"fh.scan\")","539:$t(\"fh.scanning\")","541:mismatchError","542:$t(\"fh.scan\")","544:$t(\"fh.noneMismatch\", { count: mismatchScanned.toLocaleString() })","545:$t(\"fh.scan\")","549:mismatchHits.length === 1 ? $t(\"fh.summaryOneMismatch\", { count: mismatchHits.length }) : $t(\"fh.summaryManyMismatch\", { count: mismatchHits.length })","553:$t(\"fh.capped\")","555:$t(\"fh.scan\")","576:h.path","585:baseName(h.path)","586:parentDir(h.path)","589:$t(\"fh.mismatchBadge\", { claimed: h.claimedExt, detected: h.detectedLabel })","592:h.fixError","600:$t(\"fh.mismatchFix\", { ext: h.detectedExt })","605:h.fixing ? $t(\"fh.mismatchFixing\") : $t(\"fh.mismatchFix\", { ext: h.detectedExt })","616:$t(\"fh.introOrphan\")","617:$t(\"fh.scan\")","620:$t(\"fh.scanning\")","622:orphanError","623:$t(\"fh.scan\")","625:$t(\"fh.noneOrphan\", { count: orphanScanned.toLocaleString() })","626:$t(\"fh.scan\")","630:orphans.length === 1 ? $t(\"fh.summaryOneOrphan\", { count: orphans.length }) : $t(\"fh.summaryManyOrphan\", { count: orphans.length })","634:$t(\"fh.capped\")","636:$t(\"fh.scan\")","641:o.path","643:baseName(o.path)","644:parentDir(o.path)","647:$t(\"fh.orphanBadge\")","656:$t(\"fh.introEmpty\")","657:$t(\"fh.scan\")","660:$t(\"fh.scanning\")","662:emptyError","663:$t(\"fh.scan\")","665:$t(\"fh.noneEmpty\", { count: emptyScanned.toLocaleString() })","666:$t(\"fh.scan\")","670:emptyDirs.length === 1 ? $t(\"fh.summaryOneEmpty\", { count: emptyDirs.length }) : $t(\"fh.summaryManyEmpty\", { count: emptyDirs.length })","674:$t(\"fh.capped\")","676:$t(\"fh.scan\")","681:d.path","683:baseName(d.path)","684:parentDir(d.path)"],
  "NearDuplicatesDialog.svelte": ["152:title","154:title","155:baseName(root) || root","155:root","156:$t(\"common.close\")","163:$t(\"nd.intro\")","164:$t(\"nd.scan\")","167:$t(\"nd.scanning\")","169:error","170:$t(\"nd.scan\")","172:$t(\"nd.none\", { count: scannedCount.toLocaleString() })","173:$t(\"nd.scan\")","177:groups.length === 1 ? $t(\"nd.summaryOne\", { count: groups.length }) : $t(\"nd.summaryMany\", { count: groups.length })","181:$t(\"sim.capped\")","184:$t(\"nd.scan\")","185:$t(\"nd.selectExtrasTip\")","185:$t(\"sim.selectExtras\")","187:deleting ? $t(\"sim.removing\") : $t(\"sim.moveToBin\", { count: selected.size })","196:$t(\"nd.groupHead\", { count: g.paths.length })","201:$t(\"nd.markForBin\")","204:p","206:baseName(p)","207:parentDir(p)"],
  "SimilarImagesDialog.svelte": ["152:$t(\"sim.title\")","154:$t(\"sim.title\")","155:baseName(root) || root","155:root","156:$t(\"common.close\")","163:$t(\"sim.intro\")","164:$t(\"sim.scan\")","167:$t(\"sim.scanning\")","169:error","170:$t(\"sim.scan\")","172:$t(\"sim.none\", { count: filesScanned.toLocaleString() })","173:$t(\"sim.scan\")","177:groups.length === 1 ? $t(\"sim.summaryOne\", { count: groups.length }) : $t(\"sim.summaryMany\", { count: groups.length })","181:$t(\"sim.capped\")","184:$t(\"sim.scan\")","185:$t(\"sim.selectExtras\")","185:$t(\"sim.selectExtrasTip\")","187:deleting ? $t(\"sim.removing\") : $t(\"sim.moveToBin\", { count: selected.size })","196:$t(\"sim.groupHead\", { count: g.paths.length })","201:$t(\"sim.markForBin\")","204:p","206:baseName(p)","207:parentDir(p)"],
  "DeclutterDialog.svelte": ["178:$t(\"dc.title\")","180:$t(\"dc.title\")","181:baseName(root) || root","181:root","182:$t(\"common.close\")","189:$t(\"dc.intro\")","190:$t(\"dc.scan\")","193:$t(\"dc.scanning\")","195:error","196:$t(\"dc.scan\")","198:$t(\"dc.none\")","199:$t(\"dc.scan\")","203:findings.length === 1 ? $t(\"dc.summaryOne\", { count: findings.length }) : $t(\"dc.summaryMany\", { count: findings.length })","208:$t(\"dc.scan\")","210:deleting ? $t(\"dc.removing\") : $t(\"dc.moveToBin\", { count: selected.size })","219:reasonLabel(g.reason)","224:$t(\"dc.markForBin\")","227:f.path","228:f.name"],
  "BatchMediaDialog.svelte": ["469:watermarkImage || \"No image chosen — no watermark\"","470:watermarkImage ? baseName(watermarkImage) : \"No image chosen (no watermark)\"","488:$t(\"bm.renameEscapes\")","494:$t(\"bm.convertEscapes\")","501:mediaOpLabel(op)","534:baseName(it.input)","534:it.input","536:baseName(it.output)","536:it.output","537:it.summary","548:planError","550:applyError","556:planned.length","563:done","573:s.name","597:baseName(dir) || dir","597:dir","612:checkpointPartial.length","617:p.dir","618:baseName(p.dir) || p.dir","634:overwriteItems.length","655:applying ? \"Applying…\" : \"Apply\""],
  "SplitFileDialog.svelte": ["101:baseName(path)","104:result.part_count","105:formatSize(result.part_size)","106:formatSize(result.total_size)","107:outDir","114:baseName(path)","129:p.label","168:outDir","176:error","183:busy ? \"Splitting…\" : \"Split\""],
  "JoinPartsDialog.svelte": ["131:baseName(joinedPath)","133:joinedPath","140:baseName(path)","146:preview.partCount","147:formatSize(preview.totalSize)","165:outPath","173:error","180:busy ? \"Joining…\" : \"Join\""],
  "ExplorerPane.svelte": ["505:$t(\"menu.view\")","507:$t(\"view.details\")","508:$t(\"view.list\")","509:$t(\"tb.icons\")","510:$t(\"view.gallery\")","514:$t(\"tb.sortBy\")","516:$t(\"sort.name\")","517:$t(\"tb.modified\")","518:$t(\"sort.type\")","519:$t(\"sort.size\")","523:$t(\"tb.direction\")","525:$t(\"cmd.ascending\")","526:$t(\"cmd.descending\")","530:$t(\"cmd.showHidden\")","537:$t(\"tb.fileList\")","573:$t(\"agent.watch\", { name: watchedAgentName })","575:baseName(c.path)","575:c.kind === \"removed\" ? \"−\" : c.kind === \"created\" ? \"+\" : \"~\"","575:c.path","578:$t(\"agent.watching\")","580:$t(\"agent.showLog\")","581:$agentTimeline.length ? `(${$agentTimeline.length})` : \"\"","581:$t(\"agent.log\")","588:selectedTag","589:visible.length"],
  "TerminalPanel.svelte": ["183:t.cwd","185:basename(t.cwd) || \"shell\"","216:c.label"],
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
});
