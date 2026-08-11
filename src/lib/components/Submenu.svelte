<script lang="ts">
  /**
   * A nested flyout submenu for the context menu (CPE-1153). The context menu was flat; this is the
   * one reusable "parent row + flyout panel" piece behind New ▸ / View ▸ / Sort by ▸.
   *
   * Behaviour (Windows 11 Explorer parity, but our own DOM — cross-platform per MENUS.md):
   *   - opens on hover AND on Right-arrow / Enter / Space / Down-arrow from the parent row;
   *   - closes on Escape / Left-arrow (returning focus to the parent) and on mouse-leave;
   *   - arrow keys move focus between the flyout's items;
   *   - a trailing chevron marks the parent row;
   *   - the flyout clamps on-screen: it opens leftward when it would overflow the right edge
   *     (the menu is `position:fixed`).
   *
   * Styling mirrors `.ctx` / `.row` from MENUS.md exactly — item text is always `var(--text)`,
   * colours come only from theme variables, identical light/dark.
   */
  import { tick } from "svelte";
  import { onDestroy } from "svelte";
  import Icon from "./Icon.svelte";

  export let label: string;
  export let icon: string;

  let open = false;
  let flip = false;
  let wrapEl: HTMLDivElement;
  let parentEl: HTMLButtonElement;
  let flyoutEl: HTMLDivElement;
  // CPE-1601 follow-up: viewport-space coordinates, NOT CSS-percent offsets — see the `.flyout` style
  // comment below for why `position: fixed` + explicit px replaced `position: absolute` + `left: 100%`.
  let top = 0;
  let left = 0;

  function items(): HTMLButtonElement[] {
    if (!flyoutEl) return [];
    return Array.from(
      flyoutEl.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"),
    );
  }

  /** Anchors the (viewport-`position:fixed`) flyout to the parent row's CURRENT on-screen rect —
   *  called on open, and again on every scroll of any ancestor (including `.ctx` itself, now that
   *  CPE-1601 made it internally scrollable) so the flyout tracks its row instead of drifting. Mirrors
   *  the OLD `top:-6px; left:100%` relative offsets, just computed in px since `position:fixed`
   *  percentages resolve against the viewport, not the parent element. */
  function positionFlyout() {
    if (!parentEl) return;
    const pad = 6;
    const pr = parentEl.getBoundingClientRect();
    const fw = flyoutEl?.getBoundingClientRect().width ?? 0;
    const fh = flyoutEl?.getBoundingClientRect().height ?? 0;

    // Clamp on-screen: if opening rightward (the default) would overflow the viewport's right edge,
    // flip to open leftward instead — same rule as before, just measured against the viewport instead
    // of relying on the (now-clipping) `.ctx` ancestor's box. `fw` is 0 on the FIRST call of a given
    // open (flyout not measured yet) and in jsdom (no real layout), so this only flips once real
    // dimensions are known — matching the original's identical "no-op until measured" behaviour.
    flip = pr.right + fw > window.innerWidth - pad && fw > 0;
    left = flip ? pr.left - fw : pr.right;
    // CPE-1601 round 3: the flip above is a bare binary choice with no floor/ceiling of its own — a
    // narrow viewport (or, found live: a right-edge menu whose flip lands the flyout further left than
    // the screen itself) could still put `left` off either edge. Clamp it the same way `top` already
    // is below, instead of trusting the flip alone to land on-screen.
    if (fw > 0) {
      left = Math.max(pad, Math.min(left, window.innerWidth - fw - pad));
    }

    top = pr.top - 6;
    // Vertical clamp — the original CSS never handled a flyout taller than the viewport; worth having
    // now that this fn already does the viewport-space math the flip check needs.
    if (fh > 0) {
      top = Math.min(top, window.innerHeight - fh - pad);
    }
    top = Math.max(pad, top);
  }

  /** True once the parent row has scrolled fully outside `ancestor`'s visible box — i.e. the row a
   *  flyout is anchored to is no longer on screen at all (not just partially clipped). */
  function anchorScrolledOut(parentRect: DOMRect, ancestorRect: DOMRect): boolean {
    return parentRect.bottom < ancestorRect.top || parentRect.top > ancestorRect.bottom;
  }

  /** Nearest ancestor that actually clips/scrolls its content (`overflow-y: auto|scroll|hidden`) —
   *  `.ctx` today (the only caller of this component), found generically rather than hard-coded to
   *  that class name so this keeps working if `Submenu` is ever reused inside a different scroll
   *  container (`AgentMenu`/`TabMenu` per docs/design/MENUS.md's shared `.ctx` pattern). `null` if none
   *  (nothing to orphan the flyout FROM, in which case scroll can't hide the anchor row this way). */
  function nearestScrollAncestor(el: HTMLElement | null): HTMLElement | null {
    let node = el?.parentElement ?? null;
    while (node) {
      const oy = getComputedStyle(node).overflowY;
      if (oy === "auto" || oy === "scroll" || oy === "hidden") return node;
      node = node.parentElement;
    }
    return null;
  }

  /** CPE-1601 round 3: scrolling `.ctx` used to only ever REPOSITION the (now-tracking)
   *  `position:fixed` flyout — it never asked whether the anchor row was still visible at all. Drag
   *  the scrollbar thumb, or a fast trackpad/momentum scroll, and the row can scroll fully out of
   *  `.ctx`'s visible box while the cursor never leaves the row or the scrollbar track — neither
   *  triggers `mouseleave`, so the flyout was left floating, clamped to the top of the menu, adjacent
   *  to nothing and overlapping unrelated rows: a menu detached from the thing that opened it.
   *  Confirmed live (real Chrome): scrolled a tall menu until the "Run macro" row's rect was
   *  `top:-87, bottom:-55` (fully scrolled away) and the flyout stayed open at `top:6`. Fix: close
   *  instead of reposition once the anchor itself is gone. */
  function onAncestorScroll() {
    if (!open) return;
    if (parentEl) {
      const ancestor = nearestScrollAncestor(parentEl);
      if (ancestor && anchorScrolledOut(parentEl.getBoundingClientRect(), ancestor.getBoundingClientRect())) {
        closeMenu(false);
        return;
      }
    }
    positionFlyout();
  }

  // Capture phase: catches scroll on ANY ancestor, including `.ctx` itself, even though element
  // `scroll` events don't reliably bubble across every engine this app targets (wry/WebKitGTK
  // included) — the standard technique for "some scrollable ancestor scrolled" without knowing which
  // one ahead of time. Registered once; guarded by the `open` check above so it's a no-op while closed.
  if (typeof window !== "undefined") {
    window.addEventListener("scroll", onAncestorScroll, true);
    onDestroy(() => window.removeEventListener("scroll", onAncestorScroll, true));
  }

  async function openMenu(focusFirst = false) {
    open = true;
    positionFlyout(); // rough pass (pre-measurement) so the first paint isn't at (0,0)
    await tick();
    positionFlyout(); // real pass — flyoutEl now exists, so the flip/vertical-clamp math has real dimensions
    if (focusFirst) items()[0]?.focus();
  }

  function closeMenu(focusParent = false) {
    open = false;
    flip = false;
    if (focusParent) parentEl?.focus();
  }

  function onParentKey(e: KeyboardEvent) {
    if (e.key === "ArrowRight" || e.key === "Enter" || e.key === " " || e.key === "ArrowDown") {
      e.preventDefault();
      void openMenu(true);
    }
  }

  function onFlyoutKey(e: KeyboardEvent) {
    const list = items();
    const idx = list.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "ArrowDown") {
      e.preventDefault();
      list[(idx + 1) % list.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      list[(idx - 1 + list.length) % list.length]?.focus();
    } else if (e.key === "ArrowLeft" || e.key === "Escape") {
      // Close just this submenu (not the whole menu) and hand focus back to the parent row.
      e.preventDefault();
      e.stopPropagation();
      closeMenu(true);
    }
  }
</script>

<!-- svelte-ignore a11y-no-noninteractive-element-interactions a11y-no-static-element-interactions -->
<div
  class="submenu"
  bind:this={wrapEl}
  on:mouseenter={() => openMenu(false)}
  on:mouseleave={() => closeMenu(false)}
>
  <button
    class="row parent"
    role="menuitem"
    aria-haspopup="true"
    aria-expanded={open}
    bind:this={parentEl}
    on:click|stopPropagation={() => (open ? closeMenu(false) : openMenu(false))}
    on:keydown={onParentKey}
  >
    <Icon name={icon} size={15} /> {label}
    <span class="chev"><Icon name="chev-right" size={14} /></span>
  </button>

  {#if open}
    <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
    <div
      class="flyout"
      role="menu"
      tabindex="-1"
      data-flip={flip}
      style="position:fixed; top:{top}px; left:{left}px"
      bind:this={flyoutEl}
      on:keydown={onFlyoutKey}
    >
      <slot />
    </div>
  {/if}
</div>

<style>
  .submenu {
    position: relative;
  }
  /* The parent row lives in *this* component, so it can't inherit ContextMenu's scoped `.row`;
     replicate the MENUS.md item layout here. Slotted items keep ContextMenu's `.row` styling. */
  .parent {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    text-align: left;
    border-radius: var(--radius);
    white-space: nowrap;
  }
  .chev {
    margin-left: auto;
    display: inline-flex;
    color: var(--text-faint);
  }
  /* The flyout reuses the exact container treatment from MENUS.md (the `.ctx` table). CPE-1601
     follow-up: `position: fixed` (was `absolute` with CSS `top:-6px; left:100%`) is load-bearing, not
     cosmetic — `.ctx` (ContextMenu.svelte) is now an `overflow-y:auto` scroll container (CPE-1601's
     original fix), and per the CSS Overflow spec a `visible` `overflow-x` computes to `auto` the moment
     the OTHER axis isn't `visible` — `overflow-x: visible` cannot opt back out (verified directly: it
     still resolves to `auto`). So `.ctx` clips BOTH axes now, and an absolutely-positioned descendant
     (this flyout, `left:100%` deliberately escaping `.submenu`'s own box) would be clipped to nothing —
     confirmed live in a real engine, not just reasoned about. `position: fixed` sidesteps the whole
     problem: a fixed-position box is positioned against the viewport (the INITIAL containing block),
     not against `.ctx`'s box, and — critically — is NOT part of `.ctx`'s scrollable-overflow region at
     all, so it paints on top, uncropped, regardless of `.ctx`'s scroll position or clip. (This only
     holds because no ancestor between here and the viewport sets `transform`/`filter`/`perspective`/
     `will-change:transform`/`contain:paint`, any of which would make THAT ancestor the fixed
     containing block instead — none of `.ctx`/`.submenu`'s ancestors do.) `top`/`left` are therefore
     computed in JS (`positionFlyout()`) from the parent row's `getBoundingClientRect()` — a fixed
     element's `%` offsets resolve against the viewport, not the parent, so the old CSS percentages
     couldn't carry over as-is. */
  /* CPE-1601 round 3: this is CPE-1601's OWN original bug, one level deeper. `macros`/`userCommands`
     (ContextMenu.svelte props feeding a Submenu's slot) are unbounded lists — a long one renders a
     flyout taller than the viewport, and with no `max-height`/`overflow-y` here it simply ran off the
     bottom with nothing to scroll it into view (found live: a 40-row flyout at 1292px tall in an 853px
     viewport, 445px past the bottom edge — `positionFlyout()`'s vertical clamp only pins `top`, it
     can't shrink a box that's taller than the screen).

     TRAP (the exact one round 1 hit on `.ctx` — repeating it here deliberately, with eyes open):
     setting `overflow-y` on an element whose `overflow-x` is `visible` makes `overflow-x` compute to
     `auto` too (CSS Overflow spec — a `visible` axis becomes `auto` the moment the other isn't). That
     makes `.flyout` a clipping box on BOTH axes. This is safe here ONLY because `.flyout` today has NO
     nested submenu of its own — nothing inside it needs to escape sideways the way `.flyout` itself
     escapes `.ctx`. If a submenu is ever nested inside a flyout (a "New ▸" inside a "Run macro ▸", say),
     whatever renders IT must repeat this same `position:fixed`-anchored-to-parent-rect treatment, not
     rely on CSS overflow/positioning — revisit this exact comment first. */
  .flyout {
    position: fixed;
    z-index: 101;
    min-width: 190px;
    max-height: calc(100vh - 12px);
    overflow-y: auto;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.16);
    outline: none;
  }
</style>
