<script lang="ts">
  // On a layer other than Main: says whether the settings below are the layer's own or
  // Main's, and lets the layer follow Main again.
  import { Layers } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { layerOverrides, resetLayerPart, type AnalogKind, type Side } from '../../lib/model/analog';
  import { app } from '../../lib/state/app.svelte';
  import LayerNote from '../ui/LayerNote.svelte';

  let { kind, sides }: { kind: AnalogKind; sides: Side[] } = $props();

  const own = $derived.by(() => {
    const p = app.profile;
    return !!p && sides.some((s) => layerOverrides(p, app.layer, kind, s).length > 0);
  });

  function reset() {
    app.edit((p) => {
      for (const s of sides) p = resetLayerPart(p, app.layer, kind, s);
      return p;
    });
  }
</script>

{#if app.layer !== 0}
  <LayerNote icon={Layers}>
    {t(own ? 'analog.layer.own' : 'analog.layer.follows', { layer: app.layerLabel(app.layer) })}
    {#snippet action()}
      {#if own && !app.readOnly}
        <button class="btn" onclick={reset}>{t('analog.layer.reset')}</button>
      {/if}
    {/snippet}
  </LayerNote>
{/if}

<style>
  .btn {
    margin-left: auto;
    padding: 3px 10px;
    font-size: var(--fs-s);
  }
</style>
