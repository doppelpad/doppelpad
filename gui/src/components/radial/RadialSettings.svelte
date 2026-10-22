<script lang="ts">
  // The config's radial menus: the layer that opens each one, how it is pointed at and when
  // the pointed sector runs, and per sector its label, icon, colour, actions or sub-menu.
  import { open } from '@tauri-apps/plugin-dialog';
  import { Plus, Trash2, X } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { layerIds } from '../../lib/model/profile';
  import { actionLabel } from '../../lib/model/labels';
  import {
    DEFAULT_SECTOR_COLOR,
    MAX_SECTORS,
    RING,
    addMenu,
    addSector,
    firstFreeLayer,
    removeMenu,
    removeSector,
    sectorCentre,
    sectorPath,
    updateMenu,
    updateSector,
  } from '../../lib/model/radial';
  import { RADIAL_ICONS, isImageIcon } from './icons';
  import { app } from '../../lib/state/app.svelte';
  import type { Action, RadialApply, RadialMenu, RadialSelect } from '../../lib/types';
  import ActionPicker from '../binding/ActionPicker.svelte';
  import Slider from '../ui/Slider.svelte';

  const menus = $derived(app.profile?.radial_menus ?? []);
  const def = $derived(app.catalog?.defaults.radial_menu);
  const disabled = $derived(app.readOnly);
  const layers = $derived(app.profile ? layerIds(app.profile).filter((l) => l !== 0) : []);
  const SELECTS: RadialSelect[] = ['right_stick', 'left_stick', 'mouse', 'commands'];
  const APPLIES: RadialApply[] = ['release', 'command'];
  const ICONS = Object.keys(RADIAL_ICONS);

  /** Sector whose action picker is open, as `menu:sector`. */
  let picking = $state<string | null>(null);

  function setMenu(i: number, patch: Partial<RadialMenu>) {
    app.edit((p) => updateMenu(p, i, patch));
  }

  function addAction(i: number, k: number, a: Action) {
    const s = app.profile?.radial_menus?.[i]?.sectors[k];
    if (s) app.edit((p) => updateSector(p, i, k, { actions: [...(s.actions ?? []), a] }));
  }

  async function pickImage(i: number, k: number) {
    const file = await open({ multiple: false, filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'ico'] }] });
    if (typeof file === 'string') app.edit((p) => updateSector(p, i, k, { icon: file }));
  }

  function newMenu() {
    app.edit((p) => addMenu(p, firstFreeLayer(p)));
  }
</script>

<div class="section-title">{t('radial.title')}</div>
<p class="hint">{t('radial.hint')}</p>

{#each menus as m, i (i)}
  {@const n = m.sectors.length}
  <section class="menu card">
    <header>
      <svg class="preview" viewBox="-104 -104 208 208" aria-hidden="true">
        {#each m.sectors as s, k (k)}
          <path d={sectorPath(k, n, 34, 100)} fill={s.color || 'var(--accent)'} fill-opacity="0.25" stroke="var(--line-strong)" stroke-width="3" />
          {@const [cx, cy] = sectorCentre(k, n, 67)}
          <text x={cx} y={cy + 6} text-anchor="middle" class="pnum">{k + 1}</text>
        {/each}
      </svg>
      <span class="num">{i + 1}</span>
      <input class="name" value={m.name ?? ''} placeholder={t('radial.name')} {disabled} onchange={(e) => setMenu(i, { name: e.currentTarget.value.trim() || undefined })} />
      {#if !disabled}
        <button class="icon-btn del danger" title={t('radial.remove')} onclick={() => app.edit((p) => removeMenu(p, i))}><Trash2 size={15} /></button>
      {/if}
    </header>

    <div class="row">
      <label class="field">
        <span>{t('radial.layer')}</span>
        <select value={m.layer ?? def?.layer ?? 1} {disabled} onchange={(e) => setMenu(i, { layer: Number(e.currentTarget.value) })}>
          <option value={0}>{t('radial.submenu_only')}</option>
          {#each layers as l (l)}<option value={l}>{app.layerLabel(l)}</option>{/each}
        </select>
      </label>
      <label class="field">
        <span>{t('radial.select')}</span>
        <select value={m.select ?? def?.select} {disabled} onchange={(e) => setMenu(i, { select: e.currentTarget.value as RadialSelect })}>
          {#each SELECTS as s (s)}<option value={s}>{t(`radial.selects.${s}`)}</option>{/each}
        </select>
      </label>
      <label class="field">
        <span>{t('radial.apply')}</span>
        <select value={m.apply ?? def?.apply} {disabled} onchange={(e) => setMenu(i, { apply: e.currentTarget.value as RadialApply })}>
          {#each APPLIES as a (a)}<option value={a}>{t(`radial.applies.${a}`)}</option>{/each}
        </select>
      </label>
    </div>
    {#if layers.length === 0}<p class="warn">{t('radial.no_layers')}</p>{/if}

    <div class="fields">
      <Slider label={t('radial.delay')} value={m.delay_ms ?? 0} def={def?.delay_ms} min={0} max={1000} step={50} unit="ms" hint={t('radial.delay_hint')} {disabled} onchange={(v) => setMenu(i, { delay_ms: v || undefined })} />
      <Slider label={t('radial.tint')} value={m.tint ?? 0} def={def?.tint} min={0} max={0.9} step={0.05} scale={100} unit="%" {disabled} onchange={(v) => setMenu(i, { tint: v || undefined })} />
      <Slider label={t('radial.scale')} value={m.scale ?? 1} def={def?.scale} min={0.5} max={2} step={0.1} scale={100} unit="%" hint={t('radial.scale_hint', { px: Math.round(RING * 2 * (m.scale ?? 1)) })} {disabled} onchange={(v) => setMenu(i, { scale: v === 1 ? undefined : v })} />
    </div>

    <div class="sectors">
      {#each m.sectors as s, k (k)}
        {@const key = `${i}:${k}`}
        {@const Icon = s.icon ? RADIAL_ICONS[s.icon] : undefined}
        <div class="sector">
          <span class="snum">{k + 1}</span>
          <input class="slabel" value={s.label ?? ''} placeholder={t('radial.label')} {disabled} onchange={(e) => app.edit((p) => updateSector(p, i, k, { label: e.currentTarget.value.trim() || undefined }))} />
          <span class="icon">{#if Icon}<Icon size={16} />{/if}</span>
          <select
            value={isImageIcon(s.icon) ? '__image' : (s.icon ?? '')}
            {disabled}
            title={isImageIcon(s.icon) ? s.icon : ''}
            onchange={(e) => {
              const v = e.currentTarget.value;
              if (v === '__file') pickImage(i, k);
              else if (v !== '__image') app.edit((p) => updateSector(p, i, k, { icon: v || undefined }));
            }}
          >
            <option value="">{t('radial.no_icon')}</option>
            {#if isImageIcon(s.icon)}<option value="__image">{t('radial.image')}</option>{/if}
            {#each ICONS as name (name)}<option value={name}>{name}</option>{/each}
            <option value="__file">{t('radial.pick_image')}</option>
          </select>
          <input type="color" value={s.color || DEFAULT_SECTOR_COLOR} {disabled} onchange={(e) => app.edit((p) => updateSector(p, i, k, { color: e.currentTarget.value }))} />
          <select
            value={s.submenu ?? 0}
            {disabled}
            title={t('radial.submenu')}
            onchange={(e) => {
              const v = Number(e.currentTarget.value);
              app.edit((p) => updateSector(p, i, k, { submenu: v || undefined }));
            }}
          >
            <option value={0}>{t('radial.actions')}</option>
            {#each menus as other, j (j)}
              {#if j !== i}<option value={j + 1}>{t('radial.opens', { name: other.name || String(j + 1) })}</option>{/if}
            {/each}
          </select>
          {#if !disabled && n > 1}
            <button class="icon-btn" title={t('radial.remove_sector')} onclick={() => app.edit((p) => removeSector(p, i, k))}><Trash2 size={14} /></button>
          {/if}
          {#if !s.submenu}
            <div class="acts">
              {#each s.actions ?? [] as a, ai (ai)}
                <span class="chip">
                  {actionLabel(a)}
                  {#if !disabled}
                    <button class="chip-x" title={t('common.remove')} onclick={() => app.edit((p) => updateSector(p, i, k, { actions: (s.actions ?? []).filter((_, x) => x !== ai) }))}><X size={11} /></button>
                  {/if}
                </span>
              {/each}
              {#if !disabled}
                <button class="add-act" onclick={() => (picking = picking === key ? null : key)}><Plus size={12} /> {t('radial.add_action')}</button>
              {/if}
            </div>
            {#if picking === key}
              <div class="picker"><ActionPicker onpick={(a) => addAction(i, k, a)} onclose={() => (picking = null)} /></div>
            {/if}
          {/if}
        </div>
      {/each}
      {#if !disabled && n < MAX_SECTORS}
        <button class="btn add" onclick={() => app.edit((p) => addSector(p, i))}><Plus size={14} /> {t('radial.add_sector')}</button>
      {/if}
    </div>
  </section>
{/each}

{#if !disabled}
  <button class="btn add" onclick={newMenu}><Plus size={14} /> {t('radial.add')}</button>
{/if}

<style>
  .section-title {
    margin-top: 10px;
  }
  .hint {
    margin: 0;
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
  .warn {
    margin: 0;
    color: var(--warning);
    font-size: var(--fs-s);
  }
  .menu {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px 16px 16px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .preview {
    width: 64px;
    height: 64px;
    flex: none;
  }
  .pnum {
    fill: var(--text-dim);
    font-size: 18px;
    font-weight: 600;
  }
  .num,
  .snum {
    color: var(--text-faint);
    font-weight: 600;
  }
  .name {
    flex: 1;
    min-width: 0;
  }
  .del {
    margin-left: auto;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 16px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: var(--fs-xs);
    color: var(--text-faint);
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 14px 24px;
  }
  .sectors {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 10px;
    border-top: 1px solid var(--line);
  }
  .sector {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
  }
  .slabel {
    width: 120px;
  }
  .icon {
    display: inline-grid;
    place-items: center;
    width: 18px;
    color: var(--text-dim);
  }
  .sector input[type='color'] {
    width: 30px;
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    flex-basis: 100%;
    padding-left: 22px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px 2px 10px;
    border-radius: var(--radius-pill);
    background: var(--bg-3);
    border: 1px solid var(--line-strong);
    font-size: var(--fs-s);
  }
  .chip-x {
    display: inline-grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    color: var(--text-dim);
  }
  .chip-x:hover {
    background: var(--danger);
    color: var(--on-danger);
  }
  .add-act {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--accent);
    font-size: var(--fs-s);
  }
  .picker {
    flex-basis: 100%;
    padding-left: 22px;
  }
  .add {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
</style>
