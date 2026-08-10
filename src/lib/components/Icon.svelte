<script lang="ts">
  /** Named glyph. Covers file categories, sidebar places, and toolbar actions. */
  export let name: string;
  export let size = 16;
</script>

<svg
  class="icon"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if name === "folder"}
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"
          fill="#ffd166" stroke="#e0a800" />
  {:else if name === "image"}
    <rect x="3" y="4" width="18" height="16" rx="2" fill="#e6f4ea" stroke="#2e7d43" />
    <circle cx="8.5" cy="9.5" r="1.5" fill="#2e7d43" stroke="none" />
    <path d="M4 17l5-5 4 4 3-2 4 4" stroke="#2e7d43" />
  {:else if name === "document"}
    <path d="M6 3h8l4 4v14H6z" fill="#e7effc" stroke="#2b579a" />
    <path d="M14 3v4h4" stroke="#2b579a" />
    <path d="M9 12h6M9 15h6M9 18h4" stroke="#2b579a" />
  {:else if name === "spreadsheet"}
    <path d="M6 3h8l4 4v14H6z" fill="#e6f4ea" stroke="#1e7145" />
    <path d="M14 3v4h4" stroke="#1e7145" />
    <path d="M9 12h9M9 16h9M12 11v9" stroke="#1e7145" />
  {:else if name === "presentation"}
    <path d="M6 3h8l4 4v14H6z" fill="#fdece7" stroke="#c43e1c" />
    <path d="M14 3v4h4" stroke="#c43e1c" />
    <rect x="9" y="12" width="7" height="5" stroke="#c43e1c" />
  {:else if name === "pdf"}
    <path d="M6 3h8l4 4v14H6z" fill="#fdeaea" stroke="#c42b1c" />
    <path d="M14 3v4h4" stroke="#c42b1c" />
    <path d="M9 16v-4h1.5a1.5 1.5 0 0 1 0 3H9M14 16v-4h2" stroke="#c42b1c" />
  {:else if name === "code"}
    <path d="M6 3h8l4 4v14H6z" fill="#eef1f5" stroke="#5f6b7a" />
    <path d="M14 3v4h4" stroke="#5f6b7a" />
    <path d="M10.5 12L9 14l1.5 2M13.5 12L15 14l-1.5 2" stroke="#5f6b7a" />
  {:else if name === "archive"}
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"
          fill="#f3e8d2" stroke="#9a7b3f" />
    <path d="M12 9v2M12 13v2" stroke="#9a7b3f" />
  {:else if name === "audio"}
    <path d="M6 3h8l4 4v14H6z" fill="#f3e8fb" stroke="#7719aa" />
    <path d="M14 3v4h4" stroke="#7719aa" />
    <path d="M10 17v-4l4-1v4" stroke="#7719aa" />
    <circle cx="9" cy="17" r="1.2" fill="#7719aa" stroke="none" />
    <circle cx="13" cy="16" r="1.2" fill="#7719aa" stroke="none" />
  {:else if name === "video"}
    <rect x="3" y="5" width="18" height="14" rx="2" fill="#e7effc" stroke="#1f5eb5" />
    <path d="M10 9l5 3-5 3z" fill="#1f5eb5" stroke="none" />
  {:else if name === "text"}
    <path d="M6 3h8l4 4v14H6z" fill="#f5f5f5" stroke="#6b6b6b" />
    <path d="M14 3v4h4" stroke="#6b6b6b" />
    <path d="M9 12h6M9 15h6M9 18h3" stroke="#6b6b6b" />
  {:else if name === "executable"}
    <rect x="3" y="5" width="18" height="14" rx="2" fill="#eae7fb" stroke="#5b3fd6" />
    <path d="M3 9h18" stroke="#5b3fd6" />
    <path d="M7 12l2.5 2L7 16M12 16h4" stroke="#5b3fd6" />
  {:else if name === "font"}
    <path d="M6 3h8l4 4v14H6z" fill="#e6f6f4" stroke="#0f766e" />
    <path d="M14 3v4h4" stroke="#0f766e" />
    <path d="M9.7 17l2.3-5 2.3 5M10.4 15.4h3.2" stroke="#0f766e" />
  {:else if name === "disk"}
    <path d="M6 3h8l4 4v14H6z" fill="#eef1f5" stroke="#475569" />
    <path d="M14 3v4h4" stroke="#475569" />
    <circle cx="12" cy="14" r="3" stroke="#475569" />
    <circle cx="12" cy="14" r="0.8" fill="#475569" stroke="none" />
  {:else if name === "database"}
    <path d="M6 3h8l4 4v14H6z" fill="#eceafe" stroke="#4f46e5" />
    <path d="M14 3v4h4" stroke="#4f46e5" />
    <ellipse cx="12" cy="11.8" rx="3.2" ry="1.2" stroke="#4f46e5" />
    <path d="M8.8 11.8v3.6c0 .7 1.4 1.2 3.2 1.2s3.2-.5 3.2-1.2v-3.6" stroke="#4f46e5" />
  {:else if name === "book"}
    <!-- Open book / manual: two facing pages meeting at a centre spine (CPE-753). -->
    <path d="M12 6.4C10.2 5.2 7.9 4.6 5.4 4.6c-.9 0-1.7.1-2.4.3v12.8c.7-.2 1.5-.3 2.4-.3 2.5 0 4.8.6 6.6 1.8" />
    <path d="M12 6.4c1.8-1.2 4.1-1.8 6.6-1.8.9 0 1.7.1 2.4.3v12.8c-.7-.2-1.5-.3-2.4-.3-2.5 0-4.8.6-6.6 1.8" />
    <path d="M12 6.4v12.6" />
  {:else if name === "ebook"}
    <path d="M6 3h8l4 4v14H6z" fill="#fdf0e0" stroke="#b45309" />
    <path d="M14 3v4h4" stroke="#b45309" />
    <path d="M12 12c-1-.6-2.4-.6-3.4 0v4.6c1-.6 2.4-.6 3.4 0M12 12c1-.6 2.4-.6 3.4 0v4.6c-1-.6-2.4-.6-3.4 0" stroke="#b45309" />
  {:else if name === "certificate"}
    <path d="M6 3h8l4 4v14H6z" fill="#fdeaee" stroke="#be123c" />
    <path d="M14 3v4h4" stroke="#be123c" />
    <circle cx="12" cy="12.4" r="2.1" stroke="#be123c" />
    <path d="M10.7 14.1l-.7 3 2-1 2 1-.7-3" stroke="#be123c" />
  {:else if name === "cube"}
    <path d="M6 3h8l4 4v14H6z" fill="#e3f4f8" stroke="#0891b2" />
    <path d="M14 3v4h4" stroke="#0891b2" />
    <path d="M12 10.4l3 1.6v3.5L12 17.1l-3-1.6V12zM9 12l3 1.6 3-1.6M12 13.6v3.5" stroke="#0891b2" />
  {:else if name === "web"}
    <path d="M6 3h8l4 4v14H6z" fill="#e7effc" stroke="#2563eb" />
    <path d="M14 3v4h4" stroke="#2563eb" />
    <circle cx="12" cy="14" r="3" stroke="#2563eb" />
    <path d="M9 14h6M12 11c1.3 1.7 1.3 4.3 0 6M12 11c-1.3 1.7-1.3 4.3 0 6" stroke="#2563eb" />
  {:else if name === "unknown"}
    <path d="M6 3h8l4 4v14H6z" fill="#f5f5f5" stroke="#8a8a8a" />
    <path d="M14 3v4h4" stroke="#8a8a8a" />
  <!-- sidebar places -->
  {:else if name === "home"}
    <path d="M4 11l8-6 8 6" stroke="#c94f18" />
    <path d="M6 10v9h12v-9" fill="#e8e8e8" stroke="#9a9a9a" />
  {:else if name === "gallery"}
    <rect x="3" y="5" width="18" height="14" rx="2" fill="#e6f4ea" stroke="#2e7d43" />
    <circle cx="8" cy="10" r="1.4" fill="#2e7d43" stroke="none" />
    <path d="M4 17l5-4 4 3 3-2 4 3" stroke="#2e7d43" />
  {:else if name === "desktop"}
    <rect x="3" y="5" width="18" height="11" rx="1.5" fill="#dbeafe" stroke="#1f5eb5" />
    <path d="M9 20h6M12 16v4" stroke="#1f5eb5" />
  {:else if name === "documents"}
    <path d="M6 3h8l4 4v14H6z" fill="#e7effc" stroke="#2b579a" />
    <path d="M14 3v4h4M9 12h6M9 15h6" stroke="#2b579a" />
  {:else if name === "downloads"}
    <path d="M12 4v10M8 11l4 4 4-4" stroke="#1e7145" />
    <path d="M5 18h14" stroke="#1e7145" />
  {:else if name === "pictures"}
    <rect x="3" y="4" width="18" height="16" rx="2" fill="#fdeaea" stroke="#c42b1c" />
    <circle cx="8.5" cy="9.5" r="1.5" fill="#c42b1c" stroke="none" />
    <path d="M4 17l5-5 4 4 3-2 4 4" stroke="#c42b1c" />
  {:else if name === "music"}
    <path d="M10 18V7l8-2v11" stroke="#7719aa" />
    <circle cx="8" cy="18" r="2" fill="#7719aa" stroke="none" />
    <circle cx="16" cy="16" r="2" fill="#7719aa" stroke="none" />
  {:else if name === "videos"}
    <rect x="3" y="6" width="18" height="12" rx="2" fill="#e7effc" stroke="#1f5eb5" />
    <path d="M10 9.5l5 2.5-5 2.5z" fill="#1f5eb5" stroke="none" />
  {:else if name === "drive"}
    <rect x="3" y="9" width="18" height="7" rx="1.5" fill="#eceff3" stroke="#5f6b7a" />
    <circle cx="17.5" cy="12.5" r="1" fill="#2e7d43" stroke="none" />
  {:else if name === "eject"}
    <path d="M12 5l7 9H5z" fill="currentColor" stroke="none" />
    <rect x="5" y="17" width="14" height="2" fill="currentColor" stroke="none" />
  <!-- toolbar / ui -->
  {:else if name === "back"}
    <path d="M15 5l-7 7 7 7" />
  {:else if name === "forward"}
    <path d="M9 5l7 7-7 7" />
  {:else if name === "up"}
    <path d="M12 19V5M5 12l7-7 7 7" />
  {:else if name === "refresh"}
    <path d="M20 12a8 8 0 1 1-2.3-5.6" />
    <path d="M20 4v5h-5" />
  {:else if name === "rotate-left"}
    <!-- Rotate counter-clockwise (CPE-1576): a single-file image action mirroring `refresh`'s circular
         arrow, curled the other way with its arrowhead at top-left. -->
    <path d="M4 12a8 8 0 1 0 2.3-5.6" />
    <path d="M4 4v5h5" />
  {:else if name === "rotate-right"}
    <!-- Rotate clockwise (CPE-1576): mirror of `rotate-left`, arrowhead at top-right. -->
    <path d="M20 12a8 8 0 1 0-2.3-5.6" />
    <path d="M20 4v5h-5" />
  {:else if name === "chev-right"}
    <path d="M9 5l7 7-7 7" />
  {:else if name === "chev-down"}
    <path d="M5 9l7 7 7-7" />
  {:else if name === "chev-up"}
    <path d="M5 15l7-7 7 7" />
  {:else if name === "search"}
    <circle cx="11" cy="11" r="6" />
    <path d="M20 20l-3.5-3.5" />
  {:else if name === "plus"}
    <path d="M12 5v14M5 12h14" />
  {:else if name === "close"}
    <path d="M6 6l12 12M18 6L6 18" />
  {:else if name === "cut"}
    <circle cx="6" cy="18" r="2.5" /><circle cx="18" cy="18" r="2.5" />
    <path d="M8 16L18 5M16 16L6 5" />
  {:else if name === "copy"}
    <rect x="9" y="9" width="11" height="11" rx="2" />
    <path d="M5 15V6a2 2 0 0 1 2-2h9" />
  {:else if name === "paste"}
    <path d="M9 4h6v3H9z" />
    <path d="M7 5H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-1" />
  {:else if name === "rename"}
    <path d="M4 20h16" />
    <path d="M14 4l6 6-9 9H5v-6z" />
  {:else if name === "share"}
    <circle cx="18" cy="5" r="2.5" /><circle cx="6" cy="12" r="2.5" /><circle cx="18" cy="19" r="2.5" />
    <path d="M8.2 10.8l7.6-4.3M8.2 13.2l7.6 4.3" />
  {:else if name === "popout"}
    <!-- Open-in-new-window: a panel with an arrow leaving the top-right corner. -->
    <path d="M13 4h7v7" />
    <path d="M20 4l-8 8" />
    <path d="M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4" />
  {:else if name === "delete"}
    <path d="M4 7h16M9 7V5h6v2M6 7l1 13h10l1-13" />
  {:else if name === "sort"}
    <path d="M4 7h10M4 12h7M4 17h4" />
    <path d="M17 8v9M14 14l3 3 3-3" />
  {:else if name === "view"}
    <rect x="4" y="4" width="7" height="7" rx="1" />
    <rect x="13" y="4" width="7" height="7" rx="1" />
    <rect x="4" y="13" width="7" height="7" rx="1" />
    <rect x="13" y="13" width="7" height="7" rx="1" />
  {:else if name === "density"}
    <!-- Row-density toggle (CPE-1529): three evenly-spaced horizontal rows — reads as "rows" whether
         the current density is comfortable or compact; the toggle button's own pressed/active state
         (not a second glyph) conveys which one is active. -->
    <path d="M4 6h16M4 12h16M4 18h16" />
  {:else if name === "filter"}
    <path d="M4 5h16l-6 7v6l-4 2v-8z" />
  {:else if name === "more"}
    <circle cx="5" cy="12" r="1.4" fill="currentColor" stroke="none" />
    <circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none" />
    <circle cx="19" cy="12" r="1.4" fill="currentColor" stroke="none" />
  {:else if name === "grip"}
    <!-- Drag handle (CPE-1520): a classic two-column six-dot grip, for the sidebar section drag affordance. -->
    <circle cx="9" cy="6" r="1.4" fill="currentColor" stroke="none" />
    <circle cx="9" cy="12" r="1.4" fill="currentColor" stroke="none" />
    <circle cx="9" cy="18" r="1.4" fill="currentColor" stroke="none" />
    <circle cx="15" cy="6" r="1.4" fill="currentColor" stroke="none" />
    <circle cx="15" cy="12" r="1.4" fill="currentColor" stroke="none" />
    <circle cx="15" cy="18" r="1.4" fill="currentColor" stroke="none" />
  {:else if name === "layers"}
    <!-- Stacked layers (CPE-1531, epic CPE-1489): the "Add to Drop Stack" menu row — a shelf of items
         accumulated across folders. Monochrome via currentColor, like the other toolbar/ui glyphs. -->
    <path d="M12 3.5l8 4.5-8 4.5-8-4.5z" />
    <path d="M4 12l8 4.5 8-4.5" />
    <path d="M4 16.5l8 4.5 8-4.5" />
  {:else if name === "details"}
    <rect x="3" y="5" width="18" height="14" rx="2" />
    <path d="M14 5v14" />
  {:else if name === "check"}
    <path d="M5 13l4 4L19 7" />
  {:else if name === "pin"}
    <path d="M12 17v4" />
    <path d="M9 3h6l-1 6 3 3v2H7v-2l3-3z" />
  {:else if name === "recent"}
    <circle cx="12" cy="12" r="8" />
    <path d="M12 7v5l3 2" />
  {:else if name === "star"}
    <path d="M12 4l2.4 5 5.6.7-4 3.9 1 5.4-5-2.7-5 2.7 1-5.4-4-3.9 5.6-.7z" />
  {:else if name === "people"}
    <circle cx="9" cy="8" r="3" /><path d="M3 20a6 6 0 0 1 12 0" />
    <path d="M16 5.5a3 3 0 0 1 0 5M17 20a6 6 0 0 0-2-4.5" />
  {:else if name === "info"}
    <circle cx="12" cy="12" r="9" />
    <path d="M12 11v5M12 8h.01" />
  {:else if name === "ban"}
    <circle cx="12" cy="12" r="9" />
    <path d="M6 6l12 12" />
  {:else if name === "keyboard"}
    <rect x="2" y="6" width="20" height="12" rx="2" />
    <path d="M6 9h.01M10 9h.01M14 9h.01M18 9h.01M6 12h.01M10 12h.01M14 12h.01M18 12h.01M8 15h8" />
  {:else if name === "tag"}
    <path d="M4 4h7l9 9-7 7-9-9z" />
    <circle cx="8" cy="8" r="1.3" fill="currentColor" stroke="none" />
  {:else if name === "settings"}
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V15z" />
  {:else if name === "activity"}
    <path d="M3 12h4l3 8 4-16 3 8h4" />
  {:else if name === "lock"}
    <!-- Padlock: password-protected archive rows (CPE-1182). Monochrome via currentColor, like the
         other toolbar/ui glyphs, so it inherits the menu row's theme colour (MENUS.md). -->
    <rect x="5" y="11" width="14" height="9" rx="1.5" />
    <path d="M8 11V8a4 4 0 0 1 8 0v3" />
    <circle cx="12" cy="15" r="1.2" fill="currentColor" stroke="none" />
  {:else if name === "lock-open"}
    <!-- Open padlock: an UNLOCKED vault (CPE-1249, epic CPE-738). Same lock body with the shackle swung
         open, monochrome via currentColor so it inherits the badge/menu row's theme colour (MENUS.md). -->
    <rect x="5" y="11" width="14" height="9" rx="1.5" />
    <path d="M8 11V8a4 4 0 0 1 7.5-1.6" />
    <circle cx="12" cy="15" r="1.2" fill="currentColor" stroke="none" />
  {:else if name === "link"}
    <!-- Chain link: New Link… / Repair link… menu rows + dialogs (CPE-1207/1209, epic CPE-715).
         Monochrome via currentColor, like the other toolbar/ui glyphs (MENUS.md). -->
    <path d="M10 14a4 4 0 0 0 5.7.4l2.6-2.6a4 4 0 0 0-5.7-5.7L11 7.5" />
    <path d="M14 10a4 4 0 0 0-5.7-.4L5.7 12.2a4 4 0 0 0 5.7 5.7L13 16.5" />
  <!-- code-preview outline glyphs (CPE-1090): a short letter/mark per SymbolKind, drawn with `currentColor`
       text so a pill's own colour (theme-only, per MENUS.md) is inherited automatically. -->
  {:else if name === "function"}
    <text x="12" y="16.5" font-size="13" font-weight="700" text-anchor="middle" fill="currentColor" stroke="none">ƒ</text>
  {:else if name === "method"}
    <text x="12" y="16.5" font-size="12" font-weight="700" text-anchor="middle" fill="currentColor" stroke="none">m</text>
  {:else if name === "struct"}
    <text x="12" y="16.5" font-size="12" font-weight="700" text-anchor="middle" fill="currentColor" stroke="none">S</text>
  {:else if name === "enum"}
    <text x="12" y="16.5" font-size="12" font-weight="700" text-anchor="middle" fill="currentColor" stroke="none">E</text>
  {:else if name === "trait"}
    <text x="12" y="16.5" font-size="12" font-weight="700" text-anchor="middle" fill="currentColor" stroke="none">T</text>
  {:else if name === "interface"}
    <text x="12" y="16.5" font-size="12" font-weight="700" text-anchor="middle" fill="currentColor" stroke="none">I</text>
  {:else if name === "class"}
    <text x="12" y="16.5" font-size="12" font-weight="700" text-anchor="middle" fill="currentColor" stroke="none">C</text>
  {:else if name === "module"}
    <path d="M12 3.5l7 4v9l-7 4-7-4v-9z" fill="none" stroke="currentColor" />
  {:else if name === "constant"}
    <text x="12" y="16.5" font-size="13" font-weight="700" text-anchor="middle" fill="currentColor" stroke="none">#</text>
  {:else if name === "type_alias"}
    <text x="12" y="16.5" font-size="12" font-weight="700" text-anchor="middle" fill="currentColor" stroke="none">τ</text>
  {:else if name === "heading"}
    <text x="12" y="16.5" font-size="12" font-weight="700" text-anchor="middle" fill="currentColor" stroke="none">¶</text>
  <!-- Link glyph (CPE-1208, epic CPE-715): the FileList row badge for a symlink entry, and its
       "broken" variant (a gap in the chain) for a symlink whose target doesn't resolve. Monochrome via
       currentColor, like the other toolbar/ui glyphs, so it inherits the badge's own colour. -->
  {:else if name === "link"}
    <path d="M9.5 14.5a3.5 3.5 0 0 0 5 0l3-3a3.5 3.5 0 0 0-5-5l-1.2 1.2" />
    <path d="M14.5 9.5a3.5 3.5 0 0 0-5 0l-3 3a3.5 3.5 0 0 0 5 5l1.2-1.2" />
  {:else if name === "link-broken"}
    <path d="M8.7 15.3a3.5 3.5 0 0 0 5 0l1.4-1.4" />
    <path d="M15.3 8.7a3.5 3.5 0 0 0-5 0L8.9 10.1" />
    <path d="M4 4l16 16" />
  {:else if name === "terminal"}
    <!-- Embedded terminal dock toggle (CPE-1243, epic CPE-714): a screen frame with a ">_" prompt. -->
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <path d="M7 10l3 3-3 3" />
    <path d="M13 16h4" />
  {:else if name === "mail"}
    <!-- Envelope (CPE-1434, epic CPE-1433): the .eml email-preview banner glyph. Monochrome via
         currentColor like the other ui glyphs. -->
    <rect x="3" y="5" width="18" height="14" rx="2" />
    <path d="M4 7l8 6 8-6" />
  {:else if name === "paperclip"}
    <!-- Paperclip (CPE-1434): the attachment-pill glyph. -->
    <path d="M20 11.5l-8.5 8.5a4.5 4.5 0 0 1-6.4-6.4l8.5-8.5a3 3 0 0 1 4.3 4.3l-8.5 8.5a1.5 1.5 0 0 1-2.2-2.2l7.8-7.8" />
  {:else if name === "calendar"}
    <!-- Calendar (CPE-1435): the .ics event-preview banner + when glyph. -->
    <rect x="3" y="4" width="18" height="17" rx="2" />
    <path d="M3 9h18M8 2v4M16 2v4" />
  {:else if name === "contact"}
    <!-- Contact card (CPE-1436): the .vcf contact-preview banner glyph. -->
    <rect x="3" y="5" width="18" height="14" rx="2" />
    <circle cx="8.5" cy="11" r="2" />
    <path d="M5.5 16a3 3 0 0 1 6 0M14 9.5h4M14 13h4" />
  {:else if name === "phone"}
    <!-- Phone (CPE-1436): the TEL row glyph. -->
    <path d="M5 3h3l1.5 5-2 1.5a11 11 0 0 0 5 5l1.5-2 5 1.5v3a2 2 0 0 1-2.2 2A17 17 0 0 1 3 5.2 2 2 0 0 1 5 3z" />
  {:else if name === "location"}
    <!-- Location pin (CPE-1435/1436): the LOCATION / ADR row glyph. -->
    <path d="M12 21s7-6.2 7-11a7 7 0 0 0-14 0c0 4.8 7 11 7 11z" />
    <circle cx="12" cy="10" r="2.5" />
  {:else if name === "globe"}
    <!-- Globe (CPE-1436): the URL row glyph. -->
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18M12 3c2.5 2.7 2.5 15.3 0 18M12 3c-2.5 2.7-2.5 15.3 0 18" />
  {/if}
</svg>

<style>
  .icon { flex: none; display: block; }
</style>
