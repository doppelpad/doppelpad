<script lang="ts">
  // Shortcuts: mappings whose trigger is 2–4 controls pressed together. New ones are
  // picked by clicking controls or pressing them on the controller; the binding panel
  // then edits the selected shortcut like any other control.
  import { Plus, Trash2, Check, X } from '@lucide/svelte';
  import { onDestroy } from 'svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { CARDINALS, SIDES, SIDE_NAMES, ZONES, controlLabel, withZone } from '../../lib/model/controls';
  import { comboLabel, mappingSummary } from '../../lib/model/labels';
  import { comboKey, comboList, sameCombo, shortcuts, toCombo } from '../../lib/model/profile';
  import { app } from '../../lib/state/app.svelte';
  import type { Combo, ControlName, Mapping } from '../../lib/types';

  const MAX = 4;
  const STICKS = SIDE_NAMES.flatMap((s) => CARDINALS.map((d) => SIDES[s].stick + d));

  let picking = $state<ControlName[] | null>(null);

  const list = $derived(app.profile ? shortcuts(app.profile, app.layer) : []);
  /** One row per trigger (several activators of one shortcut share a row). */
  const rows = $derived.by(() => {
    const byKey = new Map<string, Mapping[]>();
    for (const m of list) byKey.set(comboKey(m.trigger), [...(byKey.get(comboKey(m.trigger)) ?? []), m]);
    return [...byKey.values()];
  });

  const TRIGGERS = ['LT', 'RT'].flatMap((tr) => [tr, ...ZONES.map((z) => withZone(tr, z))]);

  const controls = $derived([...((app.catalog?.pad_buttons ?? []) as string[]), ...STICKS, ...TRIGGERS]);

  function toggle(c: ControlName) {
    if (!picking) return;
    if (picking.includes(c)) picking = picking.filter((x) => x !== c);
    else if (picking.length < MAX) picking = [...picking, c];
  }

  let releasePresses = () => {};

  function start() {
    picking = [];
    releasePresses = app.capturePresses((c) => toggle(c));
  }

  function stop() {
    picking = null;
    releasePresses();
  }

  function confirmShortcut() {
    if (!picking || picking.length < 2) return;
    const combo = toCombo(picking);
    stop();
    app.select(combo);
  }

  function remove(trigger: Combo) {
    app.edit((p) => ({
      ...p,
      mappings: p.mappings.filter((m) => !((m.layer ?? 0) === app.layer && sameCombo(m.trigger, trigger))),
    }));
    if (app.selected && sameCombo(app.selected, trigger)) app.select(null);
  }

  const label = (c: Combo) => comboLabel(c, app.family);

  onDestroy(stop);
</script>

<div class="shortcuts">
  <div class="head">
    <h2>{t('shortcuts.title', { layer: app.layerLabel(app.layer) })}</h2>
    {#if !picking && !app.readOnly}
      <button class="btn primary" onclick={start}><Plus size={14} /> {t('shortcuts.add')}</button>
    {/if}
  </div>

  {#if picking}
    <div class="picker">
      <p class="hint">{t('shortcuts.hint')}</p>
      <div class="grid">
        {#each controls as c (c)}
          <button
            class="ctl"
            class:on={picking.includes(c)}
            disabled={!picking.includes(c) && picking.length >= MAX}
            onclick={() => toggle(c)}
          >
            {controlLabel(c, app.family)}
          </button>
        {/each}
      </div>
      <div class="bar">
        <span class="sel">
          {picking.length ? t('shortcuts.selected', { list: label(picking) }) : ''}
        </span>
        <button class="btn" onclick={stop}><X size={14} /> {t('common.cancel')}</button>
        <button class="btn primary" disabled={picking.length < 2} onclick={confirmShortcut}><Check size={14} /> {t('shortcuts.confirm')}</button>
      </div>
    </div>
  {/if}

  {#if rows.length === 0 && !picking}
    <p class="empty">{t('shortcuts.empty')}</p>
  {/if}
  <div class="rows">
    {#each rows as group (comboKey(group[0].trigger))}
      {@const trig = group[0].trigger}
      <div class="row" class:selected={!!app.selected && sameCombo(app.selected, trig)}>
        <button class="main" onclick={() => app.select(trig)}>
          <span class="combo">
            {#each comboList(trig) as c, i (c)}
              {#if i > 0}<span class="plus">+</span>{/if}
              <span class="key">{controlLabel(c, app.family)}</span>
            {/each}
          </span>
          <span class="sum">{group.map((m) => mappingSummary(m, app.layerLabel)).join(' · ')}</span>
        </button>
        {#if !app.readOnly}
          <button class="icon-btn" title={t('common.delete')} onclick={() => remove(trig)}><Trash2 size={15} /></button>
        {/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .shortcuts {
    max-width: 820px;
    margin: 0 auto;
    padding: 22px 24px 40px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }
  .picker {
    border: 1px solid var(--accent);
    border-radius: var(--radius-m);
    background: var(--bg-1);
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .hint {
    margin: 0;
    color: var(--text-dim);
  }
  .grid {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .ctl {
    padding: 5px 10px;
    border-radius: 99px;
    border: 1px solid var(--line);
    background: var(--bg-2);
    font-size: 12px;
  }
  .ctl.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .ctl:disabled {
    opacity: 0.35;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .sel {
    flex: 1;
    color: var(--text-dim);
  }
  .empty {
    color: var(--text-faint);
  }
  .rows {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    align-items: center;
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    background: var(--bg-1);
    padding-right: 6px;
  }
  .row.selected {
    border-color: var(--accent);
  }
  .main {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 10px 12px;
    text-align: left;
    min-width: 0;
  }
  .combo {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
  }
  .key {
    padding: 3px 9px;
    border-radius: var(--radius-s);
    background: var(--bg-3);
    border: 1px solid var(--line-strong);
    font-weight: 600;
    font-size: 12px;
  }
  .plus {
    color: var(--text-faint);
  }
  .sum {
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
