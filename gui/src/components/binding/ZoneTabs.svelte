<script lang="ts">
  // The zones of a stick direction, trigger or gyro direction as tabs (a trigger's full pull
  // next to them), what the chosen one means, and its settings.
  import { t } from '../../lib/i18n/index.svelte';
  import { ZONES, fullPullBase, isAnyDirection, withZone, zoneParts, type ZoneName } from '../../lib/model/controls';
  import { effectiveMappings } from '../../lib/model/profile';
  import { app } from '../../lib/state/app.svelte';
  import type { ControlName } from '../../lib/types';
  import Segmented, { type SegmentedOption } from '../ui/Segmented.svelte';
  import ZoneEditor from './ZoneEditor.svelte';

  let { control }: { control: ControlName } = $props();

  /** The trigger (`LT`/`RT`) whose full pull is selected, which sits next to its zones. */
  const fullPull = $derived(fullPullBase(control));
  /** A stick direction, trigger or one of their zones: the panel offers its zones. */
  const zone = $derived(zoneParts(fullPull ?? control));
  /** Zones (and the whole range, `null`) that have a mapping on this layer. */
  const usedZones = $derived.by(() => {
    const used = new Set<ZoneName | null>();
    const profile = app.profile;
    if (!profile || !zone) return used;
    for (const z of [null, ...ZONES]) {
      if (effectiveMappings(profile, app.layer, withZone(zone.base, z)).mappings.length > 0) used.add(z);
    }
    return used;
  });
  const usedFull = $derived(
    !!app.profile && !!zone && zone.source.kind === 'trigger' &&
      effectiveMappings(app.profile, app.layer, `${zone.base}Full`).mappings.length > 0,
  );
  /** The tabs: the whole range (not for "any direction"), the zones and a trigger's full pull. */
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
</script>

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

<style>
  .act-desc {
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
</style>
