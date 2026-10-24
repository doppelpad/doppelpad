<script lang="ts">
  // Where a radial menu sits on the screen: a small screen with nine places to click (corners,
  // edges, the middle), or at the mouse pointer; then fine-tuned by an offset in pixels.
  import { MousePointer2 } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { POSITION_GRID } from '../../lib/model/radial';
  import { tip } from '../../lib/tooltip.svelte';
  import type { RadialPosition } from '../../lib/types';

  let {
    position,
    offset,
    disabled = false,
    onchange,
  }: {
    position: RadialPosition;
    offset: [number, number];
    disabled?: boolean;
    onchange: (patch: { position?: RadialPosition; offset?: [number, number] }) => void;
  } = $props();

  function setOffset(axis: 0 | 1, raw: string) {
    const v = Math.round(Number(raw));
    if (!Number.isFinite(v) || Math.abs(v) > 10000) return;
    const next: [number, number] = [...offset];
    next[axis] = v;
    onchange({ offset: next });
  }
</script>

<div class="position">
  <div class="screen" role="radiogroup" aria-label={t('radial.position')}>
    {#each POSITION_GRID as row, r (r)}
      {#each row as p (p)}
        <button
          type="button"
          role="radio"
          class="spot"
          class:on={position === p}
          aria-checked={position === p}
          aria-label={t(`radial.positions.${p}`)}
          {disabled}
          use:tip={t(`radial.positions.${p}`)}
          onclick={() => onchange({ position: p })}
        >
          <span class="dot"></span>
        </button>
      {/each}
    {/each}
  </div>
  <div class="side">
    <button
      type="button"
      class="btn cursor"
      class:on={position === 'cursor'}
      aria-pressed={position === 'cursor'}
      {disabled}
      onclick={() => onchange({ position: position === 'cursor' ? 'center' : 'cursor' })}
    >
      <MousePointer2 size={14} />
      {t('radial.positions.cursor')}
    </button>
    <div class="offset">
      <label>
        <span>{t('radial.offset_x')}</span>
        <input type="number" step="10" value={offset[0]} {disabled} onchange={(e) => setOffset(0, e.currentTarget.value)} />
      </label>
      <label>
        <span>{t('radial.offset_y')}</span>
        <input type="number" step="10" value={offset[1]} {disabled} onchange={(e) => setOffset(1, e.currentTarget.value)} />
      </label>
      <span class="unit">px</span>
    </div>
    <p class="hint">{t(position === 'cursor' ? 'radial.cursor_hint' : 'radial.offset_hint')}</p>
  </div>
</div>

<style>
  .position {
    display: flex;
    gap: 16px;
    align-items: flex-start;
  }
  /* A screen in 16:9 with the nine places. */
  .screen {
    flex: none;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    grid-template-rows: repeat(3, 1fr);
    width: 128px;
    aspect-ratio: 16 / 9;
    padding: 4px;
    gap: 2px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-s);
    background: var(--bg-0);
  }
  .spot {
    display: grid;
    place-items: center;
    border-radius: 4px;
  }
  .spot:hover:not(:disabled) {
    background: var(--bg-3);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 1.5px solid var(--text-faint);
  }
  .spot.on .dot {
    width: 12px;
    height: 12px;
    border-color: var(--accent);
    background: var(--accent);
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .cursor {
    align-self: flex-start;
  }
  .cursor.on {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .offset {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-dim);
  }
  .offset label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .offset input {
    width: 72px;
  }
  .unit {
    color: var(--text-faint);
  }
</style>
