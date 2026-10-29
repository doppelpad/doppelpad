<script lang="ts">
  // The tooltip of a layer chip: how the layer is reached (each mapping that changes to it,
  // with its activator, kind and delay, and on which layer it sits), whether it takes over
  // Main's mappings, what it gives back, and the radial menus it opens.
  import { t } from '../lib/i18n/index.svelte';
  import { comboLabel, joinLabel, jumpLabel } from '../lib/model/labels';
  import { layerEntrances, layerRadialMenus } from '../lib/model/layers';
  import { layerInheritance, layerUnlock } from '../lib/model/profile';
  import { placePopup, windowBox } from '../lib/place';
  import { app } from '../lib/state/app.svelte';

  /** `anchor`: the box of the chip it belongs to; it shows below it. */
  let { layer, anchor, id }: { layer: number; anchor: DOMRect; id: string } = $props();

  let box: HTMLDivElement | undefined = $state();
  let pos = $state<{ left: number; top: number } | null>(null);
  $effect(() => {
    if (!box) return;
    const place = placePopup(anchor, { width: box.offsetWidth, height: box.offsetHeight }, windowBox(), { gap: 6, edge: 4 });
    pos = { left: place.left, top: place.top };
  });

  const p = $derived(app.profile);
  const entrances = $derived(p ? layerEntrances(p, layer) : []);
  const radial = $derived(p ? layerRadialMenus(p, layer) : []);
  const inherit = $derived(p && layer !== 0 ? layerInheritance(p, layer).inherit : null);
  const unlock = $derived(p && layer !== 0 ? layerUnlock(p, layer) : undefined);
</script>

<div
  class="tip"
  role="tooltip"
  {id}
  bind:this={box}
  style:left="{pos?.left ?? 0}px"
  style:top="{pos?.top ?? 0}px"
  style:visibility={pos ? 'visible' : 'hidden'}
>
  <div class="name">{app.layerLabel(layer)}</div>
  {#if entrances.length}
    <ul>
      {#each entrances as e, i (i)}
        <li>
          <strong>{comboLabel(e.trigger, app.family)}</strong>
          · {joinLabel(t(`activators.${e.activator}`), jumpLabel(e))}
          {#if e.from !== layer}<span class="from">{t('layers.tip_from', { layer: app.layerLabel(e.from) })}</span>{/if}
        </li>
      {/each}
    </ul>
  {:else if layer !== 0}
    <p class="warn">{t('layers.tip_unreached')}</p>
  {/if}
  {#if radial.length}<p>{t('layers.tip_radial', { names: radial.join(', ') })}</p>{/if}
  {#if inherit !== null}<p class="dim">{t(inherit ? 'layers.tip_inherits' : 'layers.tip_no_inherit')}</p>{/if}
  {#if unlock}<p class="dim">{t(`layers.tip_unlock.${unlock}`)}</p>{/if}
</div>

<style>
  .tip {
    position: fixed;
    z-index: var(--z-menu);
    max-width: 340px;
    padding: 8px 10px;
    background: var(--bg-2);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow);
    font-size: var(--fs-s);
    pointer-events: none;
  }
  .name {
    font-weight: 600;
    margin-bottom: 4px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .from {
    color: var(--text-dim);
    margin-left: 4px;
  }
  p {
    margin: 4px 0 0;
  }
  .dim {
    color: var(--text-dim);
  }
  .warn {
    color: var(--warning);
  }
</style>
