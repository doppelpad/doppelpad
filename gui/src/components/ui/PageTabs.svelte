<script lang="ts" generics="T extends string">
  // Pages behind a vertical list of tabs, as in the settings and the config settings: the list
  // on the left (up and down, Home and End move through it), the chosen page on the right.
  import type { Component, Snippet } from 'svelte';
  import { tip } from '../../lib/tooltip.svelte';

  let {
    pages,
    current,
    onchange,
    label,
    scroll = false,
    children,
  }: {
    pages: readonly { id: T; label: string; icon: Component<{ size?: number }>; warning?: string }[];
    current: T;
    onchange: (id: T) => void;
    /** The accessible name of the list of tabs. */
    label: string;
    /** In a dialog: every page has the same height and scrolls, so the dialog does not jump. */
    scroll?: boolean;
    /** The chosen page. */
    children: Snippet;
  } = $props();

  const id = $props.id();

  function onkeydown(e: KeyboardEvent) {
    const i = pages.findIndex((p) => p.id === current);
    const to = { ArrowDown: i + 1, ArrowUp: i - 1, Home: 0, End: pages.length - 1 }[e.key];
    if (to === undefined) return;
    e.preventDefault();
    const next = pages[(to + pages.length) % pages.length]!;
    onchange(next.id);
    document.getElementById(`${id}-tab-${next.id}`)?.focus();
  }
</script>

<div class="layout" class:scroll>
  <div class="pages" role="tablist" aria-orientation="vertical" aria-label={label} tabindex="-1" {onkeydown}>
    {#each pages as p (p.id)}
      <button
        type="button"
        role="tab"
        id="{id}-tab-{p.id}"
        aria-selected={current === p.id}
        aria-controls="{id}-page"
        tabindex={current === p.id ? 0 : -1}
        class:on={current === p.id}
        onclick={() => onchange(p.id)}
      >
        <p.icon size={16} />
        <span class="name">{p.label}</span>
        {#if p.warning}<span class="flag-dot" use:tip={p.warning}></span>{/if}
      </button>
    {/each}
  </div>

  {#key current}
    <div class="page" role="tabpanel" id="{id}-page" aria-labelledby="{id}-tab-{current}" tabindex="-1">
      {@render children()}
    </div>
  {/key}
</div>

<style>
  .layout {
    display: grid;
    grid-template-columns: 184px minmax(0, 1fr);
    gap: 28px;
    align-items: start;
  }
  /* The same height on every page, so the dialog does not jump when the page changes. */
  .layout.scroll {
    gap: 20px;
    align-items: stretch;
    height: min(560px, calc(100vh - 150px));
  }
  .pages {
    position: sticky;
    top: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-right: 12px;
    border-right: 1px solid var(--line);
    outline: none;
  }
  .pages button {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: var(--radius-m);
    color: var(--text-dim);
    text-align: left;
  }
  .pages button:hover:not(.on) {
    color: var(--text);
    background: var(--bg-2);
  }
  .pages button.on {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .name {
    flex: 1;
  }
  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
    outline: none;
  }
  .scroll .page {
    min-height: 0;
    overflow-y: auto;
    padding-right: 6px;
  }
  /* The page's first section, or its first heading, needs no line or space above it. */
  .page > :global(.section:first-child),
  .page > :global(.section-title:first-child) {
    margin-top: 0;
    padding-top: 0;
    border-top: none;
  }
</style>
