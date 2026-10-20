<script lang="ts">
  // Small glyphs for the six activators: dots for taps, a bar for long press,
  // arrows onto / away from a baseline for start and release.
  import type { Activator } from '../../lib/types';

  let { activator, size = 22 }: { activator: Activator; size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
  {#if activator === 'single'}
    <circle cx="12" cy="12" r="3.2" fill="currentColor" stroke="none" />
  {:else if activator === 'double'}
    <circle cx="8" cy="12" r="2.8" fill="currentColor" stroke="none" />
    <circle cx="16" cy="12" r="2.8" fill="currentColor" stroke="none" />
  {:else if activator === 'triple'}
    <circle cx="5.5" cy="12" r="2.4" fill="currentColor" stroke="none" />
    <circle cx="12" cy="12" r="2.4" fill="currentColor" stroke="none" />
    <circle cx="18.5" cy="12" r="2.4" fill="currentColor" stroke="none" />
  {:else if activator === 'long'}
    <circle cx="6" cy="12" r="2.8" fill="currentColor" stroke="none" />
    <path d="M10 12h10" stroke-width="3" />
  {:else if activator === 'start'}
    <path d="M12 4v10M8 10l4 4 4-4" />
    <path d="M5 19h14" />
  {:else}
    <path d="M12 14V4M8 8l4-4 4 4" />
    <path d="M5 19h14" />
  {/if}
</svg>
