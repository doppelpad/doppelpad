<script lang="ts">
  // Right-hand editor for the selected control: activator, actions, mode, block, layer
  // jump, description and timing overrides.
  import {
    Plus,
    Trash2,
    Info,
    ChevronRight,
    Zap,
    Repeat,
    Ban,
    Clapperboard,
    SlidersHorizontal,
  } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import {
    ZONES,
    analogTabOf,
    fullPullBase,
    isAnyDirection,
    withZone,
    zoneParts,
    type ZoneName,
  } from '../../lib/model/controls';
  import {
    ACTIVATORS,
    activatorOf,
    addLayer,
    applyMode,
    effectiveMappings,
    isBlocked,
    isMacroMode,
    layerIds,
    layerOf,
    modeOf,
    nativeAction,
    removeMapping,
    singleControl,
  } from '../../lib/model/profile';
  import { hasParameters } from '../../lib/model/actions';
  import { parseNonNegInt } from '../../lib/model/numbers';
  import { actionLabel, comboLabel, mappingSummary } from '../../lib/model/labels';
  import { app, type AnalogTab } from '../../lib/state/app.svelte';
  import type { Action, Activator, Combo, JumpMode, Mapping, Mode } from '../../lib/types';
  import Segmented, { type SegmentedOption } from '../ui/Segmented.svelte';
  import Chip from '../ui/Chip.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import ActionParams from './ActionParams.svelte';
  import ActionPicker from './ActionPicker.svelte';
  import ActivatorIcon from './ActivatorIcon.svelte';
  import MsInput from './MsInput.svelte';
  import PanelHeader from './PanelHeader.svelte';
  import ZoneEditor from './ZoneEditor.svelte';

  let picking = $state(false);
  let showTiming = $state(false);
  /** Action whose parameters are shown below the chips. */
  let editingIdx = $state<number | null>(null);

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
  const blocked = $derived(isBlocked(shown));
  const isMacro = $derived(isMacroMode(mode));
  /** The control's own output, which turbo/toggle fall back to when nothing is assigned. */
  const native = $derived(combo ? nativeAction(combo) : null);
  /** Turbo/toggle can always be switched off; on needs outputs: assigned ones or the control's own. */
  const canEnableMode = $derived(!blocked && ((shown?.actions ?? []).length > 0 || !!native));

  /** Activators that have a mapping (own or inherited) for the selected control. */
  const usedActivators = $derived(new Set(effective.mappings.map(activatorOf)));

  const title = $derived(combo ? comboLabel(combo, app.family) : '');

  /** The selected control, when it is a single one. */
  const single = $derived(combo ? singleControl(combo) : null);
  /** The analog settings page of a selected stick or trigger control (including its zones). */
  const analogTab = $derived<AnalogTab | null>(single ? analogTabOf(single) : null);
  /** The trigger (`LT`/`RT`) whose full pull is selected, which sits next to its zones. */
  const fullPull = $derived(single ? fullPullBase(single) : null);
  /** A stick direction, trigger or one of their zones: the panel offers its zones. */
  const zone = $derived(single ? zoneParts(fullPull ?? single) : null);
  /** Zones (and the whole range, `null`) that have a mapping on this layer. */
  const usedZones = $derived.by(() => {
    const used = new Set<ZoneName | null>();
    if (!profile || !zone) return used;
    for (const z of [null, ...ZONES]) {
      if (effectiveMappings(profile, app.layer, withZone(zone.base, z)).mappings.length > 0) used.add(z);
    }
    return used;
  });
  const usedFull = $derived(
    !!profile && !!zone && zone.source.kind === 'trigger' &&
      effectiveMappings(profile, app.layer, `${zone.base}Full`).mappings.length > 0,
  );
  /** The zone tabs: the whole range (not for "any direction"), the zones and a trigger's full pull. */
  const zoneOptions = $derived.by((): SegmentedOption<ZoneName | null | 'full'>[] => {
    if (!zone) return [];
    const zones = [
      ...(isAnyDirection(zone.base) ? [] : [null]),
      ...ZONES,
      ...(zone.source.kind === 'trigger' ? (['full'] as const) : []),
    ];
    return zones.map((z) => {
      const key = z === null ? 'whole' : z.toLowerCase();
      return { value: z, label: t(`zones.${key}`), title: t(`zones.desc.${key}`), marked: z === 'full' ? usedFull : usedZones.has(z) };
    });
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

  function setMode(next: Mode) {
    edit((m) => applyMode(m, next, native));
  }

  function setBlocked(on: boolean) {
    if (on) {
      edit((m) => {
        m.actions = [];
        m.jump = undefined;
        m.mode = 'hold';
      }, true);
    } else if (combo) {
      app.edit((p) => removeMapping(p, app.layer, combo, app.activator));
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

  const num = (v: string) => parseNonNegInt(v) ?? undefined;

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
    editingIdx = null;
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
          <span class="ov-name">{comboLabel(m.trigger, app.family)}</span>
          <span class="ov-sum">{mappingSummary(m, app.layerLabel)}</span>
          <ChevronRight size={14} />
        </button>
      {/each}
    </div>
  {:else}
    <PanelHeader {title} layer={app.layer} onclose={() => app.select(null)} />

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
      <Segmented
        variant="fill"
        label={t('zones.title')}
        options={zoneOptions}
        value={fullPull ? 'full' : zp.zone}
        onchange={(z) => app.select(z === 'full' ? `${zp.base}Full` : withZone(zp.base, z))}
      />
      <div class="act-desc">
        {t(`zones.desc.${fullPull ? 'full' : zp.zone ? zp.zone.toLowerCase() : 'whole'}`)}
      </div>
      {#if zp.zone && zp.source.kind === 'gyro'}
        <button class="btn gyro-zones" onclick={() => app.openAnalog('gyro')}>{t('zones.gyro_settings')}</button>
      {:else if zp.zone && zp.source.kind !== 'gyro'}
        <ZoneEditor parts={{ ...zp, source: zp.source }} />
      {/if}
    {/if}

    <div class="activators" role="radiogroup" aria-label={t('activators.title')}>
      {#each ACTIVATORS as a (a)}
        <button
          role="radio"
          aria-checked={app.activator === a}
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
          <Chip active={editingIdx === i} onremove={readOnly ? undefined : () => removeAction(i)}>
            {#if hasParameters(a)}
              <button class="chip-label" title={t('binding.edit_params')} onclick={() => (editingIdx = editingIdx === i ? null : i)}>
                {actionLabel(a)}
              </button>
            {:else}
              {actionLabel(a)}
            {/if}
          </Chip>
        {/each}
        {#if !readOnly}
          <button class="add" class:on={picking} onclick={() => (picking = !picking)}>
            <Plus size={13} /> {t('binding.add_action')}
          </button>
        {/if}
      </div>
      {#if (shown?.actions ?? []).length === 0 && !picking}
        <p class="muted">{t('binding.no_actions')}</p>
      {/if}
      {#if editingIdx !== null && shown?.actions?.[editingIdx]}
        {@const idx = editingIdx}
        <div class="params-box">
          <ActionParams
            action={shown.actions[idx]}
            disabled={readOnly}
            onchange={(a) => edit((m) => (m.actions = (m.actions ?? []).map((x, j) => (j === idx ? a : x))))}
          />
        </div>
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
            <MsInput
              value={shown?.turbo_ms}
              placeholder={String(timing.turbo_ms ?? '')}
              min={10}
              onchange={(v) => setTiming('turbo_ms', v)}
            />
          </label>
          <label class="inline" title={t('binding.random_hint')}>
            {t('binding.random')}
            <MsInput
              value={shown?.random_ms}
              placeholder="0"
              max={10000}
              onchange={(v) => edit((m) => (m.random_ms = num(v) || undefined))}
            />
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
        <Segmented
          variant="fill"
          hover={false}
          label={t('binding.jump')}
          options={(['hold', 'toggle', 'switch'] as const).map((jm) => ({ value: jm, label: t(`binding.jump_${jm}`) }))}
          value={shown.jump.mode ?? 'hold'}
          disabled={readOnly}
          onchange={setJumpMode}
        />
        {#if shown.jump.mode === 'switch' || shown.jump.delay_ms}
          <label class="inline">
            {t('binding.jump_delay')}
            <MsInput
              value={shown.jump.delay_ms}
              placeholder="0"
              step={50}
              disabled={readOnly}
              onchange={(v) => edit((m) => m.jump && (m.jump = { ...m.jump, delay_ms: num(v) }))}
            />
          </label>
          {#if shown.jump.delay_ms}
            <label class="inline" title={t('binding.jump_postpone_hint')}>
              <input
                type="checkbox"
                checked={shown.jump.postpone ?? false}
                disabled={readOnly}
                onchange={(e) => edit((m) => m.jump && (m.jump = { ...m.jump, postpone: e.currentTarget.checked || undefined }))}
              />
              {t('binding.jump_postpone')}
            </label>
          {/if}
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
            <MsInput
              value={shown?.[f]}
              placeholder={String(timing[f] ?? '')}
              disabled={readOnly}
              onchange={(v) => setTiming(f, v)}
            />
          </label>
        {/each}
      </div>
    {/if}

    {#if own && !readOnly}
      <button class="btn danger" onclick={() => combo && app.edit((p) => removeMapping(p, app.layer, combo, app.activator))}>
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
    font-size: var(--fs-s);
  }
  .act-desc strong {
    color: var(--text);
    font-weight: 600;
  }
  .note {
    display: flex;
    gap: 6px;
    align-items: center;
    font-size: var(--fs-s);
    color: var(--layer-2);
    background: color-mix(in srgb, var(--layer-2) 10%, transparent);
    border-radius: var(--radius-s);
    padding: 6px 8px;
  }
  .section-title {
    margin-top: 6px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip-label {
    color: inherit;
    font: inherit;
    text-decoration: underline dotted;
    text-underline-offset: 3px;
  }
  .add {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 5px 10px;
    border-radius: var(--radius-pill);
    border: 1px dashed var(--line-strong);
    color: var(--accent);
    font-weight: 500;
  }
  .add.on {
    border-style: solid;
    background: var(--accent-soft);
  }
  .params-box {
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    background: var(--bg-1);
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
    font-size: var(--fs-s);
    margin-left: auto;
  }
  .macro {
    align-self: flex-start;
  }
  .macro.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .tag {
    font-size: var(--fs-xs);
    background: var(--accent-soft);
    border-radius: var(--radius-pill);
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
    font-size: var(--fs-s);
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
