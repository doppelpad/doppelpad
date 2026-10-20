<script lang="ts">
  import { Gamepad2, TriangleAlert, Power, Pause, X, ChevronDown } from '@lucide/svelte';
  import { api, errorText } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';
  import Menu from './ui/Menu.svelte';
  import Toggle from './ui/Toggle.svelte';

  const live = $derived(app.live);
  const slotNames = $derived(
    (live?.slots ?? [null, null, null, null]).map((p) => {
      if (!p) return null;
      for (const g of app.library.games) {
        const c = g.configs.find((c) => c.path === p);
        if (c) return `${g.meta.name} › ${c.name}`;
      }
      return p.split(/[\\/]/).pop() ?? p;
    }),
  );

  async function run(p: Promise<unknown>) {
    try {
      await p;
    } catch (e) {
      app.toast(errorText(e), 'error');
    }
  }

  const slotMenu = $derived(
    [0, 1, 2, 3].map((i) => ({
      label: `${t('slots.slot', { n: i + 1 })}${slotNames[i] ? ' — ' + slotNames[i] : ''}`,
      checked: app.applySlot === i,
      onclick: () => (app.applySlot = i),
    })),
  );
</script>

<!-- Problems the remapping service reports (devices, hiding, applying); English from Rust. -->
{#if live?.warnings.length}
  <ul class="service-warnings" aria-label={t('diagnostics.service')}>
    {#each live.warnings as w, i (i)}
      <li><TriangleAlert size={13} /><span>{w}</span></li>
    {/each}
  </ul>
{/if}

<footer class="bar">
  <div class="left">
    <div class="devices">
      {#if !live || live.devices.length === 0}
        <span class="none">{t('devices.none')}</span>
      {:else}
        {#each live.devices as d (d.id)}
          <button
            class="device"
            class:primary={d.id === live.primary}
            title={d.id === live.primary ? t('devices.in_use') : t('devices.use')}
            onclick={() => run(api.setPreferredDevice(d.id))}
          >
            <Gamepad2 size={15} />
            {d.name}
          </button>
        {/each}
      {/if}
      {#if live && !live.drivers.virtual_pads.available}
        <span class="warn" title={live.drivers.virtual_pads.error ?? ''}>
          <TriangleAlert size={13} /> ViGEmBus {t('drivers.missing')}
        </span>
      {/if}
    </div>

    <button
      class="remap"
      class:on={live?.remap_on}
      onclick={() => run(api.setRemap(!live?.remap_on))}
      title={live?.profile ?? ''}
    >
      <Power size={15} />
      {live?.remap_on ? t('remap.on') : t('remap.off')}
    </button>
    {#if live?.suspended && live?.remap_on && live?.profile}
      <span class="paused"><Pause size={12} /> {t('remap.paused')}</span>
    {/if}

    <div class="apply">
      <button class="btn primary" disabled={!app.configPath || app.readOnly} onclick={() => app.apply()}>
        {t('remap.apply_to', { n: app.applySlot + 1 })}
      </button>
      <Menu items={slotMenu}>
        {#snippet trigger()}<span class="btn slot-pick"><ChevronDown size={14} /></span>{/snippet}
      </Menu>
    </div>
  </div>

  <div class="right">
    <Toggle
      checked={live?.auto_switch ?? true}
      label={t('remap.autodetect')}
      onchange={(v) => run(api.setAutoSwitch(v))}
    />
    <div class="slots">
      {#each [0, 1, 2, 3] as i (i)}
        <div class="slot-wrap">
          <button
            class="slot"
            class:filled={!!slotNames[i]}
            class:active={live?.active_slot === i && live?.remap_on && !!slotNames[i]}
            class:failed={live?.active_slot === i && live?.remap_on && !!slotNames[i] && !live?.profile}
            title={slotNames[i] ?? t('slots.empty')}
            onclick={() => run(api.selectSlot(i))}
          >
            {i + 1}
          </button>
          {#if slotNames[i]}
            <button class="unload" title={t('slots.unload')} onclick={() => run(api.clearSlot(i))}><X size={10} /></button>
          {/if}
        </div>
      {/each}
    </div>
  </div>
</footer>

<style>
  .service-warnings {
    margin: 0;
    padding: 6px 16px;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-height: 88px;
    overflow-y: auto;
    background: rgba(245, 158, 11, 0.06);
    border-top: 1px solid var(--line);
    font-size: 12px;
    user-select: text;
  }
  .service-warnings li {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    color: var(--text-dim);
  }
  .service-warnings :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--warning);
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 16px;
    background: var(--bg-1);
    border-top: 1px solid var(--line);
  }
  .left,
  .right {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
  }
  .devices {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
  }
  .none {
    color: var(--text-faint);
  }
  .device {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    border-radius: var(--radius-s);
    color: var(--text-faint);
    white-space: nowrap;
  }
  .device.primary {
    color: var(--text);
    background: var(--bg-2);
  }
  .device.primary :global(svg) {
    color: var(--accent);
  }
  .warn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--warning);
    font-size: 12px;
    white-space: nowrap;
  }
  .remap {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    border-radius: 99px;
    border: 1px solid var(--line-strong);
    color: var(--text-dim);
    font-weight: 600;
    font-size: 12px;
    white-space: nowrap;
  }
  .remap.on {
    color: #04221f;
    background: var(--ok);
    border-color: var(--ok);
  }
  .paused {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--text-faint);
    white-space: nowrap;
  }
  .apply {
    display: inline-flex;
  }
  .apply :global(.btn.primary) {
    border-radius: var(--radius-s) 0 0 var(--radius-s);
  }
  .slot-pick {
    border-radius: 0 var(--radius-s) var(--radius-s) 0;
    padding: 6px 7px;
    border-left: none;
    height: 100%;
  }
  .slots {
    display: flex;
    gap: 8px;
  }
  .slot-wrap {
    position: relative;
  }
  .slot {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    border: 2px dashed var(--line-strong);
    color: var(--text-faint);
    font-weight: 600;
  }
  .slot.filled {
    border-style: solid;
    color: var(--text);
    background: var(--bg-2);
  }
  .slot.active {
    border-color: var(--accent);
    color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .slot.active.failed {
    border-color: var(--warning);
    color: var(--warning);
    box-shadow: 0 0 0 3px rgba(245, 158, 11, 0.15);
  }
  .unload {
    position: absolute;
    top: -4px;
    right: -4px;
    width: 15px;
    height: 15px;
    border-radius: 50%;
    background: var(--bg-3);
    border: 1px solid var(--line-strong);
    display: none;
    place-items: center;
    color: var(--text-dim);
  }
  .slot-wrap:hover .unload {
    display: grid;
  }
</style>
