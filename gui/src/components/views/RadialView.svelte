<script lang="ts">
  // The config's radial menus, one at a time: the ring (click a sector to edit it in the side
  // panel), the button that opens the menu, how it is pointed at and when the pointed sector
  // runs, where it sits on the screen and how it looks. A new menu brings its own layer, so
  // choosing the button is all it takes to open it.
  import { ChartPie, Plus, Trash2, TriangleAlert } from '@lucide/svelte';
  import { onDestroy } from 'svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { controlLabelMixed } from '../../lib/model/controls';
  import { comboLabel, mappingSummary } from '../../lib/model/labels';
  import { layerIds, mappingAt } from '../../lib/model/profile';
  import {
    MAX_SECTORS,
    RING,
    addSector,
    createMenu,
    deleteMenu,
    giveOwnLayer,
    openers,
    parentsOf,
    setOpener,
    updateMenu,
  } from '../../lib/model/radial';
  import { arrowKeys } from '../../lib/dom';
  import { app } from '../../lib/state/app.svelte';
  import { controlChoiceItems, createControlCapture } from '../../lib/state/controlCapture.svelte';
  import { layerColor } from '../../lib/theme/layers';
  import type { ControlName, RadialApply, RadialMenu, RadialSelect } from '../../lib/types';
  import PositionPicker from '../radial/PositionPicker.svelte';
  import RingEditor from '../radial/RingEditor.svelte';
  import Chip from '../ui/Chip.svelte';
  import Menu from '../ui/Menu.svelte';
  import Select from '../ui/Select.svelte';
  import Slider from '../ui/Slider.svelte';

  const menus = $derived(app.profile?.radial_menus ?? []);
  const index = $derived(app.radialIndex);
  const menu = $derived<RadialMenu | undefined>(menus[index]);
  const def = $derived(app.catalog?.defaults.radial_menu);
  const disabled = $derived(app.readOnly);
  const layer = $derived(menu?.layer ?? def?.layer ?? 1);
  const opened = $derived(app.profile && menu ? openers(app.profile, layer) : []);
  const parents = $derived(app.profile && menu ? parentsOf(app.profile, index) : []);

  const SELECTS: RadialSelect[] = ['right_stick', 'left_stick', 'mouse', 'commands'];
  const APPLIES: RadialApply[] = ['release', 'command'];

  function setMenu(patch: Partial<RadialMenu>) {
    app.edit((p) => updateMenu(p, index, patch));
  }

  function newMenu() {
    if (!app.profile) return;
    const made = createMenu(app.profile);
    app.update(made.profile);
    app.radialMenu = made.index;
    app.radialSector = null;
  }

  function showMenu(i: number) {
    app.radialMenu = i;
    app.radialSector = null;
  }

  async function remove() {
    if (!menu || !(await app.confirmAsk(t('radial.remove_confirm', { name: menu.name || String(index + 1) })))) return;
    app.edit((p) => deleteMenu(p, index));
    app.radialMenu = Math.max(0, index - 1);
    app.radialSector = null;
  }

  /** The layers a menu can open with, a new one, or none (a sub-menu only). */
  const layerOptions = $derived([
    ...(app.profile ? layerIds(app.profile).filter((l) => l !== 0) : []).map((l) => ({
      value: String(l),
      label: app.layerLabel(l),
      color: layerColor(l),
    })),
    { separator: true as const },
    { value: 'new', label: t('layers.add'), icon: Plus },
    { value: '0', label: t('radial.submenu_only') },
  ]);

  function setLayer(v: string) {
    if (v === 'new') app.edit((p) => giveOwnLayer(p, index));
    else setMenu({ layer: Number(v) });
  }

  // ----- the button that opens the menu ---------------------------------------------------

  /** Make `c` (held on Main) open the menu; asks first when `c` already does something there. */
  async function chooseOpener(c: ControlName) {
    const p = app.profile;
    if (!p || !menu) return;
    const own = mappingAt(p, 0, c, 'single').own;
    const busy = own && ((own.actions ?? []).length > 0 || (own.jump && own.jump.layer !== layer));
    if (
      busy &&
      !(await app.confirmAsk(
        t('radial.opener_replace', { control: controlLabelMixed(c, app.family), what: mappingSummary(own, app.layerLabel) }),
      ))
    ) {
      return;
    }
    app.edit((q) => setOpener(q, layer, c));
  }

  const capture = createControlCapture(chooseOpener);
  onDestroy(capture.stop);

  const openerItems = $derived(
    controlChoiceItems(app.catalog, app.family, chooseOpener, {
      press: t('radial.press_opener'),
      onpress: capture.start,
      extra: ['LT', 'RT'],
    }),
  );
</script>

<div class="radial">
  <header class="top">
    <h2>{t('radial.title')}</h2>
    <p class="hint">{t('radial.hint')}</p>
  </header>

  {#if menus.length === 0 || !menu}
    <div class="empty card">
      <ChartPie size={44} strokeWidth={1.3} />
      <strong>{t('radial.empty_title')}</strong>
      <p class="hint">{t('radial.empty_hint')}</p>
      {#if !disabled}
        <button class="btn primary" onclick={newMenu}><Plus size={15} /> {t('radial.add')}</button>
      {/if}
    </div>
  {:else}
    <div class="tabs">
      <div class="tab-list" role="tablist" aria-label={t('radial.title')} use:arrowKeys>
        {#each menus as m, i (i)}
          <button
            role="tab"
            class="tab"
            class:on={i === index}
            aria-selected={i === index}
            tabindex={i === index ? 0 : -1}
            onclick={() => showMenu(i)}
          >
            {m.name || t('radial.default_name', { n: i + 1 })}
            {#if m.layer === 0}<span class="sub">{t('radial.sub')}</span>{/if}
          </button>
        {/each}
      </div>
      {#if !disabled}
        <button class="tab add" onclick={newMenu}><Plus size={14} /> {t('radial.new')}</button>
      {/if}
    </div>

    <div class="editor">
      <div class="ring-col">
        <RingEditor {menu} selected={app.radialSector} onselect={(k) => (app.radialSector = k)} />
        <p class="hint center">{t('radial.pick_sector')}</p>
        {#if !disabled && menu.sectors.length < MAX_SECTORS}
          <button class="btn" onclick={() => app.edit((p) => addSector(p, index))}><Plus size={14} /> {t('radial.add_sector')}</button>
        {/if}
      </div>

      <div class="settings">
        <label class="field">
          <span class="label">{t('radial.name')}</span>
          <input
            value={menu.name ?? ''}
            placeholder={t('radial.default_name', { n: index + 1 })}
            {disabled}
            onchange={(e) => setMenu({ name: e.currentTarget.value.trim() || undefined })}
          />
        </label>

        <section>
          <div class="section-title">{t('radial.opens_with')}</div>
          {#if layer === 0}
            <p class="hint">
              {#if parents.length}
                {t('radial.opens_from', {
                  list: parents
                    .map((x) => `${menus[x.menu]?.name || x.menu + 1} › ${menus[x.menu]?.sectors[x.sector]?.label || x.sector + 1}`)
                    .join(', '),
                })}
              {:else}
                {t('radial.opens_from_none')}
              {/if}
            </p>
            {#if !disabled}
              <button class="btn own" onclick={() => app.edit((p) => giveOwnLayer(p, index))}>{t('radial.own_button')}</button>
            {/if}
          {:else}
            <div class="openers">
              {#each opened as m, i (i)}
                <Chip size="s">{comboLabel(m.trigger, app.family)}</Chip>
                {#if m.layer && m.layer !== 0}<span class="dim">{t('radial.on_layer', { layer: app.layerLabel(m.layer) })}</span>{/if}
              {/each}
              {#if capture.active}
                <span class="capturing">{t('radial.pressing')}</span>
                <button class="btn" onclick={capture.stop}>{t('common.cancel')}</button>
              {:else if !disabled}
                <Menu items={openerItems} class="btn">
                  {#snippet trigger()}{opened.length ? t('radial.change_opener') : t('radial.choose_opener')}{/snippet}
                </Menu>
              {/if}
            </div>
            {#if opened.length === 0 && !capture.active}
              <p class="warn"><TriangleAlert size={14} /> {t('radial.no_opener')}</p>
            {:else if opened.length}
              <p class="hint">{t('radial.opener_hint')}</p>
            {/if}
          {/if}
          <div class="row">
            <span class="label">{t('radial.layer')}</span>
            <Select value={String(layer)} {disabled} size="s" label={t('radial.layer')} options={layerOptions} onchange={setLayer} />
          </div>
        </section>

        <section>
          <div class="section-title">{t('radial.control')}</div>
          <div class="row">
            <span class="label">{t('radial.select')}</span>
            <Select
              value={menu.select ?? def?.select ?? 'right_stick'}
              label={t('radial.select')}
              {disabled}
              class="wide-select"
              options={SELECTS.map((s) => ({ value: s, label: t(`radial.selects.${s}`) }))}
              onchange={(select) => setMenu({ select })}
            />
          </div>
          <div class="row">
            <span class="label">{t('radial.apply')}</span>
            <Select
              value={menu.apply ?? def?.apply ?? 'release'}
              label={t('radial.apply')}
              {disabled}
              class="wide-select"
              options={APPLIES.map((a) => ({ value: a, label: t(`radial.applies.${a}`) }))}
              onchange={(apply) => setMenu({ apply })}
            />
          </div>
          {#if (menu.apply ?? def?.apply) === 'command' || (menu.select ?? def?.select) === 'commands'}
            <p class="hint">{t('radial.commands_hint')}</p>
          {/if}
        </section>

        <section>
          <div class="section-title">{t('radial.position')}</div>
          <PositionPicker
            position={menu.position ?? def?.position ?? 'center'}
            offset={menu.offset ?? [0, 0]}
            {disabled}
            onchange={(patch) =>
              setMenu({
                ...(patch.position ? { position: patch.position === 'center' ? undefined : patch.position } : {}),
                ...(patch.offset ? { offset: patch.offset[0] || patch.offset[1] ? patch.offset : undefined } : {}),
              })}
          />
        </section>

        <section>
          <div class="section-title">{t('radial.look')}</div>
          <div class="fields">
            <Slider
              label={t('radial.scale')}
              value={menu.scale ?? 1}
              def={def?.scale}
              min={0.5}
              max={2}
              step={0.1}
              scale={100}
              unit="%"
              hint={t('radial.scale_hint', { px: Math.round(RING * 2 * (menu.scale ?? 1)) })}
              {disabled}
              onchange={(v) => setMenu({ scale: v === 1 ? undefined : v })}
            />
            <Slider
              label={t('radial.tint')}
              value={menu.tint ?? 0}
              def={def?.tint}
              min={0}
              max={0.9}
              step={0.05}
              scale={100}
              unit="%"
              {disabled}
              onchange={(v) => setMenu({ tint: v || undefined })}
            />
            <Slider
              label={t('radial.delay')}
              value={menu.delay_ms ?? 0}
              def={def?.delay_ms}
              min={0}
              max={1000}
              step={50}
              unit={t('common.ms')}
              hint={t('radial.delay_hint')}
              {disabled}
              onchange={(v) => setMenu({ delay_ms: v || undefined })}
            />
          </div>
        </section>

        {#if !disabled}
          <button class="btn danger" onclick={remove}><Trash2 size={14} /> {t('radial.remove')}</button>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .radial {
    container-type: inline-size;
    max-width: 980px;
    margin: 0 auto;
    padding: 28px 28px 40px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .top h2 {
    margin: 0 0 4px;
    font-size: 18px;
    font-weight: 600;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 36px 24px;
    text-align: center;
    color: var(--text-faint);
  }
  .empty strong {
    color: var(--text);
    font-size: 15px;
  }
  .empty .hint {
    max-width: 460px;
  }
  .empty .btn {
    margin-top: 6px;
  }
  .tabs,
  .tab-list {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--line);
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
  .tab:hover {
    border-color: var(--line-strong);
    color: var(--text);
  }
  .tab.on {
    border-color: var(--accent);
    color: var(--text);
    background: var(--accent-soft);
  }
  .tab.add {
    border-style: dashed;
    color: var(--accent);
  }
  .sub {
    font-size: var(--fs-xs);
    color: var(--text-faint);
  }
  .editor {
    display: grid;
    grid-template-columns: minmax(240px, 340px) minmax(0, 1fr);
    gap: 28px;
    align-items: start;
  }
  .ring-col {
    position: sticky;
    top: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }
  .center {
    text-align: center;
  }
  .settings {
    display: flex;
    flex-direction: column;
    gap: 18px;
    min-width: 0;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
  }
  .field {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .field input {
    flex: 1;
    max-width: 280px;
  }
  .label {
    color: var(--text-dim);
    min-width: 110px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .row :global(.wide-select) {
    min-width: 220px;
  }
  .openers {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
  }
  .own {
    align-self: flex-start;
  }
  .dim {
    color: var(--text-faint);
    font-size: var(--fs-s);
  }
  .capturing {
    color: var(--accent);
    font-size: var(--fs-s);
  }
  .warn {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--warning);
    font-size: var(--fs-s);
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 14px 24px;
  }
  .danger {
    align-self: flex-start;
    color: var(--danger);
  }
  @container (max-width: 760px) {
    .editor {
      grid-template-columns: minmax(0, 1fr);
    }
    .ring-col {
      position: static;
    }
    .ring-col :global(.ring) {
      max-width: 280px;
    }
  }
</style>
