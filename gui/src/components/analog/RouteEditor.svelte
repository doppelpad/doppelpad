<script lang="ts">
  // The analog routes of the current layer, plus the ones it inherits from Main.
  import { Plus } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { controlLabel, controlLabelMixed } from '../../lib/model/controls';
  import { ROUTE_PRESETS, addRoute, layerRoutes, newRoute, overrideRoute, sourceOf, targetOf } from '../../lib/model/routes';
  import { app } from '../../lib/state/app.svelte';
  import type { AnalogMapping } from '../../lib/types';
  import Menu from '../ui/Menu.svelte';
  import RouteCard from './RouteCard.svelte';

  const refs = $derived(app.profile ? layerRoutes(app.profile, app.layer) : []);
  const own = $derived(refs.filter((r) => !r.inherited));
  const inherited = $derived(refs.filter((r) => r.inherited));

  function add(patch: Partial<AnalogMapping>) {
    const def = app.catalog?.defaults.analog;
    if (def) app.edit((p) => addRoute(p, newRoute(def, app.layer, patch)));
  }

  const presets = $derived(
    ROUTE_PRESETS.map((p) => ({
      label: t(`analog.routes.presets.${p.id}`, { control: controlLabel('LT', app.family) }),
      onclick: () => add(p.patch),
    })),
  );

  function routeName(r: AnalogMapping): string {
    return `${t(`analog.routes.sources.${sourceOf(r)}`)} → ${t(`analog.routes.targets.${targetOf(r)}`)}`;
  }
</script>

<div class="routes">
  <div class="head">
    <h3>{t('analog.routes.title', { layer: app.layerLabel(app.layer) })}</h3>
    {#if !app.readOnly}
      <Menu items={presets} align="right">
        {#snippet trigger()}<span class="btn primary"><Plus size={14} /> {t('analog.routes.add')}</span>{/snippet}
      </Menu>
    {/if}
  </div>
  <p class="intro">{t('analog.routes.intro')}</p>

  {#if own.length === 0}<p class="empty">{t('analog.routes.empty')}</p>{/if}
  {#each own as ref (ref.index)}
    <RouteCard index={ref.index} route={ref.route} />
  {/each}

  {#if inherited.length}
    <div class="inherited">
      <p class="note">{t('analog.routes.inherited_hint')}</p>
      {#each inherited as ref (ref.index)}
        <div class="inh">
          <span class="tag">{t('analog.routes.inherited')}</span>
          <span class="name">{routeName(ref.route)}</span>
          {#if ref.route.while_held?.length}
            <span class="when"
              >{t('analog.routes.while_held')}: {ref.route.while_held.map((c) => controlLabelMixed(c, app.family)).join(' + ')}</span
            >
          {/if}
          {#if !app.readOnly}
            <button class="btn" onclick={() => app.edit((p) => overrideRoute(p, ref.index, app.layer))}>
              {t('analog.routes.override')}
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .routes {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .intro,
  .empty,
  .note {
    margin: 0;
    color: var(--text-faint);
  }
  .intro {
    margin-top: -6px;
  }
  .inherited {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 6px;
  }
  .inh {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 10px;
    padding: 8px 10px 8px 12px;
    border: 1px dashed var(--line-strong);
    border-radius: var(--radius-m);
  }
  .tag {
    font-size: var(--fs-xs);
    padding: 1px 8px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--layer-0);
    color: var(--layer-0);
  }
  .name {
    font-weight: 500;
  }
  .when {
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
  .inh .btn {
    margin-left: auto;
  }
</style>
