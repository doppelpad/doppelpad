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
    <div class="group">{t(`views.groups.${g.id}`)}</div>
    <div class="grid">
      {#each g.controls as c (c)}
        {@const tl = tile(c)}
        {@const s = tileSummary(tl.members)}
        <button
          class="tile"
          class:selected={tl.members.includes(app.selectedKey)}
          class:pressed={tl.members.some((m) => app.pressed.has(m))}
          class:mapped={!!s}
          onclick={() => app.select(c)}
        >
          <span class="name">{tl.label}</span>
          <span class="sum" class:inh={s?.inherited}>{s?.text ?? '—'}</span>
        </button>
      {/each}
    </div>
  {/each}
</div>

<style>
  .list {
    padding: 20px 24px 32px;
  }
  .group {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-dim);
    margin: 14px 0 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 8px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 9px 12px;
    text-align: left;
    border-radius: var(--radius-m);
    border: 1px solid var(--line);
    background: var(--bg-1);
    min-width: 0;
  }
  .tile:hover {
    border-color: var(--line-strong);
  }
  .tile.mapped {
    border-left: 3px solid var(--accent);
  }
  .tile.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .tile.pressed {
    box-shadow: 0 0 0 2px var(--accent);
  }
  .name {
    font-weight: 600;
  }
  .sum {
    color: var(--text-dim);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sum.inh {
    color: var(--text-faint);
    font-style: italic;
  }
</style>
