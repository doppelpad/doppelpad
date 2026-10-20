<script lang="ts">
  // The overlay a window shows, picked by its label (`overlay-<kind>`).
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import CrosshairOverlay from './CrosshairOverlay.svelte';
  import Osd from './Osd.svelte';

  const kind = getCurrentWebviewWindow().label.replace(/^overlay-/, '');
</script>

{#if kind === 'osd'}
  <Osd />
{:else if kind === 'crosshair'}
  <CrosshairOverlay />
{/if}

<style>
  :global(html),
  :global(body) {
    background: transparent;
  }
</style>
