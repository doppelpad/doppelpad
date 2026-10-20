<script lang="ts">
  // The overlay a window shows, picked by its label (`overlay-<kind>`).
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import CrosshairOverlay from './CrosshairOverlay.svelte';
  import MappingsOverlay from './MappingsOverlay.svelte';
  import Osd from './Osd.svelte';
  import PadOverlay from './PadOverlay.svelte';
  import RadialOverlay from './RadialOverlay.svelte';

  const kind = getCurrentWebviewWindow().label.replace(/^overlay-/, '');
</script>

{#if kind === 'osd'}
  <Osd />
{:else if kind === 'crosshair'}
  <CrosshairOverlay />
{:else if kind === 'radial'}
  <RadialOverlay />
{:else if kind === 'pad'}
  <PadOverlay />
{:else if kind === 'mappings'}
  <MappingsOverlay />
{/if}

<style>
  :global(html),
  :global(body) {
    background: transparent;
  }
</style>
