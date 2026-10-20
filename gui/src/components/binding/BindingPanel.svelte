<script lang="ts">
  // Right-hand editor for the selected control: activator, actions, mode, block, layer
  // jump, description and timing overrides.
  import { Plus, X, Trash2, Info, ChevronRight, Zap, Repeat, Ban, Clapperboard, SlidersHorizontal } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { ZONES, controlLabel, isAnyDirection, withZone, zoneParts, type ZoneName } from '../../lib/model/controls';
  import {
    ACTIVATORS,
    activatorOf,
    actionLabel,
    addLayer,
    comboList,
    effectiveMappings,
    layerIds,
    layerOf,
    mappingSummary,
    modeOf,
    nativeAction,
    removeMapping,
    sameAction,
  } from '../../lib/model/profile';
  import { app, type AnalogTab } from '../../lib/state/app.svelte';
  import type { Action, Activator, Combo, JumpMode, Mapping, Mode } from '../../lib/types';
  import Toggle from '../ui/Toggle.svelte';
  import ActionPicker from './ActionPicker.svelte';
  import ActivatorIcon from './ActivatorIcon.svelte';
  import ZoneEditor from './ZoneEditor.svelte';

  let picking = $state(false);
  let showTiming = $state(false);

  const combo = $derived(app.selected);
  const profile = $derived(app.profile);
  const readOnly = $derived(app.readOnly);

  const at = $derived<{ own?: Mapping; inherited?: Mapping }>(combo ? app.mappingAt(app.layer, combo, app.activator) : {});
  const own = $derived(at.own);
  const inheritedFrom = $derived(at.inherited);
  const effective = $derived(
    profile && combo ? effectiveMappings(profile, app.layer, combo) : { mappings: [], inherited: false },
  );
  /** What the panel shows: this layer's mapping, else the inherited one. */
  const shown = $derived<Mapping | undefined>(own ?? inheritedFrom);
  const mode = $derived<Mode>(shown ? modeOf(shown) : 'hold');
  const blocked = $derived(!!shown && (shown.actions ?? []).length === 0 && !shown.jump);
  const isMacro = $derived(mode === 'once' || mode === 'repeat' || mode === 'while_held');
  /** The control's own output, which turbo/toggle fall back to when nothing is assigned. */
  const native = $derived(combo ? nativeAction(combo) : null);
  /** Turbo/toggle can always be switched off; on needs outputs: assigned ones or the control's own. */
  const canEnableMode = $derived(!blocked && ((shown?.actions ?? []).length > 0 || !!native));

  /** Activators that have a mapping (own or inherited) for the selected control. */
  const usedActivators = $derived(new Set(effective.mappings.map(activatorOf)));

  const title = $derived(combo ? comboList(combo).map((c) => controlLabel(c, app.family)).join(' + ') : '');

  /** The analog settings page of a selected stick or trigger control (including its zones). */
  const analogTab = $derived.by<AnalogTab | null>(() => {
    const list = combo ? comboList(combo) : [];
    if (list.length !== 1) return null;
    const c = list[0];
    if (c === 'LS' || c.startsWith('LeftStick')) return 'left';
    if (c === 'RS' || c.startsWith('RightStick')) return 'right';
    if (/^(LT|RT)(Full|Low|Med|High)?$/.test(c)) return 'triggers';
    return null;
  });

  /** A stick direction, trigger or one of their zones: the panel offers its zones. */
  const zone = $derived.by(() => {
    const list = combo ? comboList(combo) : [];
    return list.length === 1 ? zoneParts(list[0]) : null;
  });
  /** Zones (and the whole range, `null`) that have a mapping on this layer. */
  const usedZones = $derived.by(() => {
    const used = new Set<ZoneName | null>();
    if (!profile || !zone) return used;
    for (const z of [null, ...ZONES]) {
      if (effectiveMappings(profile, app.layer, withZone(zone.base, z)).mappings.length > 0) used.add(z);
    }
    return used;
  });

  const defaults = $derived(app.catalog?.defaults.timing);
  const timing = $derived({ ...defaults, ...(profile?.timing ?? {}) });

  const otherLayers = $derived(profile ? layerIds(profile).filter((id) => id !== app.layer) : []);

  /** Mappings defined on the current layer, for the overview when nothing is selected. */
  const layerMappings = $derived(profile ? profile.mappings.filter((m) => layerOf(m) === app.layer) : []);

  /** Apply a change to the selected mapping (see `app.editMapping`). */
  function edit(change: (m: Mapping) => void, keepEmpty = false) {
    if (combo) app.editMapping(app.layer, combo, app.activator, change, keepEmpty);
  }

  function addAction(a: Action) {
    edit((m) => (m.actions = [...(m.actions ?? []), a]));
  }

  function removeAction(i: number) {
    edit((m) => (m.actions = (m.actions ?? []).filter((_, j) => j !== i)));
  }

  /**
   * Turbo and toggle act on the outputs, so a control without any starts from its own output
   * (turbo on A pulses A). Back in hold mode a mapping that is just that output goes away.
   */
  function setMode(next: Mode) {
    edit((m) => {
      m.mode = next;
      const acts = m.actions ?? [];
      if (next !== 'hold' && acts.length === 0 && native) m.actions = [{ ...native }];
      const onlyNative = acts.length === 1 && !!native && sameAction(acts[0], native) && !m.jump && !m.description;
      if (next === 'hold' && onlyNative) m.actions = [];
    });
  }

  function setBlocked(on: boolean) {
    if (on) {
      edit((m) => {
        m.actions = [];
        m.jump = undefined;
        m.mode = 'hold';
      }, true);
    } else if (profile && combo) {
      app.update(removeMapping(profile, app.layer, combo, app.activator));
    }
  }

  function setJumpLayer(value: string) {
    if (value === '') {
      edit((m) => (m.jump = undefined));
      return;
    }
    if (value === 'new' && profile) {
      const { profile: p, id } = addLayer(profile);
      app.update(p);
      edit((m) => (m.jump = { layer: id }));
      return;
    }
    const layer = Number(value);
    edit((m) => (m.jump = { ...(m.jump ?? {}), layer }));
  }

  function setJumpMode(mode: JumpMode) {
    edit((m) => m.jump && (m.jump = { ...m.jump, mode }));
  }

  function num(v: string): number | undefined {
    const n = parseInt(v, 10);
    return Number.isFinite(n) && n >= 0 ? n : undefined;
  }

  type TimingField = 'turbo_ms' | 'long_press_ms' | 'double_tap_ms' | 'triple_tap_ms' | 'key_delay_ms';
  function setTiming(field: TimingField, v: string) {
    edit((m) => (m[field] = num(v)));
  }

  const timingFields = $derived<TimingField[]>(
    [
      app.activator === 'long' ? 'long_press_ms' : null,
      app.activator === 'double' ? 'double_tap_ms' : null,
      app.activator === 'triple' ? 'triple_tap_ms' : null,
      'key_delay_ms',
    ].filter(Boolean) as TimingField[],
  );

  function selectMapping(m: Mapping) {
    app.select(m.trigger as Combo);
    app.activator = activatorOf(m);
  }

  function openMacro() {
    if (!combo) return;
    app.macroTarget = { layer: app.layer, trigger: combo, activator: app.activator };
    app.view = 'macro';
  }

  // Close the picker when the selection changes.
  $effect(() => {
    void app.selectedKey;
    void app.activator;
    picking = false;
  });
</script>

<div class="panel">
  {#if !combo}
    <div class="intro">
      <Info size={16} />
      <p>{t('binding.choose')}</p>
    </div>
    <div class="section-title">{t('binding.layer_mappings', { layer: app.layerLabel(app.layer) })}</div>
    {#if layerMappings.length === 0}
      <p class="muted">{t('binding.no_layer_mappings')}</p>
    {/if}
    <div class="overview">
      {#each layerMappings as m, i (i)}
        <button class="ov-row" onclick={() => selectMapping(m)}>
          <span class="ov-act"><ActivatorIcon activator={activatorOf(m)} size={14} /></span>
          <span class="ov-name">{comboList(m.trigger).map((c) => controlLabel(c, app.family)).join(' + ')}</span>
          <span class="ov-sum">{mappingSummary(m, app.layerLabel)}</span>
          <ChevronRight size={14} />
        </button>
      {/each}
    </div>
  {:else}
    <div class="head">
      <div class="title">
        <span class="name">{title}</span>
        <span class="layer" style="--c: var(--layer-{Math.min(app.layer, 6)})">{app.layerLabel(app.layer)}</span>
      </div>
      <button class="icon-btn" title={t('common.close')} onclick={() => app.select(null)}><X size={16} /></button>
    </div>

    {#if analogTab}
      {@const stick = analogTab !== 'triggers'}
      <button class="analog-link" onclick={() => app.openAnalog(analogTab)}>
        <SlidersHorizontal size={14} />
        <span class="al-text">
          <span>{t(stick ? 'binding.stick_settings' : 'binding.trigger_settings')}</span>
          <span class="al-hint">{t(stick ? 'binding.stick_settings_hint' : 'binding.trigger_settings_hint')}</span>
        </span>
        <ChevronRight size={14} />
      </button>
    {/if}

    {#if zone}
      {@const zp = zone}
      <div class="zones" role="tablist" aria-label={t('zones.title')}>
        {#each [null, ...ZONES] as z (z ?? 'whole')}
          {#if z !== null || !isAnyDirection(zp.base)}
            <button
              role="tab"
              aria-selected={zp.zone === z}
              class:active={zp.zone === z}
              class:used={usedZones.has(z)}
              title={t(`zones.desc.${z ? z.toLowerCase() : 'whole'}`)}
              onclick={() => app.select(withZone(zp.base, z))}
            >
              {t(z ? `zones.${z.toLowerCase()}` : 'zones.whole')}
              <span class="dot"></span>
            </button>
          {/if}
        {/each}
      </div>
      <div class="act-desc">{t(`zones.desc.${zp.zone ? zp.zone.toLowerCase() : 'whole'}`)}</div>
      {#if zp.zone}
        <ZoneEditor parts={zp} />
      {/if}
    {/if}

    <div class="activators" role="tablist">
      {#each ACTIVATORS as a (a)}
        <button
          role="tab"
          aria-selected={app.activator === a}
          class="act"
          class:active={app.activator === a}
          class:used={usedActivators.has(a)}
          title={t(`activators.${a}`)}
          onclick={() => (app.activator = a as Activator)}
        >
          <ActivatorIcon activator={a} />
          <span class="dot"></span>
        </button>
      {/each}
    </div>
    <div class="act-desc">
      <strong>{t(`activators.${app.activator}`)}</strong> — {t(`activators.desc.${app.activator}`)}
    </div>

    {#if inheritedFrom}
      <div class="note"><Info size={14} /> {t('binding.inherited')}</div>
    {/if}

    <div class="section-title">{t('binding.actions')}</div>
    {#if blocked}
      <div class="blocked"><Ban size={14} /> {t('binding.unmapped')}</div>
    {:else}
      <div class="chips">
        {#each shown?.actions ?? [] as a, i (i)}
          <span class="chip">
            {actionLabel(a)}
            {#if !readOnly}
              <button class="chip-x" title={t('common.remove')} onclick={() => removeAction(i)}><X size={11} /></button>
            {/if}
          </span>
        {/each}
        {#if !readOnly}
          <button class="chip add" class:on={picking} onclick={() => (picking = !picking)}>
            <Plus size={13} /> {t('binding.add_action')}
          </button>
        {/if}
      </div>
      {#if (shown?.actions ?? []).length === 0 && !picking}
        <p class="muted">{t('binding.no_actions')}</p>
      {/if}
      {#if picking}
        <ActionPicker onpick={addAction} onclose={() => (picking = false)} />
      {/if}
    {/if}

    <div class="modes">
      <div class="mode-row">
        <Toggle
          checked={mode === 'turbo'}
          disabled={readOnly || (mode !== 'turbo' && !canEnableMode)}
          label={t('binding.turbo')}
          onchange={(v) => setMode(v ? 'turbo' : 'hold')}
        />
        <Zap size={14} />
        {#if mode === 'turbo'}
          <label class="inline">
            {t('binding.period')}
            <input
              type="number"
              min="10"
              step="10"
              value={shown?.turbo_ms ?? ''}
              placeholder={String(timing.turbo_ms ?? '')}
              onchange={(e) => setTiming('turbo_ms', e.currentTarget.value)}
            />
            {t('common.ms')}
          </label>
        {/if}
      </div>
      <div class="mode-row">
        <Toggle
          checked={mode === 'toggle'}
          disabled={readOnly || (mode !== 'toggle' && !canEnableMode)}
          label={t('binding.toggle')}
          onchange={(v) => setMode(v ? 'toggle' : 'hold')}
        />
        <Repeat size={14} />
      </div>
      <div class="mode-row">
        <Toggle checked={blocked} disabled={readOnly} label={t('binding.unmap')} onchange={setBlocked} />
      </div>
      <button class="btn macro" class:on={isMacro} onclick={openMacro} disabled={readOnly && !isMacro}>
        <Clapperboard size={14} />
        {t('binding.macro')}
        {#if isMacro}<span class="tag">{t(`macro.mode_${mode === 'while_held' ? 'while' : mode}`)}</span>{/if}
      </button>
    </div>

    <div class="section-title">{t('binding.jump')}</div>
    <div class="jump">
      <select
        value={shown?.jump ? String(shown.jump.layer) : ''}
        disabled={readOnly}
        onchange={(e) => setJumpLayer(e.currentTarget.value)}
      >
        <option value="">{t('binding.jump_none')}</option>
        {#each otherLayers as id (id)}
          <option value={String(id)}>{t('binding.jump_to', { layer: app.layerLabel(id) })}</option>
        {/each}
        <option value="new">+ {t('layers.add')}</option>
      </select>
      {#if shown?.jump}
        <div class="segmented">
          {#each ['hold', 'toggle', 'switch'] as const as jm (jm)}
            <button
              class:active={(shown.jump.mode ?? 'hold') === jm}
              disabled={readOnly}
              onclick={() => setJumpMode(jm)}
            >
              {t(`binding.jump_${jm}`)}
            </button>
          {/each}
        </div>
        {#if shown.jump.mode === 'switch'}
          <label class="inline">
            {t('binding.jump_delay')}
            <input
              type="number"
              min="0"
              step="50"
              value={shown.jump.delay_ms ?? ''}
              placeholder="0"
              disabled={readOnly}
              onchange={(e) => edit((m) => m.jump && (m.jump = { ...m.jump, delay_ms: num(e.currentTarget.value) }))}
            />
            {t('common.ms')}
          </label>
        {/if}
      {/if}
    </div>

    <div class="section-title">{t('binding.description')}</div>
    <textarea
      rows="2"
      placeholder={t('binding.description_placeholder')}
      value={shown?.description ?? ''}
      disabled={readOnly}
      onchange={(e) => edit((m) => (m.description = e.currentTarget.value.trim() || undefined))}
    ></textarea>

    <button class="disclosure" onclick={() => (showTiming = !showTiming)}>
      <ChevronRight size={14} class={showTiming ? 'rot' : ''} />
      {t('binding.advanced')}
    </button>
    {#if showTiming}
      <div class="timing">
        {#each timingFields as f (f)}
          <label class="inline">
            <span class="tl">{t(`binding.timing.${f}`)}</span>
            <input
              type="number"
              min="0"
              step="10"
              value={shown?.[f] ?? ''}
              placeholder={String(timing[f] ?? '')}
              disabled={readOnly}
              onchange={(e) => setTiming(f, e.currentTarget.value)}
            />
            {t('common.ms')}
          </label>
        {/each}
      </div>
    {/if}

    {#if own && !readOnly}
      <button class="btn danger" onclick={() => profile && combo && app.update(removeMapping(profile, app.layer, combo, app.activator))}>
        <Trash2 size={14} /> {t('binding.delete')}
      </button>
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
  .muted {
    color: var(--text-faint);
    margin: 0;
  }
  .overview {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ov-row {
    display: grid;
    grid-template-columns: 16px minmax(0, 1fr) minmax(0, 1.2fr) 14px;
    align-items: center;
    gap: 8px;
    padding: 7px 8px;
    border-radius: var(--radius-s);
    text-align: left;
    color: var(--text-dim);
  }
  .ov-row:hover {
    background: var(--bg-2);
    color: var(--text);
  }
  .ov-act {
    color: var(--accent);
    display: inline-flex;
  }
  .ov-name,
  .ov-sum {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ov-name {
    color: var(--text);
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
    gap: 4px;
    min-width: 0;
  }
  .name {
    font-size: 16px;
    font-weight: 600;
  }
  .layer {
    align-self: flex-start;
    font-size: 11px;
    padding: 1px 8px;
    border-radius: 99px;
    border: 1px solid var(--c);
    color: var(--c);
  }
  .analog-link {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-radius: var(--radius-s);
    border: 1px solid var(--line);
    background: var(--bg-2);
    color: var(--text-dim);
    text-align: left;
  }
  .analog-link:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .al-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .al-text > span:first-child {
    color: var(--text);
    font-weight: 500;
  }
  .al-hint {
    font-size: 11.5px;
    color: var(--text-faint);
  }
  .zones {
    display: grid;
    grid-auto-columns: 1fr;
    grid-auto-flow: column;
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
    overflow: hidden;
  }
  .zones button {
    position: relative;
    padding: 6px 4px 8px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .zones button + button {
    border-left: 1px solid var(--line);
  }
  .zones button:hover {
    color: var(--text);
    background: var(--bg-2);
  }
  .zones button.active {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .zones .dot {
    position: absolute;
    left: 50%;
    bottom: 3px;
    width: 4px;
    height: 4px;
    margin-left: -2px;
    border-radius: 50%;
  }
  .zones .used .dot {
    background: var(--accent);
  }
  .activators {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 4px;
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    padding: 4px;
  }
  .act {
    position: relative;
    display: grid;
    place-items: center;
    height: 38px;
    border-radius: var(--radius-s);
    color: var(--text-faint);
  }
  .act:hover {
    color: var(--text);
    background: var(--bg-2);
  }
  .act.used {
    color: var(--text-dim);
  }
  .act.active {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .act .dot {
    position: absolute;
    bottom: 4px;
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: transparent;
  }
  .act.used .dot {
    background: var(--accent);
  }
  .act-desc {
    color: var(--text-dim);
    font-size: 12px;
  }
  .act-desc strong {
    color: var(--text);
    font-weight: 600;
  }
  .note {
    display: flex;
    gap: 6px;
    align-items: center;
    font-size: 12px;
    color: var(--layer-2);
    background: color-mix(in srgb, var(--layer-2) 10%, transparent);
    border-radius: var(--radius-s);
    padding: 6px 8px;
  }
  .section-title {
    margin-top: 6px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-dim);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 5px 8px 5px 10px;
    border-radius: 99px;
    background: var(--bg-3);
    border: 1px solid var(--line-strong);
    font-weight: 500;
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
    color: white;
  }
  .chip.add {
    background: none;
    border-style: dashed;
    color: var(--accent);
    padding-right: 10px;
  }
  .chip.add.on {
    border-style: solid;
    background: var(--accent-soft);
  }
  .blocked {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--warning);
  }
  .modes {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 0 2px;
    border-top: 1px solid var(--line);
    margin-top: 4px;
  }
  .mode-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 26px;
  }
  .mode-row > :global(svg) {
    color: var(--text-faint);
  }
  .inline {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-dim);
    font-size: 12px;
    margin-left: auto;
  }
  .inline input {
    width: 72px;
    padding: 4px 6px;
  }
  .macro {
    align-self: flex-start;
  }
  .macro.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .tag {
    font-size: 11px;
    background: var(--accent-soft);
    border-radius: 99px;
    padding: 0 6px;
  }
  .jump {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .jump .inline {
    margin-left: 0;
  }
  .segmented {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
    overflow: hidden;
  }
  .segmented button {
    padding: 6px 4px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .segmented button + button {
    border-left: 1px solid var(--line);
  }
  .segmented button.active {
    background: var(--accent-soft);
    color: var(--accent);
  }
  textarea {
    resize: vertical;
    min-height: 44px;
  }
  .disclosure {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text-dim);
    align-self: flex-start;
    font-size: 12px;
  }
  .disclosure :global(.rot) {
    transform: rotate(90deg);
  }
  .timing {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .timing .inline {
    margin-left: 0;
  }
  .tl {
    flex: 1;
    min-width: 150px;
  }
  .danger {
    align-self: flex-start;
    color: var(--danger);
    margin-top: 6px;
  }
</style>
