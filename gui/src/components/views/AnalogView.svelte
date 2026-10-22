<script lang="ts">
  // Analog settings: shaping of each stick and of the triggers (for the whole config, or what
  // the current layer changes) and the analog routes of the current layer.
  import { t } from '../../lib/i18n/index.svelte';
  import { layerRoutes } from '../../lib/model/routes';
  import { app, type AnalogTab } from '../../lib/state/app.svelte';
  import GyroEditor from '../analog/GyroEditor.svelte';
  import RouteEditor from '../analog/RouteEditor.svelte';
  import StickEditor from '../analog/StickEditor.svelte';
  import TouchpadEditor from '../analog/TouchpadEditor.svelte';
  import TriggerEditor from '../analog/TriggerEditor.svelte';
  import VirtualStickEditor from '../analog/VirtualStickEditor.svelte';
  import Segmented from '../ui/Segmented.svelte';

  const TABS: AnalogTab[] = ['left', 'right', 'triggers', 'gyro', 'touchpad', 'routes'];
  const routeCount = $derived(app.profile ? layerRoutes(app.profile, app.layer).length : 0);
</script>

<div class="analog">
  <div class="head">
    <h2>{t('analog.title')}</h2>
    <Segmented
      label={t('analog.title')}
      options={TABS.map((tab) => ({ value: tab, label: t(`analog.tabs.${tab}`), badge: tab === 'routes' ? routeCount : undefined }))}
      value={app.analogTab}
      onchange={(tab) => (app.analogTab = tab)}
    />
  </div>

  {#if app.analogTab === 'routes'}
    <RouteEditor />
  {:else}
    {#if app.analogTab === 'gyro' || app.analogTab === 'touchpad'}<p class="scope">{t('analog.gyro.scope')}</p>
    {:else if app.layer === 0}<p class="scope">{t('analog.all_layers')}</p>{/if}
    {#if app.analogTab === 'touchpad'}
      <TouchpadEditor />
    {:else if app.analogTab === 'gyro'}
      <GyroEditor />
    {:else if app.analogTab === 'triggers'}
      <TriggerEditor />
    {:else}
      <StickEditor side={app.analogTab} />
      <VirtualStickEditor side={app.analogTab} />
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
  .scope {
    margin: -6px 0 4px;
    color: var(--text-faint);
  }
</style>
