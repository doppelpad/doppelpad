<script lang="ts">
  // A note about layers, in the second layer colour: an icon, the text (the children) and an
  // optional action at the end.
  import type { Component, Snippet } from 'svelte';

  let { icon: Icon, children, action }: { icon: Component<{ size?: number }>; children: Snippet; action?: Snippet } = $props();
</script>

<div class="note">
  <Icon size={14} />
  <span>{@render children()}</span>
  {#if action}{@render action()}{/if}
</div>

<style>
  .note {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: var(--fs-s);
    color: var(--layer-2);
    background: color-mix(in srgb, var(--layer-2) 10%, transparent);
    border-radius: var(--radius-s);
    padding: 6px 8px;
  }
</style>
