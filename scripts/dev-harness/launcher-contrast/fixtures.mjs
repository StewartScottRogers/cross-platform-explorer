// CPE-1966 — the DOM the AI Console launcher's stylesheet describes but its static markup does not
// contain, plus the deliberate exclusions.
//
// WHY THIS FILE EXISTS. `sidecar/ai-console/src/launcher.html` builds a large part of its own UI from
// JavaScript at runtime (session tabs, the "Close all" button, model-menu options, grid panes, swarm
// feed rows). CPE-1921's sweep loaded the STATIC page, so every one of those elements was simply
// absent — and an absent element cannot fail a contrast check. That is one of the two structural
// reasons its sweep honestly reported zero while four measured defects sat on the page (CPE-1966); the
// `.close-all-btn:hover` defect is exactly this shape, and it is invisible to a static load twice over
// (JS-built AND a `:hover` state).
//
// WHAT KEEPS IT HONEST (CPE-1932 "enumerate, don't recall"). The list below is NOT the enumeration —
// `engine.mjs` derives the enumeration from the stylesheet itself (every class/id selector it declares)
// and then requires every derived selector to be EITHER matched in the DOM (statically or after these
// fixtures run) OR named in `UNREACHABLE` with a reason. A new JS-built, CSS-styled element therefore
// fails this harness the day its rule is written, rather than silently going unmeasured. What is
// hand-written here is only WHERE each one is mounted, which no amount of parsing can recover from CSS.
//
// The mounted markup mirrors what the launcher's own JS builds. That is a provenance claim, and per
// CPE-1933 it must be derivable rather than asserted: `engine.mjs`'s `checkFixtureProvenance` reads the
// launcher's script source and fails when a fixture's `derivedFrom` string is no longer present in it,
// so a rename in the launcher's builder breaks this file loudly instead of leaving a fixture that
// mirrors code which no longer exists.

/** Elements the launcher's JS creates at runtime. `html` is inserted into `parent` before measuring. */
export const FIXTURES = [
  {
    name: "session tabs + Close all",
    parent: "#tabs",
    // `.close-all-btn` is CPE-1966's site 4: a literal-hex `:hover` colour on a JS-built element
    // sitting on `#tabs`'s own `rgba(128,128,128,0.10)` fill, not on bare Canvas.
    // Child order and class set copied from `addSession()`: dot, chip, label, usage, close — and the
    // three tab states the JS can add (`active`, `blocked` from renderState, `ended` from ws.onclose).
    derivedFrom: [
      'b.className = "close-all-btn"', 'tab.className = "tab"', 'chip.className = "tab-chip"',
      'use.className = "tab-usage"', 'x.className = "tab-close"',
      's.tab.classList.add("ended")',
    ],
    html: `
      <div class="tab active" data-fixture="tab-active">
        <span class="state-dot"></span>
        <span class="tab-chip" style="background:__PALETTE_0__">1</span>
        <span class="tab-label">claude — cross-platform-explorer</span>
        <span class="tab-usage">$0.12</span>
        <button class="tab-close" type="button">×</button>
      </div>
      <div class="tab blocked" data-fixture="tab-blocked">
        <span class="state-dot"></span>
        <span class="tab-chip" style="background:__PALETTE_1__">2</span>
        <span class="tab-label">aider — needs you</span>
        <span class="tab-usage">$0.31</span>
        <button class="tab-close" type="button">×</button>
      </div>
      <div class="tab ended" data-fixture="tab-ended">
        <span class="state-dot"></span>
        <span class="tab-chip" style="background:__PALETTE_1__">3</span>
        <span class="tab-label">codex — finished</span>
        <span class="tab-usage">$0.04</span>
        <button class="tab-close" type="button">×</button>
      </div>
      <button class="close-all-btn" type="button" data-fixture="close-all">Close all</button>`,
  },
  {
    // Every entry of the identity palette, not a sample of it: `sessionColor()` picks by hash, so any
    // one of the eight can end up behind the chip's white numeral. `engine.mjs` reads the array out of
    // launcher.html's own script and expands this fixture to one chip per entry.
    name: "session chips (one per SESSION_CHIP_COLORS entry)",
    parent: "#tabs",
    derivedFrom: ["const SESSION_CHIP_COLORS = ["],
    html: "__PALETTE_CHIPS__",
  },
  {
    name: "model menu options",
    parent: "#model-menu",
    derivedFrom: ['opt.className = "model-opt"'],
    html: `
      <button class="model-opt active" type="button">anthropic/claude-opus-4 <span class="mo-sub">200k ctx</span></button>
      <button class="model-opt" type="button">openai/gpt-5 <span class="mo-sub">400k ctx</span></button>
      <div class="model-msg">No models cached <button type="button">Refresh</button></div>`,
  },
  {
    name: "grid view panes",
    parent: "#terms",
    // The grid is a whole second view of the SAME panes, reached by a `.grid-view` class on #terms.
    // Nothing about it renders on the static page, so none of its colours had ever been measured.
    derivedFrom: ['classList.toggle("grid-view"', 'head.className = "pane-head"', 'pstate.className = "pane-state"', 's.pane.classList.add("ended")'],
    applyToParent: "grid-view",
    html: `
      <div class="term-pane focused" data-fixture="pane-focused">
        <div class="pane-head">
          <span class="state-dot"></span>
          <span class="pane-chip" style="background:__PALETTE_0__">1</span>
          <span class="pane-label">claude — cross-platform-explorer</span>
          <span class="pane-state">working</span>
          <span class="pane-usage">$0.12</span>
        </div>
        <div class="term-host"></div>
        <div class="sb"><div class="sb-thumb"></div></div>
      </div>
      <div class="term-pane ended" data-fixture="pane-ended">
        <div class="pane-head">
          <span class="state-dot"></span>
          <span class="pane-chip" style="background:__PALETTE_1__">2</span>
          <span class="pane-label">aider — done</span>
          <span class="pane-state">done</span>
          <span class="pane-usage">$0.04</span>
        </div>
        <div class="term-host"></div>
        <div class="sb"><div class="sb-thumb"></div></div>
      </div>`,
  },
  {
    name: "swarm coordination feed",
    parent: "#sw-mailbox",
    derivedFrom: ['row.className = "sw-msg"', 'k.className = "sw-kind"'],
    html: `
      <div class="sw-msg"><span class="sw-kind">ask</span><span class="sw-route">b1 &rarr; coord</span><span class="sw-body">Which glob owns docs/?</span></div>
      <div class="sw-note">coordinator: split docs/** off to builder 2</div>
      <div class="sw-empty">Nothing yet</div>`,
  },
  {
    name: "install badges + saved-session rows",
    parent: "#install-wrap",
    derivedFrom: ['badge.className = "badge " + (a.installed ? "yes" : "no")'],
    html: `
      <span class="badge yes">installed</span>
      <span class="badge no">not installed</span>`,
  },
  {
    name: "recent-sessions rows + key rows",
    parent: "#history-list",
    derivedFrom: ['row.className = "hist-row"'],
    html: `
      <div class="hist-row"><span class="hist-meta">claude · openrouter · 2 days ago</span><span class="hist-btns"><button type="button">Relaunch</button></span></div>`,
  },
  {
    name: "saved key rows",
    parent: "#keys-list",
    derivedFrom: ['row.className = "key-row"'],
    html: `<div class="key-row"><span>openrouter · work</span><button type="button">Forget</button></div>`,
  },
  {
    name: "status-line states",
    parent: "#msg",
    // #msg/#keys-msg take one of three state classes from setMsg()/keysMsg(). The class is what
    // CPE-1921 moved them onto, so measuring the DEFAULT (classless) #msg measures nothing it fixed.
    derivedFrom: ["function setMsg(", "function keysMsg("],
    mode: "classes-on-self",
    classes: ["ok", "warn", "err"],
    html: "",
  },
  {
    name: "keys status-line states",
    parent: "#keys-msg",
    derivedFrom: ["function keysMsg("],
    mode: "classes-on-self",
    classes: ["ok", "warn", "err"],
    html: "Saved.",
  },
];

/**
 * Selectors the stylesheet declares that this harness deliberately does NOT measure, each with the
 * reason. `engine.mjs` fails if a stylesheet selector is neither matched nor listed here, so this list
 * is the only way a rule can go unmeasured — and it costs a written reason every time.
 */
export const UNREACHABLE = [
  ["done", "`.boot-overlay.done` is the faded-OUT boot overlay: `opacity: 0; pointer-events: none`. Nothing in it is readable by construction, so a contrast bar is meaningless there."],
  ["help-more", "dead CSS: `.help-more` is styled in launcher.html but no markup and no JS ever creates it (grepped 2026-08-27). Delete the rule or use it; either way there is nothing on screen to measure."],
  ["busy", "`body.busy` is a cursor-only rule (`cursor: progress !important`) from the CPE-482 busy-cursor convention. It declares no colour, so there is nothing for a contrast bar to apply to."],
  ["xterm-viewport", "vendor xterm.js chrome, not authored here: the terminal's own CSS is injected at serve time by console.rs's `__XTERM_CSS__` substitution, and its colours come from the `Terminal({ theme })` object in the launcher's JS. Measuring it here would measure a stylesheet this file never sees."],
];
