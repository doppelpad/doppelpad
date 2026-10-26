<script module lang="ts">
  import { platform } from '../../lib/state/window.svelte';
  import type { LiveState } from '../../lib/types';

  /** The drivers Doppelpad uses and whether each is installed (none before the first live state). On macOS
   * also the permission keyboard and mouse need, while virtual controllers do not count: macOS has none
   * to install. */
  export function driverStates(live: LiveState | null) {
    if (!live) return [];
    const drivers = [
      { key: 'virtual_pads' as const, state: live.drivers.virtual_pads },
      { key: 'device_hiding' as const, state: live.drivers.device_hiding },
    ];
    return platform === 'macos' ? [...drivers.slice(1), { key: 'input_access' as const, state: live.drivers.input_access }] : drivers;
  }
</script>

<script lang="ts">
  // The drivers and whether they are installed. `icon` (settings): a green or warning icon,
  // "missing" at the right and the error as tooltip; `row` (welcome guide): a missing driver's
  // row in the warning colour, with a download button. On macOS the permission keyboard and
  // mouse need instead, with a button to its settings, and virtual controllers as not there yet.
  // In the settings also Bluetooth, which only Switch 2 controllers over Bluetooth need, so it is
  // never missing: on macOS its permission, elsewhere whether it is there and on, with a button
  // to Windows' Bluetooth settings (Linux desktops have no one place for them).
  import { CircleAlert, CircleCheck } from '@lucide/svelte';
  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import { tip } from '../../lib/tooltip.svelte';
  import type { Driver } from '../../lib/types';

  let { tone }: { tone: 'icon' | 'row' } = $props();

  const drivers = $derived(driverStates(app.live));
  const mac = platform === 'macos';
  // Shown in the settings, where virtual controllers stay listed on macOS too.
  const macPads = $derived(
    mac && tone === 'icon' && app.live ? [{ key: 'virtual_pads' as const, state: app.live.drivers.virtual_pads }] : [],
  );
  const bluetooth = $derived(tone === 'icon' && app.live ? [{ key: 'bluetooth' as const, state: app.live.drivers.bluetooth }] : []);

  /** A permission the user gives in macOS's settings rather than a driver to install. */
  const permission = (key: Driver) => key === 'input_access' || key === 'bluetooth';

  function name(key: Driver) {
    return t(mac && !permission(key) ? `drivers.${key}_macos` : `drivers.${key}`);
  }

  function missing(key: Driver) {
    if (key === 'bluetooth' && !mac) return t('drivers.bluetooth_unavailable');
    if (permission(key)) return t('drivers.not_allowed');
    return t(mac ? 'drivers.not_on_macos' : 'drivers.missing');
  }
</script>

{#each [...macPads, ...drivers, ...bluetooth] as d (d.key)}
  <div class="driver {tone}" class:missing={!d.state.available} use:tip={tone === 'icon' ? (d.state.error ?? '') : undefined}>
    {#if d.state.available}<CircleCheck size={15} />{:else}<CircleAlert size={15} />{/if}
    <span class="name">{name(d.key)}</span>
    {#if !d.state.available}
      {#if tone === 'icon'}
        <span class="state">{missing(d.key)}</span>
        {#if d.key === 'input_access'}
          <button class="btn" onclick={() => app.attempt(api.openDriverPage(d.key))}>{t('drivers.open_settings')}</button>
        {:else if d.key === 'bluetooth' && platform !== 'linux'}
          <button class="btn" onclick={() => app.attempt(api.openDriverPage(d.key))}>
            {t(mac ? 'drivers.allow' : 'drivers.open_settings')}
          </button>
        {/if}
      {:else if d.key === 'input_access'}
        <button class="btn" onclick={() => app.attempt(api.openDriverPage(d.key))}>{t('drivers.open_settings')}</button>
      {:else if !mac}
        <button class="btn" onclick={() => app.attempt(api.openDriverPage(d.key))}>{t('drivers.download')}</button>
      {/if}
    {/if}
  </div>
{/each}
{#if bluetooth[0]?.state.available}
  <p class="hint">{t('drivers.bluetooth_hint')}</p>
{/if}

<style>
  .driver {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .icon {
    color: var(--text);
  }
  .icon :global(svg) {
    color: var(--ok);
  }
  .icon.missing :global(svg) {
    color: var(--warning);
  }
  .state {
    margin-left: auto;
    color: var(--warning);
    font-size: var(--fs-s);
  }
  .row.missing {
    color: var(--warning);
  }
  .row .name {
    flex: 1;
  }
</style>
