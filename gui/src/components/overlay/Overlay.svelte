<script lang="ts">
  // The overlay a window shows, picked by its label (`overlay-<kind>`).
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import CrosshairOverlay from './CrosshairOverlay.svelte';
  import Osd from './Osd.svelte';
  import RadialOverlay from './RadialOverlay.svelte';

  const kind = getCurrentWebviewWindow().label.replace(/^overlay-/, '');
</script>

{#if kind === 'osd'}
  <Osd />
{:else if kind === 'crosshair'}
  <CrosshairOverlay />
{:else if kind === 'radial'}
  <RadialOverlay />
{/if}

<style>
  :global(html),
  :global(body) {
    background: transparent;
  }
</style>
