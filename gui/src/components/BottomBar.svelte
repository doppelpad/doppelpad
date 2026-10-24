<script lang="ts">
  import { GamepadDirectional, Pause, Power, TriangleAlert } from '@lucide/svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { configName } from '../lib/model/labels';
  import { app } from '../lib/state/app.svelte';
  import { platform } from '../lib/state/window.svelte';
  import DevicePicker from './bottom/DevicePicker.svelte';
  import ServiceWarnings from './bottom/ServiceWarnings.svelte';
  import SlotBar from './bottom/SlotBar.svelte';
  import { tip } from '../lib/tooltip.svelte';

  const live = $derived(app.live);

  const paused = $derived(!!(live?.suspended && live.remap_on && live.profile));
  const powerTitle = $derived.by(() => {
    if (!live) return '';
    const lines = [t(live.remap_on ? 'remap.on' : 'remap.off')];
    if (paused) lines.push(t(live.locked ? 'remap.paused_locked' : live.focus_paused ? 'remap.paused_focus' : 'remap.paused'));
    if (live.remap_on && live.profile) {
      const name = live.profile_path ? configName(app.library, live.profile_path) : live.profile;
      lines.push(t(live.auto_active ? 'remap.running_auto' : 'remap.running', { name }));
    }
    return lines.join('\n');
  });
</script>

<ServiceWarnings />

<footer class="bar">
  <div class="left">
    <button
      class="power"
      class:on={live?.remap_on}
      aria-pressed={!!live?.remap_on}
      aria-label={t(live?.remap_on ? 'remap.on' : 'remap.off')}
      use:tip={powerTitle}
      onclick={() => app.attempt(api.setRemap(!live?.remap_on))}
    >
      {#if paused}<Pause size={15} />{:else}<Power size={15} />{/if}
    </button>

    <button
      class="power nav"
      class:on={app.settings.pad_navigation}
      aria-pressed={app.settings.pad_navigation}
      aria-label={t('padnav.toggle')}
      use:tip={t('padnav.hint')}
      onclick={() => app.setPadNavigation(!app.settings.pad_navigation)}
    >
      <GamepadDirectional size={15} />
    </button>

    <DevicePicker />

    {#if live && platform === 'macos'}
      {#if !live.drivers.input_access.available}
        <span class="warn" use:tip={live.drivers.input_access.error ?? ''}>
          <TriangleAlert size={13} />
          {t('drivers.not_allowed_named', { driver: t('drivers.input_access') })}
        </span>
      {/if}
    {:else if live && !live.drivers.virtual_pads.available}
      <span class="warn" use:tip={live.drivers.virtual_pads.error ?? ''}>
        <TriangleAlert size={13} />
        {t('drivers.missing_named', { driver: 'ViGEmBus' })}
      </span>
    {/if}
  </div>

  <SlotBar />
</footer>

<style>
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 16px;
    background: var(--bg-1);
    border-top: 1px solid var(--line);
  }
  .left {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  /* Circles switch something (remapping, slots); rectangles pick and act. All are 32 px high. */
  .bar :global(.btn) {
    height: 32px;
  }
  .power {
    flex: none;
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    margin-right: 4px;
    border-radius: 50%;
    border: 1px solid var(--line-strong);
    color: var(--text-faint);
    transition:
      background 0.12s,
      border-color 0.12s,
      color 0.12s;
  }
  .power:hover {
    border-color: var(--text-faint);
    color: var(--text-dim);
  }
  .power.on {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .power.on:hover {
    background: color-mix(in srgb, var(--accent) 24%, transparent);
  }
  .warn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: 6px;
    color: var(--warning);
    font-size: var(--fs-s);
    white-space: nowrap;
  }
</style>
