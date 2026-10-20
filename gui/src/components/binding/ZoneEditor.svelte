<script lang="ts">
  // Zone thresholds of the selected stick or trigger: a bar with the zones and the live
  // value, and where each zone starts. Shared by every mapping of that stick or trigger on
  // a layer; a layer may have zones of its own.
  import { untrack } from 'svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { layerOverrides, triggerConfig } from '../../lib/model/analog';
  import type { ZoneName, ZoneParts } from '../../lib/model/controls';
  import {
    LEVELS,
    axisOf,
    isElliptic,
    setElliptic,
    setStickLevel,
    setTriggerLevel,
    stickThresholds,
    stickZoneAt,
    storedZones,
    triggerThresholds,
    triggerZoneAt,
    withZones,
    zoneProblem,
    type Level,
    type Triple,
    type ZoneDefaults,
  } from '../../lib/model/zones';
  import { app } from '../../lib/state/app.svelte';
  import type { StickZones, TriggerZones } from '../../lib/types';

  let { parts }: { parts: ZoneParts } = $props();

  const ZONE_OF: Record<Level, ZoneName> = { low: 'Low', med: 'Med', high: 'High' };

  const source = $derived(parts.source);
  const isStick = $derived(source.kind === 'stick');
  const defaults = $derived<ZoneDefaults>({
    stickLow: app.catalog?.defaults.stick.zones.low ?? 0.24,
    triggerLow: app.catalog?.defaults.trigger.zones.low ?? 0.03,
    fullPull: app.catalog?.defaults.trigger.full_pull ?? 0.99,
  });
  // Zones are per config; a layer may have its own (shown and edited on that layer).
  const zonesHere = $derived(app.profile ? storedZones(app.profile, source, app.layer) : undefined);
  const stickZones = $derived(isStick ? (zonesHere as StickZones | undefined) : undefined);
  const ownZones = $derived(
    !!app.profile && layerOverrides(app.profile, app.layer, isStick ? 'sticks' : 'triggers', source.side).includes('zones'),
  );
  const trigger = $derived.by(() => {
    const def = app.catalog?.defaults.trigger;
    if (isStick || !app.profile || !def) return undefined;
    return { zones: zonesHere as TriggerZones | undefined, full_pull: triggerConfig(app.profile, source.side, def, app.layer).full_pull };
  });

  const th = $derived.by<{ x: Triple; y: Triple }>(() => {
    if (isStick) return stickThresholds(stickZones, defaults);
    const tr = triggerThresholds(trigger?.zones, trigger?.full_pull, defaults);
    return { x: tr, y: tr };
  });
  const problem = $derived(zoneProblem(th.x) ?? zoneProblem(th.y));

  /** Edit horizontal and vertical thresholds separately (elliptic zones). */
  let separate = $state(false);
  $effect(() => {
    void source.kind;
    void source.side;
    separate = untrack(() => isStick && isElliptic(stickZones));
  });
  const axes = $derived<('both' | 0 | 1)[]>(separate ? [0, 1] : ['both']);

  const live = $derived.by(() => {
    const a = app.primary?.state.axes;
    if (!a) return null;
    if (isStick) {
      const [x, y] = source.side === 'left' ? [a[0], a[1]] : [a[2], a[3]];
      return { x: Math.abs(x), y: Math.abs(y), mag: Math.hypot(x, y), zone: stickZoneAt(x, y, th) };
    }
    const v = a[source.side === 'left' ? 4 : 5];
    return { x: v, y: v, mag: v, zone: triggerZoneAt(v, th.x) };
  });

  const pct = (v: number) => String(Math.round(v * 1000) / 10);

  function stored(level: Level, axis: 'both' | 0 | 1): number | undefined {
    if (isStick) return axisOf(stickZones?.[level], axis === 1 ? 1 : 0);
    return trigger?.zones?.[level];
  }

  function effective(level: Level, axis: 'both' | 0 | 1): number {
    return (axis === 1 ? th.y : th.x)[LEVELS.indexOf(level)];
  }

  function set(level: Level, axis: 'both' | 0 | 1, raw: string) {
    if (!app.profile) return;
    const n = parseFloat(raw.replace(',', '.'));
    const v = Number.isFinite(n) ? Math.min(Math.max(n, 0), 100) / 100 : undefined;
    const zones = isStick
      ? setStickLevel(stickZones, level, axis, v, defaults)
      : setTriggerLevel(trigger?.zones, level, v);
    app.update(withZones(app.profile, source, zones, app.layer));
  }

  function setSeparate(on: boolean) {
    separate = on;
    if (!on && app.profile && isElliptic(stickZones)) {
      app.update(withZones(app.profile, source, setElliptic(stickZones, false), app.layer));
    }
  }

  /** Segment from `a` to `b` (fractions) as inline style. */
  const seg = (a: number, b: number) => {
    const l = Math.min(Math.max(a, 0), 1);
    const r = Math.min(Math.max(b, l), 1);
    return `left: ${l * 100}%; width: ${(r - l) * 100}%`;
  };
</script>

{#snippet bar(tr: Triple, value: number | null, label: string | null)}
  <div class="bar-row">
    {#if label}<span class="axis">{label}</span>{/if}
    <div class="bar">
      <span class="seg dead" style={seg(0, tr[0])} title={t('zones.deadzone')}></span>
      <span class="seg low" class:on={parts.zone === 'Low'} style={seg(tr[0], tr[1])}></span>
      <span class="seg med" class:on={parts.zone === 'Med'} style={seg(tr[1], tr[2])}></span>
      <span class="seg high" class:on={parts.zone === 'High'} style={seg(tr[2], 1)}></span>
      {#if value !== null}
        <span class="marker" style="left: {Math.min(Math.max(value, 0), 1) * 100}%"></span>
      {/if}
    </div>
  </div>
{/snippet}

<div class="zone-editor">
  <div class="section-title">{t('zones.thresholds')}</div>
  <p class="hint">
    {t('zones.thresholds_hint')}
    {t(isStick ? 'zones.shared_stick' : 'zones.shared_trigger')}
    {#if app.layer !== 0}
      {t(ownZones ? 'zones.layer_own' : 'zones.layer_main', { layer: app.layerLabel(app.layer) })}
    {/if}
  </p>

  {#if separate}
    {@render bar(th.x, live?.x ?? null, t('zones.horizontal'))}
    {@render bar(th.y, live?.y ?? null, t('zones.vertical'))}
  {:else}
    {@render bar(th.x, live?.mag ?? null, null)}
  {/if}
  {#if live}
    <div class="now">
      {live.zone ? t('zones.now', { zone: t(`zones.${live.zone.toLowerCase()}`) }) : t('zones.now_none')}
    </div>
  {/if}

  <div class="grid" class:two={separate}>
    {#if separate}
      <span></span>
      <span class="col">{t('zones.horizontal')}</span>
      <span class="col">{t('zones.vertical')}</span>
    {/if}
    {#each LEVELS as level (level)}
      <span class="lvl" class:on={parts.zone === ZONE_OF[level]}>{t(`zones.${level}`)}</span>
      {#each axes as axis (axis)}
        {@const v = stored(level, axis)}
        <label class="pct">
          <input
            type="number"
            min="0"
            max="100"
            step="1"
            value={v === undefined ? '' : pct(v)}
            placeholder={pct(effective(level, axis))}
            disabled={app.readOnly}
            aria-label="{t(`zones.${level}`)}{axis === 0 ? ` ${t('zones.horizontal')}` : axis === 1 ? ` ${t('zones.vertical')}` : ''}"
            onchange={(e) => set(level, axis, e.currentTarget.value)}
          />
          %
        </label>
      {/each}
    {/each}
  </div>

  {#if isStick}
    <label class="check">
      <input
        type="checkbox"
        checked={separate}
        disabled={app.readOnly}
        onchange={(e) => setSeparate(e.currentTarget.checked)}
      />
      {t('zones.separate_axes')}
    </label>
  {/if}

  {#if problem}
    <p class="problem">{t(problem)}</p>
  {/if}
</div>

<style>
  .zone-editor {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .section-title {
    margin-top: 6px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-dim);
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--text-faint);
  }
  .bar-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .axis {
    width: 72px;
    font-size: 11px;
    color: var(--text-dim);
  }
  .bar {
    position: relative;
    flex: 1;
    height: 14px;
    border-radius: 99px;
    background: var(--bg-0);
    border: 1px solid var(--line);
    overflow: hidden;
  }
  .seg {
    position: absolute;
    top: 0;
    bottom: 0;
  }
  .seg.dead {
    background: repeating-linear-gradient(135deg, var(--bg-3) 0 4px, transparent 4px 8px);
  }
  .seg.low {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .seg.med {
    background: color-mix(in srgb, var(--accent) 42%, transparent);
  }
  .seg.high {
    background: color-mix(in srgb, var(--accent) 64%, transparent);
  }
  .seg.on {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .marker {
    position: absolute;
    top: -1px;
    bottom: -1px;
    width: 3px;
    margin-left: -1px;
    background: var(--text);
    border-radius: 2px;
  }
  .now {
    font-size: 12px;
    color: var(--text-dim);
  }
  .grid {
    display: grid;
    grid-template-columns: auto auto;
    justify-content: start;
    align-items: center;
    gap: 6px 12px;
  }
  .grid.two {
    grid-template-columns: auto auto auto;
  }
  .col {
    font-size: 11px;
    color: var(--text-dim);
  }
  .lvl {
    color: var(--text-dim);
    font-size: 12px;
    min-width: 64px;
  }
  .lvl.on {
    color: var(--accent);
    font-weight: 600;
  }
  .pct {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text-dim);
    font-size: 12px;
  }
  .pct input {
    width: 64px;
    padding: 4px 6px;
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .problem {
    margin: 0;
    font-size: 12px;
    color: var(--warning);
  }
</style>
