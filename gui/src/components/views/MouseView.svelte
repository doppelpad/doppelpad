<script lang="ts">
  // Mouse as an input device: buttons and wheel on the drawing, the remaining wheel and
  // movement directions as tiles.
  import { t } from '../../lib/i18n/index.svelte';
  import { DIRS8, controlLabel } from '../../lib/model/controls';
  import { effectiveSummary } from '../../lib/model/labels';
  import { app } from '../../lib/state/app.svelte';
  import type { ControlName } from '../../lib/types';
  import ControllerView from './ControllerView.svelte';

  const groups = [
    { id: 'mouse_wheel', controls: ['Wheel:Left', 'Wheel:Right'] },
    {
      id: 'mouse_move',
      // Mapping a diagonal makes mouse motion tell eight directions apart.
      controls: [
        ...DIRS8.map((d) => `MouseMove:${d}`),
        ...['Low', 'Med', 'High'].map((z) => `MouseMove:${z}`),
      ],
    },
  ];

  const summary = (c: ControlName) => app.profile && effectiveSummary(app.profile, app.layer, c, app.layerLabel);
</script>

<div class="mouse">
  <div class="drawing"><ControllerView device="mouse" /></div>

  {#each groups as g (g.id)}
    <div class="group">{t(`picker.${g.id}`)}</div>
    <div class="grid">
      {#each g.controls as c (c)}
        {@const s = summary(c)}
        <button class="tile" class:mapped={!!s} class:selected={app.selectedKey === c} onclick={() => app.select(c)}>
          <span class="name">{controlLabel(c, null)}</span>
          <span class="sum" class:inh={s?.inherited}>{s?.text ?? '—'}</span>
        </button>
      {/each}
    </div>
  {/each}
</div>

<style>
  .mouse {
    padding: 8px 24px 32px;
    max-width: 900px;
    margin: 0 auto;
  }
  .drawing {
    height: 380px;
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
