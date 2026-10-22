<script lang="ts">
  // A section of a settings dialog: a line above it, the title (buttons may sit at its right),
  // an optional hint, then the settings. `<p class="hint">` paragraphs among the settings get
  // the hint look as well.
  import type { Snippet } from 'svelte';

  let {
    title,
    hint,
    actions,
    shrink = false,
    children,
  }: {
    title: string;
    hint?: string;
    /** Buttons at the right of the title. */
    actions?: Snippet;
    /** In a dialog that does not scroll: the section may get smaller than its content, which then scrolls. */
    shrink?: boolean;
    children: Snippet;
  } = $props();
</script>

<div class="section" class:shrink>
  <div class="section-title">
    <span>{title}</span>
    {#if actions}{@render actions()}{/if}
  </div>
  {#if hint}<p class="hint">{hint}</p>{/if}
  {@render children()}
</div>

<style>
  .section {
    border-top: 1px solid var(--line);
    padding-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .section.shrink {
    min-height: 0;
  }
  .section-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
</style>
