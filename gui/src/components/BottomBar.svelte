<script lang="ts">
  import {
    BatteryCharging,
    BatteryFull,
    BatteryLow,
    BatteryMedium,
    BatteryWarning,
    ChevronDown,
    Gamepad2,
    Layers,
    Pause,
    Power,
    Settings,
    TriangleAlert,
    X,
  } from '@lucide/svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';
  import type { Battery, LiveDevice } from '../lib/types';
  import IdentifyWizard from './IdentifyWizard.svelte';
  import Menu, { type MenuItem } from './ui/Menu.svelte';

  const live = $derived(app.live);
  const set = $derived(app.slotSet);

  /** *Game › Config* for a config in a game, a loose config's name, else the file name. */
  function configName(path: string) {
    for (const g of app.library.games) {
      const c = g.configs.find((c) => c.path === path);
      if (c) return `${g.meta.name} › ${c.name}`;
    }
    const loose = app.library.unsorted.find((c) => c.path === path);
    return loose?.name ?? path.split(/[\\/]/).pop() ?? path;
  }

  const slotNames = $derived(set.slots.map((p) => (p ? configName(p) : null)));
  const ownIds = $derived(new Set((live?.device_slots ?? []).map((d) => d.id)));

  /** The controller the bar shows: one with slots of its own while they are on show, else the one in use. */
  const currentId = $derived(set.device ?? live?.primary ?? live?.devices[0]?.id ?? null);
  const current = $derived(live?.devices.find((d) => d.id === currentId) ?? null);
  const others = $derived((live?.devices ?? []).filter((d) => d.id !== currentId));

  /** Low or critical, for a battery that is not charging. */
  function batteryTone(b: Battery | null): 'warning' | 'danger' | undefined {
    if (!b || b.wired || b.charging) return undefined;
    return b.level < 20 ? 'danger' : b.level < 40 ? 'warning' : undefined;
  }

  /** The worst battery among the controllers the select does not show. */
  const othersTone = $derived.by(() => {
    const tones = others.map((d) => batteryTone(d.battery));
    return tones.includes('danger') ? 'danger' : tones.includes('warning') ? 'warning' : undefined;
  });

  const paused = $derived(!!(live?.suspended && live.remap_on && live.profile));
  const powerTitle = $derived.by(() => {
    if (!live) return '';
    const lines = [t(live.remap_on ? 'remap.on' : 'remap.off')];
    if (paused) lines.push(t(live.locked ? 'remap.paused_locked' : live.focus_paused ? 'remap.paused_focus' : 'remap.paused'));
    if (live.remap_on && live.profile) {
      const name = live.profile_path ? configName(live.profile_path) : live.profile;
      lines.push(t(live.auto_active ? 'remap.running_auto' : 'remap.running', { name }));
    }
    return lines.join('\n');
  });

  /** A controller with its own slots shows them; any other one uses (and drives) the shared ones. */
  function pick(d: LiveDevice) {
    if (ownIds.has(d.id)) {
      app.slotDevice = d.id;
    } else {
      app.slotDevice = null;
      app.attempt(api.setPreferredDevice(d.id));
    }
  }

  /** The device the identify wizard runs for. */
  let identify = $state<string | null>(null);

  const deviceItems = $derived<MenuItem[]>(
    (live?.devices ?? []).map((d) => ({
      label: d.name,
      checked: d.id === currentId,
      detail: d.battery && !d.battery.wired ? `${d.battery.level} %` : undefined,
      tone: batteryTone(d.battery),
      onclick: () => pick(d),
    })),
  );

  function deviceMenu(d: LiveDevice): MenuItem[] {
    const own = ownIds.has(d.id);
    return [
      {
        label: t('devices.own_slots'),
        checked: own,
        onclick: async () => {
          await app.attempt(api.ownSlots(d.id, !own));
          app.slotDevice = own ? null : d.id;
        },
      },
      // Generic pads: which button is which.
      ...(d.family === 'generic' && d.raw ? [{ label: t('identify.menu'), onclick: () => (identify = d.id) }] : []),
      // Only wireless controllers can be told to turn off.
      ...(d.battery && !d.battery.wired ? [{ label: t('devices.power_off'), onclick: () => app.attempt(api.powerOff(d.id)) }] : []),
      // A group's own id cannot be ignored; its members can, in the settings.
      ...(d.id.startsWith('group:')
        ? []
        : [{ label: t('devices.ignore'), onclick: () => app.attempt(api.setBlocked(d.id, true)) }]),
    ];
  }

  const slotMenu = $derived(
    [0, 1, 2, 3].map((i) => ({
      label: `${t('slots.slot', { n: i + 1 })}${slotNames[i] ? ' — ' + slotNames[i] : ''}`,
      checked: app.applySlot === i,
      onclick: () => (app.applySlot = i),
    })),
  );
  const canApply = $derived(!!app.configPath && !app.readOnly);
</script>

{#snippet battery(b: Battery)}
  {@const tone = batteryTone(b)}
  <span
    class="battery"
    class:low={tone === 'warning'}
    class:critical={tone === 'danger'}
    title={t(b.charging ? 'devices.battery_charging' : 'devices.battery', { level: b.level })}
  >
    {#if b.charging}<BatteryCharging size={14} />{:else if b.level < 20}<BatteryWarning size={14} />{:else if b.level < 40}<BatteryLow size={14} />{:else if b.level < 80}<BatteryMedium size={14} />{:else}<BatteryFull size={14} />{/if}
    {b.level} %
  </span>
{/snippet}

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
    <button
      class="power"
      class:on={live?.remap_on}
      aria-pressed={!!live?.remap_on}
      aria-label={t(live?.remap_on ? 'remap.on' : 'remap.off')}
      title={powerTitle}
      onclick={() => app.attempt(api.setRemap(!live?.remap_on))}
    >
      {#if paused}<Pause size={15} />{:else}<Power size={15} />{/if}
    </button>

    {#if !live || live.devices.length === 0}
      <span class="btn select empty"><Gamepad2 size={15} /><span class="name">{t('devices.none')}</span></span>
    {:else}
      <Menu items={deviceItems} title={ownIds.has(currentId ?? '') ? t('devices.own_hint') : t('devices.pick')}>
        {#snippet trigger()}
          <span class="btn select">
            {#if ownIds.has(currentId ?? '')}<Layers size={15} />{:else}<Gamepad2 size={15} />{/if}
            <span class="name">{current?.name ?? live?.seen_devices.find((d) => d.id === currentId)?.name ?? currentId}</span>
            {#if current?.battery && !current.battery.wired}{@render battery(current.battery)}{/if}
            {#if others.length}
              <span
                class="more"
                class:low={othersTone === 'warning'}
                class:critical={othersTone === 'danger'}
                title={others
                  .map((d) => (d.battery && !d.battery.wired ? `${d.name} — ${t('devices.battery', { level: d.battery.level })}` : d.name))
                  .join('\n')}
              >
                +{others.length}
              </span>
            {/if}
            <ChevronDown size={14} />
          </span>
        {/snippet}
      </Menu>
      {#if current}
        <Menu items={deviceMenu(current)} title={t('devices.options')}>
          {#snippet trigger()}<span class="btn square"><Settings size={15} /></span>{/snippet}
        </Menu>
      {/if}
    {/if}

    {#if live && !live.drivers.virtual_pads.available}
      <span class="warn" title={live.drivers.virtual_pads.error ?? ''}>
        <TriangleAlert size={13} /> ViGEmBus {t('drivers.missing')}
      </span>
    {/if}
  </div>

  <div class="right">
    <!-- Hovering it marks the slot it fills. -->
    <div class="apply" class:ready={canApply}>
      <button class="btn primary" disabled={!canApply} onclick={() => app.apply()}>
        {t('remap.apply_to', { n: app.applySlot + 1 })}
      </button>
      <Menu items={slotMenu} align="right" title={t('remap.pick_slot')}>
        {#snippet trigger()}<span class="btn primary slot-pick"><ChevronDown size={14} /></span>{/snippet}
      </Menu>
    </div>
    <div class="slots">
      {#each [0, 1, 2, 3] as i (i)}
        <div class="slot-wrap">
          <button
            class="slot"
            class:filled={!!slotNames[i]}
            class:target={app.applySlot === i}
            class:active={set.active === i && live?.remap_on && !!slotNames[i]}
            class:failed={set.active === i && live?.remap_on && !!slotNames[i] && !set.session && (set.device === null || !!live?.devices.some((d) => d.id === set.device))}
            title={slotNames[i] ?? t('slots.empty')}
            onclick={() => app.attempt(api.selectSlot(i, set.device))}
          >
            {i + 1}
          </button>
          {#if slotNames[i]}
            <button class="unload" title={t('slots.unload')} onclick={() => app.attempt(api.clearSlot(i, set.device))}><X size={10} /></button>
          {/if}
        </div>
      {/each}
    </div>
  </div>
</footer>

{#if identify}<IdentifyWizard deviceId={identify} onclose={() => (identify = null)} />{/if}

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
    background: color-mix(in srgb, var(--warning) 6%, transparent);
    border-top: 1px solid var(--line);
    font-size: var(--fs-s);
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
    gap: 8px;
    min-width: 0;
  }
  .right {
    gap: 14px;
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
    transition: background 0.12s, border-color 0.12s, color 0.12s;
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
  .select {
    gap: 7px;
    max-width: 340px;
    white-space: nowrap;
  }
  .select > :global(svg:first-child) {
    flex: none;
    color: var(--accent);
  }
  .select > :global(svg:last-child) {
    flex: none;
    color: var(--text-faint);
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .select.empty {
    color: var(--text-faint);
    cursor: default;
  }
  .select.empty:hover {
    background: var(--bg-2);
    border-color: var(--line);
  }
  .select.empty > :global(svg:first-child) {
    color: var(--text-faint);
  }
  .square {
    width: 32px;
    padding: 0;
    justify-content: center;
    color: var(--text-dim);
  }
  .battery {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--text-dim);
    font-size: var(--fs-xs);
  }
  .battery.low,
  .more.low {
    color: var(--warning);
  }
  .battery.critical,
  .more.critical {
    color: var(--danger);
  }
  .more {
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--line-strong);
    color: var(--text-dim);
    font-size: var(--fs-xs);
    font-weight: 600;
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
  .apply {
    display: inline-flex;
  }
  .apply > :global(.btn.primary) {
    border-radius: var(--radius-s) 0 0 var(--radius-s);
  }
  .slot-pick {
    padding: 0 7px;
    border-radius: 0 var(--radius-s) var(--radius-s) 0;
    border-left-color: color-mix(in srgb, var(--on-accent) 30%, transparent);
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
    border: 1px dashed var(--line-strong);
    color: var(--text-faint);
    font-weight: 600;
    transition: background 0.12s, border-color 0.12s, box-shadow 0.12s;
  }
  .slot:hover {
    border-color: var(--text-faint);
  }
  .slot.filled {
    border-style: solid;
    color: var(--text);
    background: var(--bg-2);
  }
  .slot.active {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .slot.active.failed {
    border-color: var(--warning);
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 12%, transparent);
  }
  .right:has(.apply.ready:hover, .apply.ready:focus-within) .slot.target {
    border-style: solid;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
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
