<script lang="ts">
  // The crosshair over the game, sent by the app (crosshair.rs, see lib/overlay.ts); the
  // window is sized and centred by the app.
  import { onMount } from 'svelte';
  import { overlayContent } from '../../lib/overlay';
  import type { Crosshair } from '../../lib/types';
  import CrosshairShape from '../crosshair/CrosshairShape.svelte';

  type Full = Required<Omit<Crosshair, 'magnifier'>>;
  let crosshair = $state<Full | null>(null);

  onMount(() => overlayContent<Full>((c) => (crosshair = c)));
</script>

{#if crosshair}
  <div class="centre"><CrosshairShape {crosshair} /></div>
{/if}

<style>
  .centre {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
  }
</style>
