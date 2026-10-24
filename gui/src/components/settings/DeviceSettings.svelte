<script lang="ts">
  // Which controllers Doppelpad uses: devices it ignores completely, and groups of controllers
  // that act as one (one profile, one set of slots). Only connected devices can be grouped.
  import { Plus, Trash2 } from '@lucide/svelte';
  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import { padColor } from '../../lib/model/padColors';
  import type { DeviceGroup } from '../../lib/types';
  import Select from '../ui/Select.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingsSection from './SettingsSection.svelte';
  import { tip } from '../../lib/tooltip.svelte';

  const live = $derived(app.live);
  const seen = $derived(live?.seen_devices ?? []);
  const groups = $derived(live?.groups ?? []);
  const blocked = $derived(new Set(live?.blocked ?? []));
  const grouped = $derived(new Set(groups.flatMap((g) => g.members)));
  /** Names of devices seen now, else the id (a member that is not connected). */
  const nameOf = (id: string) => seen.find((d) => d.id === id)?.name ?? id;

  let name = $state('');
  let picked = $state<string[]>([]);
  /** Devices seen now, then ignored ones that are not connected (by id). */
  const listed = $derived([...seen, ...[...blocked].filter((id) => !seen.some((d) => d.id === id)).map((id) => ({ id, name: id }))]);
  const free = $derived(seen.filter((d) => !blocked.has(d.id) && !grouped.has(d.id)));
  const powerOffAfter = $derived(live?.power_off_after_min ?? 0);
  /** Idle timeouts offered, in minutes; 0 never turns controllers off. */
  const IDLE_MINUTES = [0, 5, 10, 15, 20, 30, 45, 60];

  /** The colour of a controller's tray icon. */
  function setColor(id: string, color: string) {
    return app.setSettings((s) => ({ controller_colors: { ...s.controller_colors, [id]: color } }));
  }

  function setGroups(next: DeviceGroup[]) {
    return app.attempt(api.setGroups(next));
  }

  async function create() {
    if (!(await setGroups([...groups, { name: name.trim(), members: picked }]))) return;
    name = '';
    picked = [];
  }
</script>

<SettingsSection title={t('devices.settings.title')} hint={t('devices.settings.hint')}>
  {#each listed as d (d.id)}
    <div class="dev-row">
      <label class="dev">
        <input type="checkbox" checked={!blocked.has(d.id)} onchange={(e) => app.attempt(api.setBlocked(d.id, !e.currentTarget.checked))} />
        <span class="name">{d.name}</span>
        {#if blocked.has(d.id)}<span class="tag">{t('devices.settings.ignored')}</span>{/if}
      </label>
      {#if app.settings.tray_per_controller}
        <input
          type="color"
          use:tip={t('devices.settings.tray_color')}
          aria-label={t('devices.settings.tray_color')}
          value={padColor(d.id, app.settings.controller_colors)}
          onchange={(e) => setColor(d.id, e.currentTarget.value)}
        />
      {/if}
    </div>
  {/each}

  <div class="sub">{t('devices.settings.groups')}</div>
  {#each groups as g, i (g.name)}
    <div class="group">
      <span class="gname">{g.name}</span>
      <span class="members">{g.members.map(nameOf).join(' + ')}</span>
      <button class="icon-btn" use:tip={t('devices.settings.ungroup')} onclick={() => setGroups(groups.filter((_, j) => j !== i))}>
        <Trash2 size={14} />
      </button>
    </div>
  {/each}
  {#if free.length >= 2}
    <div class="new">
      <input placeholder={t('devices.settings.group_name')} bind:value={name} />
      {#each free as d (d.id)}
        <label class="pick">
          <input
            type="checkbox"
            checked={picked.includes(d.id)}
            onchange={(e) => (picked = e.currentTarget.checked ? [...picked, d.id] : picked.filter((p) => p !== d.id))}
          />
          {d.name}
        </label>
      {/each}
      <button class="btn" disabled={!name.trim() || picked.length < 2} onclick={create}
        ><Plus size={14} /> {t('devices.settings.make_group')}</button
      >
    </div>
  {:else if groups.length === 0}
    <p class="hint">{t('devices.settings.group_hint')}</p>
  {/if}

  <SettingRow label={t('devices.settings.power_off_after')}>
    <Select
      value={powerOffAfter}
      class="setting"
      options={(IDLE_MINUTES.includes(powerOffAfter) ? IDLE_MINUTES : [...IDLE_MINUTES, powerOffAfter]).map((m) => ({
        value: m,
        label: m === 0 ? t('devices.settings.never') : t('devices.settings.minutes', { n: m }),
      }))}
      onchange={(m) => app.attempt(api.setPowerOffAfter(m))}
    />
  </SettingRow>
  <p class="hint">{t('devices.settings.power_off_hint')}</p>
</SettingsSection>

<style>
  .dev-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .dev,
  .pick {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tag {
    color: var(--warning);
    font-size: var(--fs-xs);
  }
  .sub {
    margin-top: 4px;
    color: var(--text-dim);
    font-weight: 600;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .gname {
    font-weight: 600;
  }
  .members {
    flex: 1;
    min-width: 0;
    color: var(--text-dim);
    font-size: var(--fs-s);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .new {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    border: 1px dashed var(--line-strong);
    border-radius: var(--radius-m);
  }
  .new .btn {
    align-self: flex-start;
  }
</style>
