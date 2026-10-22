<script lang="ts">
  // Plain list of controls with their mapping on the current layer. Used for controllers
  // without a drawing, and as an accessible alternative to the drawing.
  import { DEVICE_LAYOUTS, type DeviceId } from '../../assets/devices/layouts';
  import { t } from '../../lib/i18n/index.svelte';
  import {
    CARDINALS,
    DIAGONALS,
    SIDES,
    SIDE_NAMES,
    controlLabel,
    gyroControls,
    touchControls,
    zoneBaseLabel,
    zoneFamily,
    zoneParts,
    zoneTag,
  } from '../../lib/model/controls';
  import { hasMotion } from '../../lib/model/devices';
  import { effectiveSummary } from '../../lib/model/labels';
  import { app } from '../../lib/state/app.svelte';
  import type { ControlName } from '../../lib/types';
  import ControlTile from './ControlTile.svelte';

  /** `device`: the drawing this list stands in for; it lists the buttons that drawing has. */
  let { back = false, device }: { back?: boolean; device: DeviceId } = $props();


  const groups = $derived.by(() => {
    const buttons = (app.catalog?.pad_buttons ?? []) as string[];
    const drawn = new Set(DEVICE_LAYOUTS[device].controls.map((c) => c.control));
    if (back) {
      const paddles = buttons.filter((b) => b.startsWith('Paddle'));
      return [{ id: 'back', controls: ['LB', 'RB', 'LT', 'RT', ...paddles].filter((c) => drawn.has(c)) }];
    }
    return [
      { id: 'buttons', controls: buttons.filter((b) => drawn.has(b)) },
      {
        id: 'sticks',
        // Each direction, then the stick in any direction (which exists only per zone).
        controls: SIDE_NAMES.flatMap((side) => {
          const s = SIDES[side].stick;
          const diag = app.profile?.sticks?.[side]?.diagonals ? DIAGONALS : [];
          return [...CARDINALS, ...diag].map((d) => s + d).concat(`${s}Low`);
        }),
      },
      { id: 'triggers', controls: ['LT', 'RT', 'LTFull', 'RTFull'] },
      ...(drawn.has('Touchpad') ? [{ id: 'touchpad', controls: touchControls(!!app.profile?.touchpad?.split) }] : []),
      ...(hasMotion(device) ? [{ id: 'gyro', controls: gyroControls() }] : []),
    ];
  });

  /** A tile stands for a control and its zones; it opens on `c`. */
  function tile(c: ControlName): { label: string; members: ControlName[] } {
    const zp = zoneParts(c);
    if (!zp) return { label: controlLabel(c, app.family), members: [c] };
    return { label: zoneBaseLabel(zp.base, app.family), members: zoneFamily(zp.base) };
  }

  /** Summary of a tile: the control's own mappings, then its zones' tagged with the zone. */
  function tileSummary(members: ControlName[]): { text: string; inherited: boolean } | null {
    const parts: string[] = [];
    let inherited = true;
    for (const m of members) {
      const s = summary(m);
      if (!s) continue;
      const z = zoneParts(m)?.zone;
      parts.push(z ? `${zoneTag(z)} ${s.text}` : s.text);
      inherited &&= s.inherited;
    }
    return parts.length ? { text: parts.join(' · '), inherited } : null;
  }

  const summary = (c: ControlName) => app.profile && effectiveSummary(app.profile, app.layer, c, app.layerLabel);
</script>

<div class="list">
  {#each groups as g (g.id)}
    <div class="section-title">{t(`views.groups.${g.id}`)}</div>
    <div class="grid">
      {#each g.controls as c (c)}
        {@const tl = tile(c)}
        {@const s = tileSummary(tl.members)}
        <ControlTile
          name={tl.label}
          summary={s}
          selected={tl.members.includes(app.selectedKey)}
          pressed={tl.members.some((m) => app.pressed.has(m))}
          onclick={() => app.select(c)}
        />
      {/each}
    </div>
  {/each}
</div>

<style>
  .list {
    padding: 20px 24px 32px;
  }
  .section-title {
    margin: 14px 0 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 8px;
  }
</style>
