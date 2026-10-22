<script lang="ts">
  // Mouse as an input device: buttons and wheel on the drawing, the remaining wheel and
  // movement directions as tiles.
  import { t } from '../../lib/i18n/index.svelte';
  import { DIRS8, controlLabel } from '../../lib/model/controls';
  import { effectiveSummary } from '../../lib/model/labels';
  import { app } from '../../lib/state/app.svelte';
  import type { ControlName } from '../../lib/types';
  import ControllerView from './ControllerView.svelte';
  import ControlTile from './ControlTile.svelte';

  const groups = [
    { id: 'mouse_wheel', controls: ['Wheel:Left', 'Wheel:Right'] },
    {
      id: 'mouse_move',
      // Mapping a diagonal makes mouse motion tell eight directions apart.
      controls: [...DIRS8.map((d) => `MouseMove:${d}`), ...['Low', 'Med', 'High'].map((z) => `MouseMove:${z}`)],
    },
  ];

  const summary = (c: ControlName) => app.profile && effectiveSummary(app.profile, app.layer, c, app.layerLabel);
</script>

<div class="mouse">
  <div class="drawing"><ControllerView device="mouse" /></div>

  {#each groups as g (g.id)}
    <div class="section-title">{t(`picker.${g.id}`)}</div>
    <div class="grid">
      {#each g.controls as c (c)}
        {@const s = summary(c)}
        <ControlTile name={controlLabel(c, null)} summary={s} control={c} selected={app.selectedKey === c} onclick={() => app.select(c)} />
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
  .section-title {
    margin: 14px 0 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 8px;
  }
</style>
