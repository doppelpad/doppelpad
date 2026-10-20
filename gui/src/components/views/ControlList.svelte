<script lang="ts">
  // Plain list of controls with their mapping on the current layer. Used for controllers
  // without a drawing, and as an accessible alternative to the drawing.
  import { t } from '../../lib/i18n/index.svelte';
  import { controlLabel } from '../../lib/model/controls';
  import { effectiveMappings, mappingSummary } from '../../lib/model/profile';
  import { app } from '../../lib/state/app.svelte';
  import type { ControlName } from '../../lib/types';

  let { back = false }: { back?: boolean } = $props();

  const PADDLES = ['PaddleUL', 'PaddleUR', 'PaddleLL', 'PaddleLR'];
  const STICK_DIRS = ['Up', 'Down', 'Left', 'Right'];

  const groups = $derived.by(() => {
    const buttons = (app.catalog?.pad_buttons ?? []) as string[];
    if (back) return [{ id: 'back', controls: PADDLES }];
    return [
      { id: 'buttons', controls: buttons.filter((b) => !PADDLES.includes(b)) },
      { id: 'sticks', controls: ['LeftStick', 'RightStick'].flatMap((s) => STICK_DIRS.map((d) => s + d)) },
      { id: 'triggers', controls: ['LT', 'RT', 'LTFull', 'RTFull'] },
    ];
  });

  const pressed = $derived.by(() => {
    const dev = app.primary;
    const names = app.catalog?.pad_buttons ?? [];
    const s = new Set<string>();
    if (!dev) return s;
    names.forEach((b, i) => (dev.state.buttons >>> i) & 1 && s.add(b));
    return s;
  });

  function summary(c: ControlName): { text: string; inherited: boolean } | null {
    if (!app.profile) return null;
    const eff = effectiveMappings(app.profile, app.layer, c);
    if (eff.mappings.length === 0) return null;
    return { text: eff.mappings.map((m) => mappingSummary(m, app.layerLabel)).join(' · '), inherited: eff.inherited };
  }
</script>

<div class="list">
  {#each groups as g (g.id)}
    <div class="group">{t(`views.groups.${g.id}`)}</div>
    <div class="grid">
      {#each g.controls as c (c)}
        {@const s = summary(c)}
        <button
          class="tile"
          class:selected={app.selectedKey === c}
          class:pressed={pressed.has(c)}
          class:mapped={!!s}
          onclick={() => app.select(c)}
        >
          <span class="name">{controlLabel(c, app.family)}</span>
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
