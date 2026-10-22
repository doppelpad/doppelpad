<script lang="ts">
  // The controller the bar shows (with its battery and how many others there are), the menu to
  // pick another, and the shown controller's options: own slots, identify, power off, ignore.
  import { ChevronDown, Gamepad2, Layers, Settings } from '@lucide/svelte';
  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { batteryTone } from '../../lib/model/devices';
  import { app } from '../../lib/state/app.svelte';
  import type { LiveDevice } from '../../lib/types';
  import IdentifyWizard from '../IdentifyWizard.svelte';
  import Menu, { type MenuItem } from '../ui/Menu.svelte';
  import BatteryBadge from './BatteryBadge.svelte';

  const live = $derived(app.live);
  const ownIds = $derived(new Set((live?.device_slots ?? []).map((d) => d.id)));

  /** The controller the bar shows: one with slots of its own while they are on show, else the one in use. */
  const currentId = $derived(app.slotSet.device ?? live?.primary ?? live?.devices[0]?.id ?? null);
  const current = $derived(live?.devices.find((d) => d.id === currentId) ?? null);
  const others = $derived((live?.devices ?? []).filter((d) => d.id !== currentId));

  /** The worst battery among the controllers the select does not show. */
  const othersTone = $derived.by(() => {
    const tones = others.map((d) => batteryTone(d.battery));
    return tones.includes('danger') ? 'danger' : tones.includes('warning') ? 'warning' : undefined;
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
</script>

{#if !live || live.devices.length === 0}
  <span class="btn select empty"><Gamepad2 size={15} /><span class="name">{t('devices.none')}</span></span>
{:else}
  <Menu items={deviceItems} title={ownIds.has(currentId ?? '') ? t('devices.own_hint') : t('devices.pick')}>
    {#snippet trigger()}
      <span class="btn select">
        {#if ownIds.has(currentId ?? '')}<Layers size={15} />{:else}<Gamepad2 size={15} />{/if}
        <span class="name">{current?.name ?? live?.seen_devices.find((d) => d.id === currentId)?.name ?? currentId}</span>
        {#if current?.battery && !current.battery.wired}<BatteryBadge battery={current.battery} />{/if}
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

{#if identify}<IdentifyWizard deviceId={identify} onclose={() => (identify = null)} />{/if}

<style>
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
  .more {
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--line-strong);
    color: var(--text-dim);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .more.low {
    color: var(--warning);
  }
  .more.critical {
    color: var(--danger);
  }
</style>
