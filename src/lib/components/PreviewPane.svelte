<script lang="ts">
  import { tick, onDestroy } from "svelte";
  import { get } from "svelte/store";
  import type { DirEntry } from "../types";
  import { pickProvider, mediaType, visibleActions, type ArchiveEntry, type PreviewAction, type PreviewActionCtx } from "../preview/provider";
  import { parseCsv } from "../preview/csv";
  import { highlightForFile, ensureLanguageForName, languageForName, splitHighlightedIntoLines } from "../preview/highlight";
  import { renderMarkdown } from "../preview/markdown";
  import {
    lineToScrollTop,
    scrollTopToLine,
    enclosingSymbol,
    resolveLineHeight,
    countHiddenAbove,
    foldToExpand,
    lineAtVisibleRow,
    rowScrollTarget,
  } from "../preview/outline";
  import { commands } from "../bindings.gen";
  import type { Symbol as CodeSymbol, FoldRange, MinimapRow, ModelInfo } from "../bindings.gen";
  import HexView from "./HexView.svelte";
  import BinaryPreview from "./BinaryPreview.svelte";
  import DataBrowser from "./DataBrowser.svelte";
  import FontPreview from "./FontPreview.svelte";
  import JwtPreview from "./JwtPreview.svelte";
  import JsonTree from "./JsonTree.svelte";
  import CertPreview from "./CertPreview.svelte";
  import EmailPreview from "./EmailPreview.svelte";
  import IcalPreview from "./IcalPreview.svelte";
  import VcardPreview from "./VcardPreview.svelte";
  import NotebookPreview from "./NotebookPreview.svelte";
  import MediaPlayer from "./MediaPlayer.svelte";
  import FolderBrowser from "./FolderBrowser.svelte";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import { formatSize } from "../format";
  import { lsBool, lsSet } from "../persist";
  import { unwrap, invoke as ipcInvoke } from "../invoke";

  /** The single selected entry to preview, or null. */
  export let entry: DirEntry | null = null;
  /** Resolve a file path to a URL the webview can load (convertFileSrc in the app). */
  export let assetUrl: (path: string) => string = (p) => p;
  /** Read a text file's contents (a size-capped backend command in the app). */
  export let loadText: (path: string) => Promise<string> = async () => "";
  /** List an archive's entries (a backend command in the app). */
  export let loadEntries: (path: string) => Promise<ArchiveEntry[]> = async () => [];
  /** Save edited text back to a file (a backend command in the app). */
  export let saveText: (path: string, contents: string) => Promise<void> = async () => {};
  /** Read a read-only text summary of a binary file (a backend command in the app). */
  export let loadInfo: (path: string) => Promise<string> = async () => "";
  /** Decode a non-native image to a data: URL (a backend command in the app). */
  export let loadImageData: (path: string) => Promise<string> = async () => "";
  /** Extract a camera-RAW file's embedded JPEG preview as a data: URL (a backend command in the app). */
  export let loadRawImageData: (path: string) => Promise<string> = async () => "";
  /** Decode a DICOM file's pixel data to a data: URL (a backend command in the app). */
  export let loadDicomImageData: (path: string) => Promise<string> = async () => "";
  /** Read a curated set of DICOM tags for the preview pane (a backend command in the app). */
  export let loadDicomTags: (path: string) => Promise<[string, string][]> = async () => [];
  /** Decode a HEIC/HEIF file to a data: URL via the platform image stack (a backend command in the app). */
  export let loadHeicImageData: (path: string) => Promise<string> = async () => "";
  /** Structural-validity check for a PDF — resolves (page count, or null if unknown) when the file looks
   *  safe to hand to the WebView2 iframe, rejects on a malformed/empty PDF (a backend command in the app). */
  export let loadPdfValidity: (path: string) => Promise<number | null> = async () => null;
  /** Open a file in the OS default handler — used by the media player's unsupported-codec fallback
   *  (CPE-1429). Wired to the `open_external` command in the app; a no-op in tests. */
  export let openExternal: (path: string) => Promise<void> | void = () => {};
  /** Extract the previewed archive into a new subfolder next to it (CPE-1578, epic CPE-1568 slice 4) —
   *  wired in the app to the SAME `extractHereDest`/`extractWithPasswordFallback` core the context
   *  menu's "Extract" already uses, just entered with the previewed entry. A no-op in tests/float
   *  preview (no app-level transfer state to reuse there). */
  export let extractArchiveHere: (entry: DirEntry) => Promise<void> = async () => {};
  /** Extract the previewed archive into a folder chosen from the native picker (CPE-1578) — wired to the
   *  same path as the context menu's "Extract to…". */
  export let extractArchiveTo: (entry: DirEntry) => Promise<void> = async () => {};
  /** Run the archive-safety scan for the previewed archive (CPE-1578) — wired to open the SAME
   *  `ArchiveSafetyDialog` the context menu's "Check archive safety…" opens. */
  export let checkArchiveSafety: (entry: DirEntry) => void = () => {};

  /** Cap the number of CSV rows rendered so a huge sheet can't lock the pane. */
  const CSV_ROW_CAP = 200;

  /**
   * 3D-model extensions (CPE-1334): the metadata-pane fallback for epic CPE-118. Reuses the same
   * extension list `filetypes.ts`'s ICON_BY_EXT already recognises for the "cube" icon (stl/obj/gltf/
   * glb), narrowed to the formats `read_model_info` (CPE-1333) actually parses (STL/OBJ) plus glTF/GLB,
   * which the backend's file-type detector recognises by magic but doesn't yet geometry-parse — calling
   * it for those simply returns `null` (handled gracefully, no section shown) rather than reinventing
   * detection. Independent of the preview `provider` kind: the geometry section is additive and appears
   * above whatever provider (hex/etc.) renders for these binary formats. PLY (CPE-1340, backend
   * CPE-1337) reuses this same fallback.
   */
  const MODEL_EXTS = new Set(["stl", "obj", "glb", "gltf", "ply"]);

  $: provider = pickProvider(entry);
  $: needsText =
    provider.kind === "text" ||
    provider.kind === "markdown" ||
    provider.kind === "json" ||
    provider.kind === "csv" ||
    provider.kind === "tsv";

  let text = "";
  let textState: "idle" | "loading" | "error" = "idle";
  let reqId = 0;

  let entries: ArchiveEntry[] = [];
  let entriesState: "idle" | "loading" | "error" = "idle";
  let entryReqId = 0;

  let info = "";
  let infoState: "idle" | "loading" | "error" = "idle";
  let infoReqId = 0;

  let imgUrl = "";
  let imgState: "idle" | "loading" | "error" = "idle";
  let imgReqId = 0;

  // Camera-RAW embedded preview (CPE-1349): kept separate from the decoded-image state above so a
  // missing embedded preview can fall back to the metadata slot instead of the tiff/psd error note.
  let rawImgUrl = "";
  let rawImgState: "idle" | "loading" | "error" = "idle";
  let rawImgReqId = 0;

  // DICOM (CPE-1350): the decoded pixel-data image and the curated tag list load independently —
  // an unsupported transfer syntax / corrupt pixel data can fail the image while the tags (read
  // structurally, not from pixel data) still come back fine, so both get their own request/state.
  let dicomImgUrl = "";
  let dicomImgState: "idle" | "loading" | "error" = "idle";
  let dicomImgReqId = 0;

  let dicomTags: [string, string][] = [];
  let dicomTagsReqId = 0;

  // HEIC/HEIF (CPE-1351): decoded via the platform image stack. Kept separate so a missing OS HEIF
  // codec (the common Windows case → backend Err) falls back to the metadata slot, like raw-image.
  let heicImgUrl = "";
  let heicImgState: "idle" | "loading" | "error" = "idle";
  let heicImgReqId = 0;

  // PDF (CPE-1357): WebView2's built-in PDF viewer can crash the renderer — and take the whole app
  // down with it — on a malformed/empty PDF. `pdfState` gates the raw `<iframe>` render: it only ever
  // reaches `src={assetUrl(...)}` once a backend structural-validity check (`loadPdfValidity`) comes
  // back clean; `error` (a rejected check, an iframe `error` event, or a load timeout — see the two
  // handlers below) shows the metadata fallback slot instead, same pattern as raw-image/heic/dicom.
  let pdfState: "idle" | "loading" | "error" = "idle";
  let pdfReqId = 0;
  let pdfLoadTimer: ReturnType<typeof setTimeout> | undefined;
  /** Generous — even a large, legitimately valid PDF should start rendering well inside this; a miss
   *  means the WebView2 PDF plugin is hung on this file, not just being slow. */
  const PDF_LOAD_TIMEOUT_MS = 15_000;

  // ---- generic provider action bar (CPE-1570, epic CPE-1568) ----
  // Providers declare `actions` in `preview/provider.ts`; this renders them uniformly instead of each
  // preview component wiring up its own ad hoc buttons. A self-contained preview component (JWT,
  // certificate, …) that doesn't use the pane's own text pipeline reports its copyable values up via a
  // per-kind `values` bag, keyed to match its provider's declared action ids (see JwtPreview's `onValues`).
  let jwtValues: Record<string, string> = {};
  /** A stable function reference for `JwtPreview`'s `onValues` prop — an inline arrow literal would get a
   *  new identity every time this component re-renders, which would re-trigger JwtPreview's own reactive
   *  `$: onValues(...)` statement (it depends on the prop) on every unrelated update, looping forever. */
  function onJwtValues(v: Record<string, string>) {
    jwtValues = v;
  }

  // Font glyph grid (CPE-1586, epic CPE-1568): `FontPreview.svelte` reports the currently-selected
  // glyph's character + codepoint up whenever the selection changes (always-reactive, like JWT above —
  // it defaults to the grid's first glyph on load rather than requiring an explicit click first).
  let fontValues: Record<string, string> = {};
  function onFontValues(v: Record<string, string>) {
    fontValues = v;
  }

  // JSON tree (CPE-1573, epic CPE-1568): the tree only reports a value when the user clicks a node (see
  // `JsonTree.svelte`'s `setFocus`), unlike JWT's always-reactive `onValues` — so a stale focus from a
  // previously previewed file must be cleared explicitly on entry change, or "Copy path" could stay
  // enabled pointing at the wrong file.
  let jsonValues: Record<string, string> = {};
  function onJsonValues(v: Record<string, string>) {
    jsonValues = v;
  }
  let jsonValuesEntryPath = "";
  $: if (entry?.path !== jsonValuesEntryPath) {
    jsonValuesEntryPath = entry?.path ?? "";
    jsonValues = {};
  }

  // Collapse-all/expand-all (CPE-1573): a one-shot command sent DOWN into the mounted JsonTree via
  // `ctx.dispatch` — the counterpart to the `values` bags above, which only send data UP. `token` bumps
  // on every dispatch so repeating the same command id still re-triggers (see JsonTree's command prop).
  let jsonCommand: { id: string; token: number } | null = null;
  let jsonCommandToken = 0;
  function jsonDispatch(commandId: string) {
    jsonCommandToken += 1;
    jsonCommand = { id: commandId, token: jsonCommandToken };
  }

  // Transient action-bar status message (CPE-1573) — e.g. the JSON Validate action's parse result.
  // `run()` lives in the framework-free `preview/provider.ts`, so it hands PreviewPane an i18n key +
  // params instead of pre-rendered text; PreviewPane (which has `$t`) resolves the string here.
  let actionMessage = "";
  let actionMessageIsError = false;
  let actionMessageTimer: ReturnType<typeof setTimeout> | undefined;
  const ACTION_MESSAGE_ERROR_KEYS = new Set(["pv.json.invalid", "pv.json.formatError"]);
  function showActionMessage(key: string, params?: Record<string, string | number>) {
    actionMessage = get(t)(key, params);
    actionMessageIsError = ACTION_MESSAGE_ERROR_KEYS.has(key);
    if (actionMessageTimer !== undefined) clearTimeout(actionMessageTimer);
    actionMessageTimer = setTimeout(() => {
      actionMessage = "";
    }, 4000);
  }

  async function copyToClipboard(value: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(value);
    } catch {
      /* clipboard unavailable — ignore, same as the pane's existing Copy menu item (menuCopy below) */
    }
  }

  /** Copy an already-resolved image URL to the OS clipboard as real image bytes (CPE-1576, epic
   *  CPE-1568): fetches the URL — works for both the `asset://` protocol a native image uses and a
   *  `data:` URL decoded-image/raw-image/heic already hold — and writes the resulting blob via the
   *  Clipboard API's image `write`, mirroring `copyToClipboard`'s own "best-effort, ignore if
   *  unavailable" convention so a click never throws even where image-clipboard support is missing. */
  async function copyImageToClipboard(url: string): Promise<void> {
    if (!url) return;
    try {
      const resp = await fetch(url);
      const blob = await resp.blob();
      await navigator.clipboard.write([new ClipboardItem({ [blob.type || "image/png"]: blob })]);
    } catch {
      /* image bytes unavailable, or the webview has no image-clipboard support — ignore */
    }
  }

  /** Resolve `titleKey` through i18n, then hand it to the OS-native `window.prompt` (CPE-1576's Convert
   *  action) — the bridge `PreviewActionCtx.promptText` needs since `run()` lives in the framework-free
   *  `preview/provider.ts` module without `$t`, mirroring `showActionMessage`'s own key-resolution split. */
  function promptText(titleKey: string, defaultValue?: string): string | null {
    return window.prompt(get(t)(titleKey), defaultValue);
  }

  /** The currently resolved image URL for the previewed entry, across whichever image-kind provider
   *  matched (CPE-1576): the pane's own `<img src>` — `assetUrl` for a natively-rendered image, else
   *  the already-decoded `data:` URL for decoded-image/raw-image/heic. Reported to the action bar via
   *  `ctx.values["copy-image"]` below, per the CPE-1570 `values` convention (mirrors jwt/json's own
   *  copyable bags), keyed to match `imageActions`' "copy-image" action id in `preview/provider.ts`.
   *  Empty when nothing has resolved yet (still loading, or a raw/heic file the platform couldn't
   *  decode) — the action's own `enabled` gate reads emptiness as "nothing to copy". */
  $: imageCopyUrl =
    provider.kind === "image" && entry
      ? assetUrl(entry.path)
      : provider.kind === "decoded-image" && imgState === "idle"
        ? imgUrl
        : provider.kind === "raw-image" && rawImgState === "idle"
          ? rawImgUrl
          : provider.kind === "heic" && heicImgState === "idle"
            ? heicImgUrl
            : "";

  let actionCtx: PreviewActionCtx | null = null;
  $: actionCtx = entry
    ? {
        entry,
        text: needsText ? text : undefined,
        values:
          provider.kind === "jwt" ? jwtValues
          : provider.kind === "json" ? jsonValues
          : provider.kind === "font" ? fontValues
          : imageCopyUrl ? { "copy-image": imageCopyUrl }
          : {},
        copyToClipboard,
        copyImageToClipboard,
        promptText,
        invoke: ipcInvoke,
        dispatch: provider.kind === "json" ? jsonDispatch : undefined,
        showMessage: showActionMessage,
        extractHere: extractArchiveHere,
        extractTo: extractArchiveTo,
        checkSafety: checkArchiveSafety,
      }
    : null;
  $: actions = actionCtx ? visibleActions(provider, actionCtx) : [];

  let lastActionId = "";
  let lastActionTimer: ReturnType<typeof setTimeout> | undefined;
  /** Run a declared action, then briefly swap its icon to a checkmark so the click has visible feedback
   *  (mirrors the "Copied" flash the inline copy buttons used to show). */
  async function runAction(action: PreviewAction) {
    if (!actionCtx) return;
    await action.run(actionCtx);
    lastActionId = action.id;
    if (lastActionTimer !== undefined) clearTimeout(lastActionTimer);
    lastActionTimer = setTimeout(() => {
      lastActionId = "";
    }, 1500);
  }

  // 3D-model geometry summary (CPE-1334): independent of `provider.kind` — see MODEL_EXTS above.
  // `modelInfo` stays null (never an error state) whenever the backend can't parse the file, so a
  // corrupt/unsupported model just omits the section instead of showing an error toast.
  let modelInfo: ModelInfo | null = null;
  let modelReqId = 0;
  $: isModelFile = !!entry && !entry.is_dir && MODEL_EXTS.has(entry.extension);

  // Load text whenever the selected entry (for a text-based provider) changes.
  // A monotonically increasing request id discards any load superseded by a
  // newer selection.
  $: if (entry && needsText) loadTextFor(entry);
  $: if (entry && provider.kind === "archive") loadEntriesFor(entry);
  $: if (entry && provider.kind === "info") loadInfoFor(entry);
  $: if (entry && provider.kind === "decoded-image") loadImageDataFor(entry);
  $: if (entry && provider.kind === "raw-image") loadRawImageDataFor(entry);
  $: if (entry && provider.kind === "heic") loadHeicImageDataFor(entry);
  $: if (entry && provider.kind === "pdf") {
    loadPdfValidityFor(entry);
  } else {
    // Navigated away from a PDF (to a non-PDF, a folder, or nothing): disarm any in-flight PDF load so a
    // leftover 15s load-timeout can't later flip `pdfState` to "error" on a file we're no longer showing
    // (CPE-1363 audit #2). Idempotent, so re-running on unrelated reactive ticks is a no-op.
    cancelPdfLoad();
  }
  $: if (entry && provider.kind === "dicom") {
    loadDicomImageDataFor(entry);
    loadDicomTagsFor(entry);
  }
  $: if (entry && isModelFile) {
    loadModelFor(entry);
  } else {
    modelInfo = null;
    modelReqId++; // supersede any in-flight fetch for a file we've navigated away from
  }

  async function loadModelFor(e: DirEntry) {
    const mine = ++modelReqId;
    try {
      const result = unwrap(await commands.readModelInfo(e.path));
      if (mine !== modelReqId) return; // stale — selection moved on while this was in flight
      modelInfo = result;
    } catch {
      if (mine !== modelReqId) return;
      modelInfo = null; // never an error toast — the section is just omitted
    }
  }

  /** Bounding-box DIMENSIONS (width × height × depth), derived as max − min per axis from
   *  `[min_x,min_y,min_z,max_x,max_y,max_z]` — never the raw min/max themselves. All-zero means no
   *  vertices/accessor-extrema were read (an empty mesh, or a glTF with no POSITION min/max) — omit
   *  the row entirely rather than showing a misleading "0 × 0 × 0". */
  $: modelDims =
    modelInfo && modelInfo.bounding_box.some((v) => v !== 0)
      ? {
          w: modelInfo.bounding_box[3] - modelInfo.bounding_box[0],
          h: modelInfo.bounding_box[4] - modelInfo.bounding_box[1],
          d: modelInfo.bounding_box[5] - modelInfo.bounding_box[2],
        }
      : null;

  /** Trim a geometry float to at most 3 decimals without trailing zeros (2.500 → "2.5", 10 → "10"). */
  function fmtDim(n: number): string {
    return (Math.round(n * 1000) / 1000).toString();
  }

  $: modelFormatLabel =
    modelInfo?.format === "Obj"
      ? "OBJ"
      : modelInfo?.format === "Stl"
        ? "STL"
        : modelInfo?.format === "Gltf"
          ? "glTF"
          : modelInfo?.format === "Ply"
            ? "PLY"
            : "";
  // OBJ's and PLY's triangle_count is a FACE count (quads/n-gons aren't necessarily triangles) — label
  // it "Faces" for both; STL facets really are triangles, so "Triangles" is accurate there. glTF has no
  // triangle/face count at all (see ModelInfo.triangle_count doc) — it gets its own "Meshes" row
  // instead, so this label is never consulted for Gltf.
  $: modelCountLabel = modelInfo?.format === "Obj" || modelInfo?.format === "Ply" ? $t("pv.model.faces") : $t("pv.model.triangles");

  async function loadInfoFor(e: DirEntry) {
    const mine = ++infoReqId;
    infoState = "loading";
    try {
      const t = await loadInfo(e.path);
      if (mine !== infoReqId) return;
      info = t;
      infoState = "idle";
    } catch {
      if (mine !== infoReqId) return;
      infoState = "error";
    }
  }

  async function loadImageDataFor(e: DirEntry) {
    const mine = ++imgReqId;
    imgState = "loading";
    try {
      const url = await loadImageData(e.path);
      if (mine !== imgReqId) return;
      imgUrl = url;
      imgState = "idle";
    } catch {
      if (mine !== imgReqId) return;
      imgState = "error";
    }
  }

  /** No embedded JPEG (backend `Err`) lands in `rawImgState === "error"`, which the render side
   *  below shows as the metadata slot rather than an error message — a RAW file's info pane is
   *  useful on its own, unlike a genuinely corrupt tiff/psd. */
  async function loadRawImageDataFor(e: DirEntry) {
    const mine = ++rawImgReqId;
    rawImgState = "loading";
    try {
      const url = await loadRawImageData(e.path);
      if (mine !== rawImgReqId) return;
      rawImgUrl = url;
      rawImgState = "idle";
    } catch {
      if (mine !== rawImgReqId) return;
      rawImgState = "error";
    }
  }

  /** No decodable HEIC (backend `Err` — commonly a missing OS HEIF codec on Windows, or a corrupt
   *  file) lands in `heicImgState === "error"`, which the render side shows as the metadata slot
   *  rather than an error message — same clean fallback as the raw-image miss above. */
  async function loadHeicImageDataFor(e: DirEntry) {
    const mine = ++heicImgReqId;
    heicImgState = "loading";
    try {
      const url = await loadHeicImageData(e.path);
      if (mine !== heicImgReqId) return;
      heicImgUrl = url;
      heicImgState = "idle";
    } catch {
      if (mine !== heicImgReqId) return;
      heicImgState = "error";
    }
  }

  /** No decodable pixel data (backend `Err` — unsupported transfer syntax/corrupt) lands in
   *  `dicomImgState === "error"`, which the render side shows as the metadata slot rather than an
   *  error message, same as the raw-image miss above; the tag list (loaded separately) still shows
   *  if it came back. */
  async function loadDicomImageDataFor(e: DirEntry) {
    const mine = ++dicomImgReqId;
    dicomImgState = "loading";
    try {
      const url = await loadDicomImageData(e.path);
      if (mine !== dicomImgReqId) return;
      dicomImgUrl = url;
      dicomImgState = "idle";
    } catch {
      if (mine !== dicomImgReqId) return;
      dicomImgState = "error";
    }
  }

  /** Validates a PDF's structure BEFORE ever setting the iframe's `src` (CPE-1357): a rejected check
   *  (backend `Err` — no `%PDF-` header, no resolvable xref, or zero declared pages) lands in
   *  `pdfState === "error"`, which the render side shows as the metadata slot rather than handing the
   *  file to WebView2's PDF viewer at all — that viewer can crash the whole app on exactly this input. A
   *  passed check arms a load-timeout defense (see `onPdfIframeLoad`/`onPdfIframeError` below) for the
   *  iframe render that follows. */
  async function loadPdfValidityFor(e: DirEntry) {
    const mine = ++pdfReqId;
    pdfState = "loading";
    if (pdfLoadTimer !== undefined) {
      clearTimeout(pdfLoadTimer);
      pdfLoadTimer = undefined;
    }
    try {
      await loadPdfValidity(e.path);
      if (mine !== pdfReqId) return; // stale — selection moved on while this was in flight
      pdfState = "idle";
      // Defense in depth: even a structurally-valid PDF could still hang the WebView2 PDF plugin on
      // load (rather than error or crash outright). If neither `on:load` nor `on:error` fires in time,
      // fall back rather than leave the pane wedged on a dead iframe.
      pdfLoadTimer = setTimeout(() => {
        if (mine === pdfReqId) pdfState = "error";
      }, PDF_LOAD_TIMEOUT_MS);
    } catch {
      if (mine !== pdfReqId) return;
      pdfState = "error";
    }
  }

  /** The iframe loaded (or at least fired `load` — WebView2's PDF viewer does this once it has taken
   *  over rendering) — disarm the hang-detection timeout above. */
  function onPdfIframeLoad() {
    if (pdfLoadTimer !== undefined) {
      clearTimeout(pdfLoadTimer);
      pdfLoadTimer = undefined;
    }
  }

  /** The iframe itself reported an error (e.g. the asset protocol request failed) — disarm the timeout
   *  and fall back immediately rather than waiting it out. */
  function onPdfIframeError() {
    if (pdfLoadTimer !== undefined) {
      clearTimeout(pdfLoadTimer);
      pdfLoadTimer = undefined;
    }
    pdfState = "error";
  }

  /** Disarm any in-flight PDF validity check + armed load-timeout, and reset to idle. Called when the
   *  selection leaves a PDF and on teardown, so a leftover timer can't fire against a stale file
   *  (CPE-1363 audit #2). Idempotent: once there's no timer and state is already idle it does nothing, so
   *  re-running on unrelated reactive ticks is free. */
  function cancelPdfLoad() {
    if (pdfLoadTimer !== undefined) {
      clearTimeout(pdfLoadTimer);
      pdfLoadTimer = undefined;
    }
    if (pdfState !== "idle") {
      pdfReqId++; // supersede a pending loadPdfValidity so its late resolve is discarded
      pdfState = "idle";
    }
  }

  onDestroy(() => {
    if (pdfLoadTimer !== undefined) clearTimeout(pdfLoadTimer);
    if (lastActionTimer !== undefined) clearTimeout(lastActionTimer);
  });

  async function loadDicomTagsFor(e: DirEntry) {
    const mine = ++dicomTagsReqId;
    try {
      const tags = await loadDicomTags(e.path);
      if (mine !== dicomTagsReqId) return;
      dicomTags = tags;
    } catch {
      if (mine !== dicomTagsReqId) return;
      dicomTags = []; // never an error toast — the section is just omitted
    }
  }

  async function loadTextFor(e: DirEntry) {
    const mine = ++reqId;
    textState = "loading";
    try {
      const t = await loadText(e.path);
      if (mine !== reqId) return;
      text = t;
      textState = "idle";
    } catch {
      if (mine !== reqId) return;
      textState = "error";
    }
  }

  async function loadEntriesFor(e: DirEntry) {
    const mine = ++entryReqId;
    entriesState = "loading";
    try {
      const list = await loadEntries(e.path);
      if (mine !== entryReqId) return;
      entries = list;
      entriesState = "idle";
    } catch {
      if (mine !== entryReqId) return;
      entriesState = "error";
    }
  }

  // Pretty-print JSON, falling back to the raw text if it does not parse.
  function prettyJson(raw: string): string {
    try {
      return JSON.stringify(JSON.parse(raw), null, 2);
    } catch {
      return raw;
    }
  }

  $: tableRows =
    textState === "idle" && (provider.kind === "csv" || provider.kind === "tsv")
      ? parseCsv(text, provider.kind === "tsv" ? "\t" : ",")
      : [];

  // Async-rendered HTML for code (lazy grammar) and markdown (lazy renderer).
  let codeHtml = "";
  let mdHtml = "";
  let codeReq = 0;
  let mdReq = 0;

  $: if (entry && textState === "idle" && provider.kind === "text") {
    renderCode(entry.name, text);
  }
  $: if (entry && textState === "idle" && provider.kind === "markdown") {
    renderMd(text);
  }

  async function renderCode(name: string, src: string) {
    const mine = ++codeReq;
    codeHtml = highlightForFile(src, name); // escaped immediately
    const ok = await ensureLanguageForName(name);
    if (ok && mine === codeReq) codeHtml = highlightForFile(src, name); // now highlighted
  }

  async function renderMd(src: string) {
    const mine = ++mdReq;
    const html = await renderMarkdown(src);
    if (mine === mdReq) mdHtml = html;
  }

  // ---- code intelligence: outline strip + breadcrumb (CPE-1090) + per-line rows / fold / indent /
  //      minimap (CPE-1091), epic CPE-724. One codeIntel fetch feeds all of it. ----
  /** Tab width sent to codeIntel; also the column span of one indent guide. Matches the backend
   *  default (`code_intel` uses 4) so `indent[i]` levels line up with `TAB_WIDTH`-ch guides. */
  const TAB_WIDTH = 4;
  let outline: CodeSymbol[] = [];
  let folds: FoldRange[] = [];
  let indent: number[] = [];
  let minimap: MinimapRow[] = [];
  let outlineReq = 0;
  let breadcrumbSym: CodeSymbol | null = null;

  $: if (entry && textState === "idle" && provider.kind === "text") {
    loadCodeIntelFor(entry.name, text);
  } else if (!entry || provider.kind !== "text") {
    outline = [];
    folds = [];
    indent = [];
    minimap = [];
    breadcrumbSym = null;
    outlineReq++; // supersede any in-flight fetch for a file we've navigated away from
  }

  async function loadCodeIntelFor(name: string, src: string) {
    const mine = ++outlineReq;
    // Clear synchronously so a superseded fetch can never leave a stale (wrong-file) view visible
    // while the new one is in flight.
    outline = [];
    folds = [];
    indent = [];
    minimap = [];
    breadcrumbSym = null;
    if (!src) return;
    const lang = languageForName(name) ?? "";
    try {
      const result = await commands.codeIntel(src, lang, TAB_WIDTH, null);
      if (mine !== outlineReq) return;
      outline = result.outline;
      folds = result.folds;
      indent = result.indent;
      minimap = result.minimap;
      updateBreadcrumb();
      updateMinimapViewport();
    } catch {
      if (mine !== outlineReq) return;
      outline = [];
      folds = [];
      indent = [];
      minimap = [];
    }
  }

  // ---- per-line rows (CPE-1091) ----
  // The highlighted blob split into one span-safe HTML fragment per source line. Recomputed whenever the
  // highlight output changes (two-phase: escaped → highlighted). Only the code branch consumes it.
  $: codeLines = provider.kind === "text" ? splitHighlightedIntoLines(codeHtml) : [];
  // Gutter width in ch: enough for the largest line number, plus room for a fold triangle.
  $: gutterDigits = Math.max(2, String(codeLines.length).length);

  // ---- fold gutter (CPE-1091) ----
  // Foldable ranges keyed by their 1-based start line. Only ranges that actually cover ≥1 interior line
  // are foldable (start==end → nothing to collapse; division/empty-safe).
  $: foldByStart = new Map<number, FoldRange>(
    folds.filter((f) => f.end_line > f.start_line).map((f) => [f.start_line, f]),
  );
  /** Start lines the user has collapsed. Per-file; reset on entry change (see the lastPath effect). */
  let foldCollapsed = new Set<number>();
  // 1-based lines currently hidden by an active fold (start_line+1 .. end_line for each collapsed range).
  $: hiddenLines = (() => {
    const hidden = new Set<number>();
    for (const start of foldCollapsed) {
      const f = foldByStart.get(start);
      if (!f) continue;
      for (let ln = f.start_line + 1; ln <= f.end_line; ln++) hidden.add(ln);
    }
    return hidden;
  })();

  function toggleFold(startLine: number) {
    const next = new Set(foldCollapsed);
    if (next.has(startLine)) next.delete(startLine);
    else next.add(startLine);
    foldCollapsed = next;
  }
  /** Number of lines a collapsed range hides, for its "⋯ N lines" badge. */
  function foldLen(startLine: number): number {
    const f = foldByStart.get(startLine);
    return f ? f.end_line - f.start_line : 0;
  }

  /** Walk up from `el` to the nearest ancestor whose content actually overflows its box. The code
   *  `<pre>` has no `overflow` set (it grows to fit its content — see `.preview-text`), so the real
   *  scroll container is `aside.preview` (`overflow:auto`); walking generically avoids hard-coding that
   *  and keeps working if the CSS changes. */
  function findScrollContainer(el: HTMLElement | undefined): HTMLElement | undefined {
    let node: HTMLElement | null = el ?? null;
    while (node) {
      if (node.scrollHeight > node.clientHeight + 1) return node;
      node = node.parentElement;
    }
    return el;
  }

  function measureLineHeight(el: HTMLElement): number {
    const style = getComputedStyle(el);
    return resolveLineHeight(style.lineHeight, parseFloat(style.fontSize));
  }

  /** The `<pre>`'s top offset within the scroll container's scroll coordinate space. The container
   *  (`aside.preview`) scrolls the edit-bar + outline strip *above* the `<pre>`, so the first code line
   *  sits this many px down — jump/breadcrumb must add/subtract it or they drift by the header's height
   *  (which itself grows when the pills reflow). `getBoundingClientRect` is used rather than `offsetTop`
   *  because `aside.preview` is not a positioned `offsetParent`. */
  function preOffset(container: HTMLElement, pre: HTMLElement): number {
    return pre.getBoundingClientRect().top - container.getBoundingClientRect().top + container.scrollTop;
  }

  /** Click a symbol pill: scroll the preview so that symbol's line is at (or near) the top. Fold-aware
   *  (CPE-1095): if the target line is currently hidden inside a collapsed fold, expand that fold first
   *  so the row renders, then scroll to the row's *actual* rendered position
   *  (`getBoundingClientRect`) — exact regardless of any other collapsed fold above it or word-wrap,
   *  since a hidden row is removed from the flow entirely rather than just visually hidden. Falls back to
   *  the uniform-line-height math (corrected for any still-hidden lines above) only if the row still
   *  isn't rendered for some other reason. */
  async function jumpToSymbol(sym: CodeSymbol) {
    if (!textContentEl) return;
    const container = findScrollContainer(textContentEl);
    if (!container) return;

    const startToExpand = foldToExpand(sym.line, foldCollapsed, foldByStart);
    if (startToExpand !== null) {
      toggleFold(startToExpand);
      await tick(); // let the newly-unhidden rows render before we query/measure them
    }

    const lineHeight = measureLineHeight(textContentEl);
    const fallback =
      preOffset(container, textContentEl) +
      lineToScrollTop(sym.line - countHiddenAbove(sym.line, hiddenLines), lineHeight);
    const row = container.querySelector<HTMLElement>(`[data-line="${sym.line}"]`);
    const target = rowScrollTarget(
      row ? row.getBoundingClientRect() : null,
      container.getBoundingClientRect(),
      container.scrollTop,
      fallback,
    );
    container.scrollTop = Math.max(0, Math.min(target, container.scrollHeight));
    updateBreadcrumb();
  }

  /** Recompute the breadcrumb (the enclosing symbol of the top-visible line) from the live scroll
   *  position. Cheap: `outline` is small, and this is only a linear scan. Fold-aware (CPE-1095): the
   *  line-height math only knows "the Nth *rendered* row" — `lineAtVisibleRow` maps that back to the real
   *  source line so the breadcrumb stays correct while a fold above the viewport is collapsed. */
  function updateBreadcrumb() {
    if (!textContentEl || outline.length === 0) {
      breadcrumbSym = null;
      return;
    }
    const container = findScrollContainer(textContentEl);
    if (!container) return;
    const lineHeight = measureLineHeight(textContentEl);
    const visibleRow = scrollTopToLine(container.scrollTop - preOffset(container, textContentEl), lineHeight);
    const topLine = lineAtVisibleRow(visibleRow, codeLines.length, hiddenLines);
    breadcrumbSym = enclosingSymbol(outline, topLine);
  }

  /** Bound declaratively on `aside.preview` (the scroll container — see `findScrollContainer`), which
   *  is a fixed part of the markup rather than re-created per entry, so Svelte manages attach/detach for
   *  its whole lifetime; no manual addEventListener/removeEventListener bookkeeping needed. Guarded so it
   *  no-ops outside the code view (other preview kinds, or while editing). */
  function handlePreviewScroll() {
    if (provider.kind !== "text" || editing) return;
    if (outline.length > 0) updateBreadcrumb();
    updateMinimapViewport();
  }

  // ---- minimap (CPE-1091) ----
  // The viewport indicator box + click-to-scroll work purely off the scroll container's
  // scrollTop/scrollHeight/clientHeight ratios, so they stay correct under the wrap toggle too (no
  // uniform-line-height assumption). Division-safe: a zero/unmeasurable scrollHeight fills the box.
  let mmViewport = { top: 0, height: 100 }; // percentages of the minimap height
  let mmHeightPx = 0; // minimap pixel height = the visible pane height (the minimap overlays the viewport)
  function updateMinimapViewport() {
    if (!textContentEl || minimap.length === 0) return;
    const container = findScrollContainer(textContentEl);
    if (!container) return;
    mmHeightPx = container.clientHeight;
    const sh = container.scrollHeight;
    if (!Number.isFinite(sh) || sh <= 0) {
      mmViewport = { top: 0, height: 100 };
      return;
    }
    const top = Math.max(0, Math.min(100, (container.scrollTop / sh) * 100));
    const height = Math.max(2, Math.min(100, (container.clientHeight / sh) * 100));
    mmViewport = { top, height };
  }
  // Re-measure the minimap after new rows paint or the window resizes (guarded — no-ops before mount /
  // outside the code view). `tick()` waits for the DOM so scrollHeight/clientHeight are real.
  $: if (minimap.length && codeLines.length) tick().then(updateMinimapViewport);

  /** Click (or drag) on the minimap: scroll the pane so the clicked fraction of the file is centred. */
  function onMinimapPoint(e: MouseEvent) {
    if (!textContentEl) return;
    const container = findScrollContainer(textContentEl);
    if (!container) return;
    const rail = e.currentTarget as HTMLElement;
    const rect = rail.getBoundingClientRect();
    if (rect.height <= 0) return;
    const frac = Math.max(0, Math.min(1, (e.clientY - rect.top) / rect.height));
    const target = frac * container.scrollHeight - container.clientHeight / 2;
    container.scrollTop = Math.max(0, Math.min(target, container.scrollHeight));
    updateMinimapViewport();
    updateBreadcrumb();
  }
  function onMinimapDown(e: MouseEvent) {
    onMinimapPoint(e);
    const move = (ev: MouseEvent) => onMinimapPoint(ev);
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  // ---- word wrap (CPE-565): wrap long lines in the text/code/json preview; remembered across files. ----
  // Default ON (the pane has always wrapped); only an explicit "0" turns wrapping off.
  const WRAP_KEY = "cpe.previewWrap";
  let wrapLines = lsBool(WRAP_KEY, true);
  function toggleWrap() {
    wrapLines = !wrapLines;
    lsSet(WRAP_KEY, wrapLines ? "1" : "0");
  }
  // The `<pre>`-rendered previews where wrapping applies (JSON + text/code); markdown + tables don't.
  $: isPreText = provider.kind === "json" || provider.kind === "text";

  // ---- JSON tree view (CPE-1573, epic CPE-1568) ----
  // The collapsible tree is the default view; a toggle in the edit bar switches to the plain
  // pretty-printed raw text (the pane's pre-CPE-1573 behavior), remembered like word-wrap above.
  const JSON_VIEW_KEY = "cpe.jsonTreeView";
  let jsonTreeView = lsBool(JSON_VIEW_KEY, true);
  function setJsonView(tree: boolean) {
    jsonTreeView = tree;
    lsSet(JSON_VIEW_KEY, tree ? "1" : "0");
  }

  // ---- editing ----
  let editing = false;
  let draft = "";
  let saving = false;
  let saveError = "";
  let lastPath = "";

  $: dirty = draft !== text;

  // Leave edit mode (without saving) whenever the selected file changes.
  $: if (entry && entry.path !== lastPath) {
    lastPath = entry.path;
    editing = false;
    saveError = "";
    foldCollapsed = new Set(); // fold state is per-file — never carry a collapse across files (CPE-1091)
  }

  function startEdit() {
    draft = text;
    saveError = "";
    editing = true;
  }

  function cancelEdit() {
    editing = false;
    saveError = "";
  }

  async function save() {
    if (!entry || !dirty || saving) return;
    saving = true;
    saveError = "";
    try {
      await saveText(entry.path, draft);
      text = draft;
      editing = false;
    } catch {
      saveError = "Couldn't save the file.";
    } finally {
      saving = false;
    }
  }

  function onEditorKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      save();
    }
  }

  // ---- text context menu (Cut / Copy / Paste / Select All) ----
  let editorEl: HTMLTextAreaElement | undefined;
  let textContentEl: HTMLElement | undefined;
  let ctxMenu: { x: number; y: number } | null = null;

  // Cut/Paste only apply while editing (view is read-only). The menu items are
  // always present; these flags disable them.
  $: canModify = editing;

  $: isTextKind =
    provider.kind === "text" ||
    provider.kind === "markdown" ||
    provider.kind === "json" ||
    provider.kind === "csv" ||
    provider.kind === "tsv";

  function openTextMenu(e: MouseEvent) {
    if (!isTextKind || textState !== "idle") return; // let non-text use the native menu
    e.preventDefault();
    ctxMenu = { x: e.clientX, y: e.clientY };
  }
  function closeTextMenu() {
    ctxMenu = null;
  }

  /** Currently selected text — from the editor when editing, else the window selection. */
  function selectedText(): string {
    if (editing && editorEl) {
      return draft.slice(editorEl.selectionStart, editorEl.selectionEnd);
    }
    return window.getSelection()?.toString() ?? "";
  }

  async function menuCopy() {
    const sel = selectedText();
    const all = editing ? draft : text;
    try {
      await navigator.clipboard.writeText(sel || all);
    } catch {
      /* clipboard unavailable — ignore */
    }
    closeTextMenu();
  }

  async function menuCut() {
    if (!editing || !editorEl) return closeTextMenu();
    const start = editorEl.selectionStart;
    const end = editorEl.selectionEnd;
    const cut = draft.slice(start, end);
    if (cut) {
      try {
        await navigator.clipboard.writeText(cut);
      } catch {
        /* ignore */
      }
      draft = draft.slice(0, start) + draft.slice(end);
      await tick();
      editorEl.focus();
      editorEl.setSelectionRange(start, start);
    }
    closeTextMenu();
  }

  async function menuPaste() {
    if (!editing || !editorEl) return closeTextMenu();
    let clip = "";
    try {
      clip = await navigator.clipboard.readText();
    } catch {
      return closeTextMenu();
    }
    const start = editorEl.selectionStart;
    const end = editorEl.selectionEnd;
    draft = draft.slice(0, start) + clip + draft.slice(end);
    await tick();
    editorEl.focus();
    const caret = start + clip.length;
    editorEl.setSelectionRange(caret, caret);
    closeTextMenu();
  }

  function menuSelectAll() {
    if (editing && editorEl) {
      editorEl.focus();
      editorEl.select();
    } else if (textContentEl) {
      const range = document.createRange();
      range.selectNodeContents(textContentEl);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
    }
    closeTextMenu();
  }
</script>

<svelte:window on:click={closeTextMenu} on:keydown={(e) => e.key === "Escape" && closeTextMenu()} on:resize={updateMinimapViewport} />

<!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
<aside class="preview" on:contextmenu={openTextMenu} on:scroll={handlePreviewScroll}>
  {#if actions.length > 0}
    <!-- Generic provider action bar (CPE-1570, epic CPE-1568): renders whatever the current provider
         declares in `actions`, styled like `.preview-edit-bar` per MENUS.md (theme-only colors). -->
    <div class="preview-action-bar" data-testid="preview-action-bar">
      {#each actions as action (action.id)}
        <button class="editbtn actionbtn" on:click={() => runAction(action)} title={$t(action.labelKey)}>
          <Icon name={lastActionId === action.id ? "check" : action.icon} size={14} />
          {$t(action.labelKey)}
        </button>
      {/each}
      {#if actionMessage}
        <!-- Transient status from an action that needs to surface more than a checkmark (e.g. JSON
             Validate's parse result, CPE-1573). -->
        <span class="action-msg" class:err={actionMessageIsError} data-testid="preview-action-message">
          {actionMessage}
        </span>
      {/if}
    </div>
  {/if}
  {#if modelInfo}
    <div class="model-info" data-testid="model-info-section">
      <div class="model-info-title">{$t("pv.model.title")}</div>
      <div class="model-info-rows">
        <div class="model-row"><span class="model-k">{$t("pv.model.format")}</span><span class="model-v">{modelFormatLabel}</span></div>
        {#if modelInfo.format === "Stl"}
          <div class="model-row">
            <span class="model-k">{$t("pv.model.encoding")}</span>
            <span class="model-v">{modelInfo.ascii ? $t("pv.model.ascii") : $t("pv.model.binary")}</span>
          </div>
        {/if}
        {#if modelInfo.format === "Gltf"}
          <div class="model-row"><span class="model-k">{$t("pv.model.meshes")}</span><span class="model-v">{modelInfo.mesh_count.toLocaleString()}</span></div>
        {:else}
          <div class="model-row"><span class="model-k">{modelCountLabel}</span><span class="model-v">{modelInfo.triangle_count.toLocaleString()}</span></div>
          <div class="model-row"><span class="model-k">{$t("pv.model.vertices")}</span><span class="model-v">{modelInfo.vertex_count.toLocaleString()}</span></div>
        {/if}
        {#if modelDims}
          <div class="model-row">
            <span class="model-k">{$t("pv.model.dimensions")}</span>
            <span class="model-v">{fmtDim(modelDims.w)} × {fmtDim(modelDims.h)} × {fmtDim(modelDims.d)}</span>
          </div>
        {/if}
      </div>
    </div>
  {/if}
  {#if provider.kind === "dicom" && dicomTags.length > 0}
    <div class="model-info" data-testid="dicom-tags-section">
      <div class="model-info-title">{$t("pv.dicom.title")}</div>
      <div class="model-info-rows">
        {#each dicomTags as [name, value]}
          <div class="model-row"><span class="model-k">{name}</span><span class="model-v">{value}</span></div>
        {/each}
      </div>
    </div>
  {/if}
  {#if provider.kind === "image" && entry}
    <img class="preview-img" src={assetUrl(entry.path)} alt={entry.name} />
  {:else if provider.kind === "decoded-image" && entry}
    {#if imgState === "loading"}
      <p class="preview-note">{$t("pv.loading")}</p>
    {:else if imgState === "error"}
      <p class="preview-note">{$t("pv.cantImage")}</p>
    {:else}
      <img class="preview-img" src={imgUrl} alt={entry.name} />
    {/if}
  {:else if provider.kind === "raw-image" && entry}
    {#if rawImgState === "loading"}
      <p class="preview-note">{$t("pv.loading")}</p>
    {:else if rawImgState === "error"}
      <slot />
    {:else}
      <img class="preview-img" src={rawImgUrl} alt={entry.name} />
    {/if}
  {:else if provider.kind === "heic" && entry}
    {#if heicImgState === "loading"}
      <p class="preview-note">{$t("pv.loading")}</p>
    {:else if heicImgState === "error"}
      <slot />
    {:else}
      <img class="preview-img" src={heicImgUrl} alt={entry.name} />
    {/if}
  {:else if provider.kind === "dicom" && entry}
    {#if dicomImgState === "loading"}
      <p class="preview-note">{$t("pv.loading")}</p>
    {:else if dicomImgState === "error"}
      <slot />
    {:else}
      <img class="preview-img" src={dicomImgUrl} alt={entry.name} />
    {/if}
  {:else if provider.kind === "media" && entry}
    <!-- Temporal media (CPE-1429, epic CPE-720): a native <audio>/<video> element fed by the asset
         protocol behind a custom themed transport, with an open-externally fallback on an unsupported
         codec. `{#key}` remounts the player per file so its transport state never leaks across clips. -->
    {#key entry.path}
      <MediaPlayer
        src={assetUrl(entry.path)}
        type={mediaType(entry)}
        name={entry.name}
        openExternal={() => openExternal(entry.path)}
      />
    {/key}
  {:else if provider.kind === "pdf" && entry}
    {#if pdfState === "loading"}
      <p class="preview-note">{$t("pv.loading")}</p>
    {:else if pdfState === "error"}
      <slot />
    {:else}
      <!-- No `sandbox` attribute: WebView2/Chromium render PDFs through the MimeHandlerView plugin
           (the built-in PDF viewer), which a sandboxed iframe disables outright — the pane went blank
           on valid PDFs (CPE-1362). Crash-safety is preserved without it: the `pdfState` gate above
           only reaches this branch once the backend `read_pdf_validity` check passes, and the load
           timeout + `on:error` handler still catch a valid-looking PDF that hangs the plugin. -->
      <iframe
        class="preview-pdf"
        title={entry.name}
        src={assetUrl(entry.path)}
        on:load={onPdfIframeLoad}
        on:error={onPdfIframeError}
      ></iframe>
    {/if}
  {:else if provider.kind === "font" && entry}
    <!-- Font specimen + glyph grid + metadata (CPE-1586, epic CPE-1568 slice 5): self-contained like
         JwtPreview/CertPreview — reports its selected glyph's copyable values up via onValues. -->
    <FontPreview path={entry.path} extension={entry.extension} size={entry.size} {assetUrl} onValues={onFontValues} />
  {:else if provider.kind === "archive" && entry}
    {#if entriesState === "loading"}
      <p class="preview-note">{$t("pv.loading")}</p>
    {:else if entriesState === "error"}
      <p class="preview-note">{$t("pv.cantArchive")}</p>
    {:else}
      <div class="preview-table-wrap">
        <table class="preview-table">
          <tbody>
            {#each entries as e}
              <tr>
                <td>{e.name}</td>
                <td class="num">{e.is_dir ? "" : formatSize(e.size)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        <p class="preview-note">{entries.length === 1 ? $t("pv.itemOne", { count: entries.length }) : $t("pv.itemMany", { count: entries.length })}</p>
      </div>
    {/if}
  {:else if provider.kind === "info" && entry}
    {#if infoState === "loading"}
      <p class="preview-note">{$t("pv.loading")}</p>
    {:else if infoState === "error"}
      <p class="preview-note">{$t("pv.cantFile")}</p>
    {:else}
      <pre class="preview-text" bind:this={textContentEl}>{info}</pre>
    {/if}
  {:else if provider.kind === "binary" && entry}
    <!-- Binary Inspector (CPE-1597, epic CPE-1562 slice 4; CPE-1615 slice 5): self-contained like
         FontPreview/CertPreview above — fetches its own data from `path`, no declared action-bar actions.
         No `extension` prop (CPE-1615 retired the frontend's extension-keyed managed-.NET heuristic in
         favour of the backend's real `info.is_managed` flag). -->
    <BinaryPreview path={entry.path} size={entry.size} />
  {:else if provider.kind === "hex" && entry}
    <HexView path={entry.path} size={entry.size} />
  {:else if provider.kind === "data-grid" && entry}
    <DataBrowser {entry} />
  {:else if provider.kind === "email" && entry}
    <EmailPreview path={entry.path} />
  {:else if provider.kind === "calendar" && entry}
    <IcalPreview path={entry.path} />
  {:else if provider.kind === "vcard" && entry}
    <VcardPreview path={entry.path} />
  {:else if provider.kind === "jwt" && entry}
    <JwtPreview path={entry.path} onValues={onJwtValues} />
  {:else if provider.kind === "cert" && entry}
    <CertPreview path={entry.path} />
  {:else if provider.kind === "notebook" && entry}
    <!-- Jupyter notebook viewer (CPE-1616, epic CPE-1568 slice 6): self-contained like CertPreview/
         FontPreview above — fetches + parses its own content from `path`, no declared action-bar actions. -->
    <NotebookPreview path={entry.path} />
  {:else if provider.kind === "folder" && entry}
    <!-- Folder peek (CPE-1426): browsable one-level-down listing; row clicks bubble up to the app via
         Svelte's bare `on:` forwarding (this component has no other listeners to conflict with). -->
    <FolderBrowser path={entry.path} on:pick on:open />
  {:else if needsText && entry}
    {#if textState === "loading"}
      <p class="preview-note">{$t("pv.loading")}</p>
    {:else if textState === "error"}
      <p class="preview-note">{$t("pv.cantFile")}</p>
    {:else if editing}
      <div class="preview-edit-bar">
        <button class="editbtn primary" disabled={!dirty || saving} on:click={save}
          >{saving ? $t("pv.saving") : $t("pv.save")}</button>
        <button class="editbtn" on:click={cancelEdit}>{$t("common.cancel")}</button>
        {#if saveError}<span class="edit-err">{saveError}</span>{/if}
      </div>
      <textarea
        class="preview-editor"
        bind:value={draft}
        bind:this={editorEl}
        on:keydown={onEditorKeydown}
        spellcheck="false"
      ></textarea>
    {:else}
      {#if provider.editable}
        <div class="preview-edit-bar">
          {#if provider.kind === "json"}
            <!-- Tree/Raw view toggle (CPE-1573, epic CPE-1568): the collapsible tree is the default;
                 Raw keeps the pre-CPE-1573 pretty-printed `<pre>` available. -->
            <button
              class="editbtn"
              class:on={jsonTreeView}
              aria-pressed={jsonTreeView}
              on:click={() => setJsonView(true)}>{$t("pv.json.viewTree")}</button>
            <button
              class="editbtn"
              class:on={!jsonTreeView}
              aria-pressed={!jsonTreeView}
              on:click={() => setJsonView(false)}>{$t("pv.json.viewRaw")}</button>
          {/if}
          {#if isPreText && !(provider.kind === "json" && jsonTreeView)}
            <button class="editbtn wrapbtn" class:on={wrapLines} title="Wrap long lines" aria-label="Wrap long lines" aria-pressed={wrapLines} on:click={toggleWrap}>↩</button>
          {/if}
          <button class="editbtn" on:click={startEdit}>{$t("pv.edit")}</button>
        </div>
      {/if}
      {#if provider.kind === "csv" || provider.kind === "tsv"}
        <div class="preview-table-wrap" bind:this={textContentEl}>
          <table class="preview-table">
            <tbody>
              {#each tableRows.slice(0, CSV_ROW_CAP) as r}
                <tr>{#each r as cell}<td>{cell}</td>{/each}</tr>
              {/each}
            </tbody>
          </table>
          {#if tableRows.length > CSV_ROW_CAP}
            <p class="preview-note">{$t("pv.showingRows", { cap: CSV_ROW_CAP, total: tableRows.length })}</p>
          {/if}
        </div>
      {:else if provider.kind === "json"}
        {#if jsonTreeView}
          <!-- Collapsible tree (CPE-1573, epic CPE-1568) — {#key} remounts per file so a previous
               file's expand/collapse state never leaks into the next one. -->
          {#key entry?.path}
            <JsonTree text={text} command={jsonCommand} onValues={onJsonValues} />
          {/key}
        {:else}
          <pre class="preview-text" class:nowrap={!wrapLines} bind:this={textContentEl}>{prettyJson(text)}</pre>
        {/if}
      {:else if provider.kind === "markdown"}
        <!-- mdHtml is DOMPurify-sanitized (lazy renderer), safe to inject. -->
        <div class="preview-markdown" bind:this={textContentEl}>{@html mdHtml}</div>
      {:else}
        {#if outline.length > 0}
          <!-- Symbol outline strip + breadcrumb (CPE-1090): additive UI over the outline; jumps index
               into the per-line rows below via the same scroll math. -->
          <div class="outline-bar">
            {#if breadcrumbSym}
              <div class="outline-crumb">
                <Icon name={breadcrumbSym.kind} size={12} />
                <span class="outline-crumb-name">{breadcrumbSym.name}</span>
              </div>
            {/if}
            <div class="outline-pills">
              {#each outline as sym (sym.line + ":" + sym.name)}
                <button
                  type="button"
                  class="outline-pill"
                  class:active={breadcrumbSym === sym}
                  aria-label={`Jump to ${sym.kind} ${sym.name}, line ${sym.line}`}
                  title={`${sym.name} — line ${sym.line}`}
                  on:click={() => jumpToSymbol(sym)}
                >
                  <Icon name={sym.kind} size={12} />
                  <span class="outline-pill-name">{sym.name}</span>
                </button>
              {/each}
            </div>
          </div>
        {/if}
        <!-- Per-line code rows (CPE-1091): one span-safe row per source line inside ONE <pre>/<code>, so
             select-all/copy still grabs the whole file as one blob. Line numbers come from CSS ::before
             (never part of the selection); the fold triangle is a text-free button (CSS glyph). codeHtml
             is escaped-or-hljs output (lazy grammar); each split fragment is balanced HTML, safe to inject.
             Whitespace inside <code> is significant (white-space:pre) — keep the rows compact. -->
        <div class="code-view">
          <pre
            class="preview-text code-rows"
            class:nowrap={!wrapLines}
            style="--gutter: calc({gutterDigits}ch + 22px); --tab-ch: {TAB_WIDTH}ch"
          ><code class="cl-code" bind:this={textContentEl}>{#each codeLines as line, i}{#if !hiddenLines.has(i + 1)}<span class="cl-row" class:foldable={foldByStart.has(i + 1)} data-line={i + 1} style="--indent: {indent[i] ?? 0}">{#if foldByStart.has(i + 1)}<button type="button" class="fold-toggle" class:collapsed={foldCollapsed.has(i + 1)} aria-label={foldCollapsed.has(i + 1) ? `Expand lines ${i + 1}–${foldByStart.get(i + 1)?.end_line}` : `Collapse lines ${i + 1}–${foldByStart.get(i + 1)?.end_line}`} aria-expanded={!foldCollapsed.has(i + 1)} tabindex="-1" on:click|stopPropagation={() => toggleFold(i + 1)}></button>{/if}{@html line}{#if foldCollapsed.has(i + 1)}<span class="fold-badge"> ⋯ {foldLen(i + 1)} lines</span>{/if}</span>{/if}{/each}</code></pre>
          {#if minimap.length > 0}
            <!-- svelte-ignore a11y-no-static-element-interactions -->
            <div class="minimap" role="presentation" style="height: {mmHeightPx}px" on:mousedown|preventDefault={onMinimapDown}>
              {#each minimap as row}
                <div class="mm-row"><div class="mm-fill" style="width: {6 + (row.fill / 255) * 90}%; margin-left: {Math.min(row.indent * 6, 60)}%; opacity: {0.18 + (row.fill / 255) * 0.82}"></div></div>
              {/each}
              <div class="mm-viewport" style="top: {mmViewport.top}%; height: {mmViewport.height}%"></div>
            </div>
          {/if}
        </div>
      {/if}
    {/if}
  {:else}
    <slot />
  {/if}
</aside>

{#if ctxMenu}
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
  <div
    class="text-ctx"
    role="menu"
    tabindex="-1"
    style="left:{ctxMenu.x}px; top:{ctxMenu.y}px"
    on:click|stopPropagation
    on:contextmenu|preventDefault|stopPropagation
  >
    <button role="menuitem" disabled={!canModify} on:click={menuCut}>{$t("menu.cut")}</button>
    <button role="menuitem" on:click={menuCopy}>{$t("menu.copy")}</button>
    <button role="menuitem" disabled={!canModify} on:click={menuPaste}>{$t("menu.paste")}</button>
    <div class="text-ctx-sep"></div>
    <button role="menuitem" on:click={menuSelectAll}>{$t("ctx.selectAll")}</button>
  </div>
{/if}

<style>
  .preview {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: auto;
  }
  .preview-img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    margin: auto;
  }
  .preview-pdf {
    flex: 1;
    width: 100%;
    border: none;
  }
  .preview-text {
    margin: 0;
    padding: 12px;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--mono, ui-monospace, monospace);
    font-size: 12px;
  }
  /* Wrap toggle (CPE-565): off = preserve code indentation with horizontal scroll. */
  .preview-text.nowrap { white-space: pre; word-break: normal; overflow-x: auto; }

  /* ---- Per-line code rows + gutter/fold/indent/minimap (CPE-1091, epic CPE-724) ---- */
  .code-view { position: relative; display: flex; align-items: flex-start; }
  .code-rows { flex: 1 1 auto; min-width: 0; padding: 12px 0; }
  .cl-code { display: block; line-height: 1.5; }
  /* One block per source line so line numbers, fold toggle and indent guides align to the row and the
     minimap/outline scroll math sees a uniform line height. The whole <code> stays one selection blob,
     so select-all/copy yields the file (block rows copy newline-joined). */
  .cl-row {
    display: block;
    position: relative;
    padding-left: var(--gutter);
    min-height: 1.5em; /* keep empty lines a full row tall (division-safe: no 0-height rows) */
    /* Indent guides: `--indent` faint vertical rules, one every --tab-ch columns, starting after the
       gutter. `--indent: 0` → zero background width → nothing drawn. */
    background-image: repeating-linear-gradient(
      to right,
      var(--border) 0, var(--border) 1px,
      transparent 1px, transparent var(--tab-ch)
    );
    background-repeat: no-repeat;
    background-position: var(--gutter) 0;
    background-size: calc(var(--indent, 0) * var(--tab-ch)) 100%;
  }
  /* Line number in the gutter via generated content — NOT a DOM text node, so it is never part of a
     selection/copy. Right-aligned in the gutter, dimmed, not selectable. */
  .cl-row::before {
    content: attr(data-line);
    position: absolute;
    left: 0;
    width: calc(var(--gutter) - 22px);
    text-align: right;
    color: var(--text-faint);
    user-select: none;
    -webkit-user-select: none;
    pointer-events: none;
  }
  /* Fold triangle: a text-free button (glyph drawn in CSS) so it never lands in copied text. */
  .fold-toggle {
    position: absolute;
    left: calc(var(--gutter) - 18px);
    top: 0;
    width: 14px;
    height: 1.5em;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-dim);
    user-select: none;
    -webkit-user-select: none;
  }
  .fold-toggle::before {
    content: "▾";
    font-size: 10px;
    line-height: 1;
    transition: transform 0.1s ease;
  }
  .fold-toggle.collapsed::before { transform: rotate(-90deg); }
  .fold-toggle:hover { color: var(--text); }
  .fold-badge {
    margin-left: 8px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text-dim);
    font-size: 11px;
    user-select: none;
    -webkit-user-select: none;
  }
  /* Minimap: a downsampled overview pinned to the right of the viewport (sticky), whose bars come from
     MinimapRow[] and whose viewport box + click-scroll use pure scroll fractions (wrap-safe). */
  .minimap {
    position: sticky;
    top: 0;
    flex: 0 0 auto;
    width: 64px;
    display: flex;
    flex-direction: column;
    white-space: normal; /* escape the inherited pre whitespace */
    background: var(--surface);
    border-left: 1px solid var(--border);
    cursor: pointer;
    overflow: hidden;
  }
  .mm-row { flex: 1 1 0; display: flex; align-items: center; min-height: 0; }
  .mm-fill { height: 60%; min-height: 1px; background: var(--text-dim); border-radius: 1px; }
  .mm-viewport {
    position: absolute;
    left: 0;
    right: 0;
    background: var(--accent);
    opacity: 0.16;
    border: 1px solid var(--accent);
    pointer-events: none;
  }
  /* 3D-model geometry summary (CPE-1334): additive header above whatever the underlying provider
     renders (hex/etc.) for these binary formats — the metadata-pane fallback for epic CPE-118. */
  .model-info {
    flex: none;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .model-info-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim);
    margin-bottom: 6px;
  }
  .model-info-rows { display: flex; flex-direction: column; gap: 4px; }
  .model-row { display: flex; gap: 10px; font-size: 12px; }
  .model-k { color: var(--text-dim); width: 90px; flex: none; }
  .model-v { color: var(--text); overflow-wrap: anywhere; }
  /* Symbol outline strip + breadcrumb (CPE-1090): additive header above the code <pre>. */
  .outline-bar {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    flex: none;
  }
  .outline-crumb {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    font-size: 11px;
    color: var(--text-dim);
  }
  .outline-crumb-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Tick-tacks rule: the row reflows (wraps onto more rows + grows height); each pill keeps its own
     text on one line and never shrinks. */
  .outline-pills {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .outline-pill {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 0 0 auto;
    max-width: 220px;
    padding: 2px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-size: 11px;
    line-height: 1.6;
  }
  .outline-pill-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .outline-pill.active {
    border-color: var(--accent);
  }
  .outline-pill:hover {
    border-color: var(--border-strong);
  }
  .wrapbtn { font-size: 13px; line-height: 1; min-width: 26px; }
  .editbtn.on { background: var(--accent); color: #fff; border-color: var(--accent); }
  .preview-table-wrap {
    overflow: auto;
    padding: 8px;
  }
  .preview-table {
    border-collapse: collapse;
    font-size: 12px;
  }
  .preview-table td {
    border: 1px solid var(--border);
    padding: 2px 6px;
    white-space: nowrap;
  }
  .preview-table td.num { text-align: right; color: var(--text-dim); }
  .preview-note {
    margin: auto;
    color: var(--text-faint);
    padding: 12px;
  }
  .text-ctx {
    position: fixed;
    z-index: 100;
    min-width: 160px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.16);
  }
  .text-ctx button {
    display: flex;
    align-items: center;
    width: 100%;
    height: 30px;
    padding: 0 12px;
    text-align: left;
    border-radius: var(--radius);
    font-size: 13px;
  }
  .text-ctx button:disabled { opacity: 0.4; cursor: default; }
  .text-ctx-sep { height: 1px; background: var(--border); margin: 4px 6px; }
  /* Generic provider action bar (CPE-1570, epic CPE-1568): same shape as the edit bar it sits beside —
     item text stays `var(--text)` (inherited via .editbtn, no hard-coded colour) per MENUS.md. */
  .preview-edit-bar,
  .preview-action-bar {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .editbtn {
    padding: 4px 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    font-size: 12px;
  }
  /* Action-bar buttons pair an Icon glyph with their label (the plain edit-bar buttons are text-only). */
  .actionbtn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .editbtn.primary { background: var(--accent); color: #fff; border-color: var(--accent); }
  .editbtn:disabled { opacity: 0.5; }
  .edit-err { color: var(--danger); font-size: 12px; }
  /* Transient action-bar status message (CPE-1573) — e.g. JSON Validate's result. Theme-only colours
     (MENUS.md): neutral text by default, `--danger` only for an actual error. */
  .action-msg { font-size: 12px; color: var(--text-dim); }
  .action-msg.err { color: var(--danger); }
  .preview-editor {
    flex: 1;
    width: 100%;
    resize: none;
    border: none;
    outline: none;
    padding: 12px;
    font-family: var(--mono, ui-monospace, monospace);
    font-size: 12px;
    line-height: 1.5;
    color: var(--text);
    background: var(--surface);
    tab-size: 2;
  }
  .preview-markdown {
    padding: 12px 16px;
    font-size: 13px;
    line-height: 1.5;
    overflow-wrap: anywhere;
  }
  .preview-markdown :global(h1),
  .preview-markdown :global(h2),
  .preview-markdown :global(h3) { margin: 0.6em 0 0.3em; }
  .preview-markdown :global(p) { margin: 0.5em 0; }
  .preview-markdown :global(pre) {
    background: var(--surface-alt);
    padding: 8px;
    border-radius: var(--radius);
    overflow-x: auto;
  }
  .preview-markdown :global(code) { font-family: var(--mono, ui-monospace, monospace); }
  .preview-markdown :global(a) { color: var(--accent); }
</style>
