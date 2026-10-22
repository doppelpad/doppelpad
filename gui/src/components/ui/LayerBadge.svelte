<script lang="ts">
  // A layer's name in a pill of the layer's colour.
  import { app } from '../../lib/state/app.svelte';
  import { layerColor } from '../../lib/theme/layers';

  let { layer }: { layer: number } = $props();
</script>

<span class="layer" style="--c: {layerColor(layer)}">{app.layerLabel(layer)}</span>

<style>
  .layer {
    font-size: var(--fs-xs);
    padding: 1px 8px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--c);
    color: var(--c);
  }
</style>
