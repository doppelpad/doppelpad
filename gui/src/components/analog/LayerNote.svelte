<script lang="ts">
  // On a layer other than Main: says whether the settings below are the layer's own or
  // Main's, and lets the layer follow Main again.
  import { Layers } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { layerOverrides, resetLayerPart, type AnalogKind, type Side } from '../../lib/model/analog';
  import { app } from '../../lib/state/app.svelte';

  let { kind, sides }: { kind: AnalogKind; sides: Side[] } = $props();

  const own = $derived.by(() => {
    const p = app.profile;
    return !!p && sides.some((s) => layerOverrides(p, app.layer, kind, s).length > 0);
  });

  function reset() {
    let p = app.profile;
    if (!p) return;
    for (const s of sides) p = resetLayerPart(p, app.layer, kind, s);
    app.update(p);
  }
</script>

{#if app.layer !== 0}
  <div class="note">
    <Layers size={14} />
    <span>{t(own ? 'analog.layer.own' : 'analog.layer.follows', { layer: app.layerLabel(app.layer) })}</span>
    {#if own && !app.readOnly}
      <button class="btn" onclick={reset}>{t('analog.layer.reset')}</button>
    {/if}
  </div>
{/if}

<style>
  .note {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 12px;
    color: var(--layer-2);
    background: color-mix(in srgb, var(--layer-2) 10%, transparent);
    border-radius: var(--radius-s);
    padding: 6px 8px;
  }
  .note .btn {
    margin-left: auto;
    padding: 3px 10px;
    font-size: 12px;
  }
</style>
