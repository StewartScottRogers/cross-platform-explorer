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
  import Icon from "./Icon.svelte";

  export let label: string;
  export let icon: string;

  let open = false;
  let flip = false;
  let wrapEl: HTMLDivElement;
  let parentEl: HTMLButtonElement;
  let flyoutEl: HTMLDivElement;

  function items(): HTMLButtonElement[] {
    if (!flyoutEl) return [];
    return Array.from(
      flyoutEl.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"),
    );
  }

  async function openMenu(focusFirst = false) {
    open = true;
    await tick();
    // Clamp on-screen: if the flyout (opened rightward at left:100%) would overflow the viewport's
    // right edge, flip it to open leftward instead. In jsdom rects are 0 so this is a no-op there.
    if (flyoutEl) {
      const r = flyoutEl.getBoundingClientRect();
      const pad = 6;
      flip = r.right > window.innerWidth - pad && r.width > 0;
    }
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
      class:flip
      role="menu"
      tabindex="-1"
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
  /* The flyout reuses the exact container treatment from MENUS.md (the `.ctx` table). */
  .flyout {
    position: absolute;
    top: -6px;
    left: 100%;
    z-index: 101;
    min-width: 190px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.16);
    outline: none;
  }
  .flyout.flip {
    left: auto;
    right: 100%;
  }
</style>
