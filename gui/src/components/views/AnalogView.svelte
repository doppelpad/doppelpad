<script lang="ts">
  // Analog settings: shaping of each stick and of the triggers (for the whole config, or what
  // the current layer changes) and the analog routes of the current layer.
  import { t } from '../../lib/i18n/index.svelte';
  import { layerRoutes } from '../../lib/model/analog';
  import { app, type AnalogTab } from '../../lib/state/app.svelte';
  import RouteEditor from '../analog/RouteEditor.svelte';
  import StickEditor from '../analog/StickEditor.svelte';
  import TriggerEditor from '../analog/TriggerEditor.svelte';

  const TABS: AnalogTab[] = ['left', 'right', 'triggers', 'routes'];
  const routeCount = $derived(app.profile ? layerRoutes(app.profile, app.layer).length : 0);
</script>

<div class="analog">
  <div class="head">
    <h2>{t('analog.title')}</h2>
    <div class="tabs" role="tablist">
      {#each TABS as tab (tab)}
        <button role="tab" aria-selected={app.analogTab === tab} class:active={app.analogTab === tab} onclick={() => (app.analogTab = tab)}>
          {t(`analog.tabs.${tab}`)}
          {#if tab === 'routes' && routeCount > 0}<span class="count">{routeCount}</span>{/if}
        </button>
      {/each}
    </div>
  </div>

  {#if app.analogTab === 'routes'}
    <RouteEditor />
  {:else}
    {#if app.layer === 0}<p class="scope">{t('analog.all_layers')}</p>{/if}
    {#if app.analogTab === 'triggers'}
      <TriggerEditor />
    {:else}
      <StickEditor side={app.analogTab} />
    {/if}
  {/if}
</div>

<style>
  .analog {
    container: analog / inline-size;
    max-width: 860px;
    margin: 0 auto;
    padding: 22px 24px 40px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 10px 16px;
  }
  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }
  .tabs {
    display: flex;
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
    overflow: hidden;
  }
  .tabs button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .tabs button + button {
    border-left: 1px solid var(--line);
  }
  .tabs button:hover {
    color: var(--text);
    background: var(--bg-2);
  }
  .tabs button.active {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .count {
    min-width: 16px;
    padding: 0 5px;
    border-radius: 99px;
    background: var(--bg-3);
    color: var(--text-dim);
    font-size: 10.5px;
    line-height: 16px;
    text-align: center;
  }
  .scope {
    margin: -6px 0 4px;
    color: var(--text-faint);
  }
</style>
