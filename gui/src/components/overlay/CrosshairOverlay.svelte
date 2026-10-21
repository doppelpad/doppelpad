<script lang="ts">
  // The crosshair over the game, sent by the app (crosshair.rs, see lib/overlay.ts); the
  // window is sized and centred by the app.
  import { onMount } from 'svelte';
  import { overlayContent } from '../../lib/overlay';
  import type { CrosshairFull } from '../../lib/types';
  import CrosshairShape from '../crosshair/CrosshairShape.svelte';

  let crosshair = $state<CrosshairFull | null>(null);

  onMount(() => overlayContent<CrosshairFull>((c) => (crosshair = c)));
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
