<script lang="ts">
  // The side panel of the radial menus view. For the sector picked in the ring: its label,
  // icon and colour, and what it does: steps that run in turn (each a tap, a press, a release
  // or held for a time, with pauses), or another menu to open. Without a picked sector it
  // lists the menu's sectors.
  import { open } from '@tauri-apps/plugin-dialog';
  import { ChevronRight, FolderOpen, Image, Info, Plus, Trash2, X } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { actionLabel } from '../../lib/model/labels';
  import { DEFAULT_SECTOR_COLOR, createSubmenu, removeSector, updateSector } from '../../lib/model/radial';
  import { app } from '../../lib/state/app.svelte';
  import { tip } from '../../lib/tooltip.svelte';
  import type { Action, RadialSector } from '../../lib/types';
  import StepList from '../binding/StepList.svelte';
  import Segmented from '../ui/Segmented.svelte';
  import Select from '../ui/Select.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import { RADIAL_ICONS, isImageIcon } from './icons';

  const menus = $derived(app.profile?.radial_menus ?? []);
  const i = $derived(app.radialIndex);
  const menu = $derived(menus[i]);
  const k = $derived(app.radialSector !== null && menu && app.radialSector < menu.sectors.length ? app.radialSector : null);
  const s = $derived<RadialSector | undefined>(k !== null ? menu?.sectors[k] : undefined);
  const disabled = $derived(app.readOnly);
  const steps = $derived<Action[]>(s?.actions ?? []);
  /** Run with the apply command: only then can a sector leave the menu open. */
  const byCommand = $derived((menu?.apply ?? app.catalog?.defaults.radial_menu.apply) === 'command');

  function set(patch: Partial<RadialSector>) {
    if (k !== null) app.edit((p) => updateSector(p, i, k, patch));
  }

  async function pickImage() {
    const file = await open({
      multiple: false,
      filters: [{ name: t('radial.image_filter'), extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'ico'] }],
    });
    if (typeof file === 'string') set({ icon: file });
  }

  const iconOptions = $derived([
    { value: '', label: t('radial.no_icon') },
    ...(s && isImageIcon(s.icon)
      ? [{ value: '__image', label: t('radial.image'), icon: Image, detail: s.icon?.split(/[\\/]/).pop() }]
      : []),
    { value: '__file', label: t('radial.pick_image'), icon: FolderOpen },
    { separator: true as const },
    ...Object.keys(RADIAL_ICONS).map((name) => ({ value: name, label: t(`radial.icons.${name}`), icon: RADIAL_ICONS[name] })),
  ]);

  function setIcon(v: string) {
    if (v === '__file') pickImage();
    else if (v !== '__image') set({ icon: v || undefined });
  }

  /** What a sector does: steps, or another menu. */
  type Does = 'actions' | 'submenu';
  const does = $derived<Does>(s?.submenu ? 'submenu' : 'actions');

  function setDoes(d: Does) {
    if (d === 'actions') set({ submenu: undefined });
    else {
      const other = menus.findIndex((_, j) => j !== i);
      if (other >= 0) set({ submenu: other + 1 });
      else newSubmenu();
    }
  }

  function newSubmenu() {
    if (k === null || !app.profile) return;
    app.update(createSubmenu(app.profile, i, k).profile);
  }

  const submenuOptions = $derived([
    ...menus.flatMap((m, j) => (j === i ? [] : [{ value: String(j + 1), label: m.name || t('radial.default_name', { n: j + 1 }) }])),
    { separator: true as const },
    { value: 'new', label: t('radial.new_submenu'), icon: Plus },
  ]);

  function setSubmenu(v: string) {
    if (v === 'new') newSubmenu();
    else set({ submenu: Number(v) });
  }

  /** A sector's actions in short, for the list of sectors. */
  function summary(x: RadialSector): string {
    if (x.submenu) return t('radial.opens', { name: menus[x.submenu - 1]?.name || String(x.submenu) });
    const acts = (x.actions ?? []).filter((a) => !('pause' in a));
    return acts.length ? acts.map(actionLabel).join(', ') : t('radial.does_nothing');
  }
</script>

<div class="panel">
  {#if !menu}
    <div class="intro">
      <Info size={16} />
      <p>{t('radial.panel_empty')}</p>
    </div>
  {:else if k === null || !s}
    <div class="intro">
      <Info size={16} />
      <p>{t('radial.pick_sector')}</p>
    </div>
    <div class="section-title">{t('radial.sectors', { name: menu.name || String(i + 1) })}</div>
    <div class="list">
      {#each menu.sectors as x, j (j)}
        {@const Icon = x.icon && !isImageIcon(x.icon) ? RADIAL_ICONS[x.icon] : isImageIcon(x.icon) ? Image : undefined}
        <button class="row" onclick={() => (app.radialSector = j)}>
          <span class="num">{j + 1}</span>
          <span class="ic" style:color={x.color || 'var(--accent)'}
            >{#if Icon}<Icon size={14} />{/if}</span
          >
          <span class="name">{x.label || t('radial.sector_n', { n: j + 1 })}</span>
          <span class="sum">{summary(x)}</span>
          <ChevronRight size={14} />
        </button>
      {/each}
    </div>
  {:else}
    <div class="head">
      <div class="title">
        <span class="name">{s.label || t('radial.sector_n', { n: k + 1 })}</span>
        <span class="sub">{t('radial.sector_of', { n: k + 1, name: menu.name || String(i + 1) })}</span>
      </div>
      <button class="icon-btn" use:tip={t('common.close')} onclick={() => (app.radialSector = null)}><X size={16} /></button>
    </div>

    <label class="field">
      <span>{t('radial.label')}</span>
      <input
        value={s.label ?? ''}
        placeholder={t('radial.sector_n', { n: k + 1 })}
        {disabled}
        onchange={(e) => set({ label: e.currentTarget.value.trim() || undefined })}
      />
    </label>
    <div class="look">
      <label class="field grow">
        <span>{t('radial.icon')}</span>
        <Select value={isImageIcon(s.icon) ? '__image' : (s.icon ?? '')} {disabled} wide options={iconOptions} onchange={setIcon} />
      </label>
      <label class="field">
        <span>{t('radial.color')}</span>
        <input type="color" value={s.color || DEFAULT_SECTOR_COLOR} {disabled} onchange={(e) => set({ color: e.currentTarget.value })} />
      </label>
    </div>

    <div class="section-title">{t('radial.submenu')}</div>
    <Segmented
      variant="fill"
      label={t('radial.submenu')}
      options={[
        { value: 'actions', label: t('radial.does_actions') },
        { value: 'submenu', label: t('radial.does_submenu') },
      ]}
      value={does}
      {disabled}
      onchange={setDoes}
    />

    {#if does === 'submenu'}
      <div class="sub-row">
        <Select value={String(s.submenu)} {disabled} wide options={submenuOptions} onchange={setSubmenu} />
        <button class="btn" onclick={() => s.submenu && app.reveal({ kind: 'radial', menu: s.submenu - 1, sector: null })}>
          {t('radial.edit_submenu')}
        </button>
      </div>
    {:else}
      <p class="hint">{t('radial.steps_hint')}</p>
      <!-- A sector picked anew starts without the action picker. -->
      {#key k}
        <StepList
          {steps}
          {disabled}
          narrow
          empty={t('radial.no_steps')}
          onchange={(next) => set({ actions: next.length ? next : undefined })}
        />
      {/key}
    {/if}

    {#if byCommand}
      <Toggle
        checked={s.close ?? true}
        {disabled}
        label={t('radial.close_after')}
        onchange={(v) => set({ close: v ? undefined : false })}
      />
    {/if}

    {#if !disabled && menu.sectors.length > 1}
      <button
        class="btn danger"
        onclick={() => {
          if (k === null) return;
          app.edit((p) => removeSector(p, i, k));
          app.radialSector = null;
        }}><Trash2 size={14} /> {t('radial.remove_sector')}</button
      >
    {/if}
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 14px 20px;
  }
  .intro {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    color: var(--text-dim);
    background: var(--bg-2);
    border-radius: var(--radius-m);
    padding: 10px 12px;
  }
  .intro p {
    margin: 0;
  }
  .section-title {
    margin-top: 6px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row {
    display: grid;
    grid-template-columns: 14px 16px minmax(0, 1fr) minmax(0, 1.2fr) 14px;
    align-items: center;
    gap: 8px;
    padding: 7px 8px;
    border-radius: var(--radius-s);
    text-align: left;
    color: var(--text-dim);
  }
  .row:hover {
    background: var(--bg-2);
    color: var(--text);
  }
  .num {
    color: var(--text-faint);
    font-size: var(--fs-xs);
  }
  .ic {
    display: inline-flex;
  }
  .row .name {
    color: var(--text);
  }
  .row .name,
  .sum {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
  }
  .title {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .title .name {
    font-size: 16px;
    font-weight: 600;
  }
  .title .sub {
    color: var(--text-faint);
    font-size: var(--fs-s);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: var(--fs-xs);
    color: var(--text-faint);
  }
  .look {
    display: flex;
    gap: 12px;
    align-items: flex-end;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .look input[type='color'] {
    width: 44px;
    height: 32px;
  }
  .sub-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .sub-row > :global(.menu) {
    flex: 1;
    min-width: 0;
  }

  .danger {
    align-self: flex-start;
    color: var(--danger);
    margin-top: 6px;
  }
</style>
