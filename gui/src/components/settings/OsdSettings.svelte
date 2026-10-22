<script lang="ts">
  // The on-screen display: which events it shows, on which monitor and in which corner, how
  // big, how opaque and how long; a test button.
  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import type { OsdSettings } from '../../lib/types';
  import Slider from '../ui/Slider.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import CornerSelect from './CornerSelect.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingsSection from './SettingsSection.svelte';

  const osd = $derived(app.settings.osd);
  const EVENTS = ['remap', 'slot', 'layer', 'controller', 'battery'] as const;
  let monitors = $state<string[]>([]);
  api.osdMonitors().then((m) => (monitors = m));

  function set(patch: Partial<OsdSettings>) {
    app.setSettings({ osd: { ...osd, ...patch } });
  }
</script>

<SettingsSection title={t('settings.osd.title')} hint={t('settings.osd.hint')}>
  {#snippet actions()}
    <button class="btn" disabled={!osd.enabled} onclick={() => app.attempt(api.osdTest())}>{t('settings.osd.test')}</button>
  {/snippet}
  <Toggle checked={osd.enabled} label={t('settings.osd.enabled')} onchange={(v) => set({ enabled: v })} />
  {#if osd.enabled}
    <div class="events">
      {#each EVENTS as e (e)}
        <label class="check">
          <input type="checkbox" checked={osd[e]} onchange={(ev) => set({ [e]: ev.currentTarget.checked })} />
          <span>{t(`settings.osd.${e}`)}</span>
        </label>
        {#if e === 'layer'}
          <label class="check keep">
            <input
              type="checkbox"
              checked={osd.keep_layer}
              disabled={!osd.layer}
              onchange={(ev) => set({ keep_layer: ev.currentTarget.checked })}
            />
            <span>{t('settings.osd.keep_layer')}</span>
          </label>
        {/if}
      {/each}
    </div>
    <SettingRow label={t('settings.osd.monitor')}>
      <select value={osd.monitor} onchange={(e) => set({ monitor: Number(e.currentTarget.value) })}>
        {#each monitors as size, i (i)}
          <option value={i}>{i === 0 ? t('settings.osd.primary', { size }) : t('settings.osd.monitor_n', { n: i + 1, size })}</option>
        {/each}
      </select>
    </SettingRow>
    <CornerSelect value={osd.corner} onchange={(corner) => set({ corner })} />
    <Slider
      label={t('settings.osd.scale')}
      value={osd.scale}
      def={100}
      min={50}
      max={200}
      step={10}
      unit="%"
      onchange={(v) => set({ scale: v })}
    />
    <Slider
      label={t('settings.osd.opacity')}
      value={osd.opacity}
      def={80}
      min={20}
      max={100}
      step={5}
      unit="%"
      onchange={(v) => set({ opacity: v })}
    />
    <Slider
      label={t('settings.osd.seconds')}
      value={osd.seconds}
      def={5}
      min={1}
      max={30}
      step={1}
      unit="s"
      onchange={(v) => set({ seconds: v })}
    />
  {/if}
</SettingsSection>

<style>
  select {
    min-width: 160px;
  }
  .events {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  /* Belongs to "layer": indented under it. */
  .check.keep {
    margin-left: 24px;
  }
</style>
