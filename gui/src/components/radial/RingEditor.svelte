<script lang="ts">
  // A radial menu drawn as it shows over the game, to pick a sector by clicking it (or with
  // the keyboard: each sector is a button). The picked one is outlined in the accent colour.
  import { Image } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { sectorCentre, sectorPath } from '../../lib/model/radial';
  import type { RadialMenu } from '../../lib/types';
  import { RADIAL_ICONS, isImageIcon } from './icons';

  let { menu, selected, onselect }: { menu: RadialMenu; selected: number | null; onselect: (k: number) => void } = $props();

  const OUTER = 100;
  const INNER = 36;
  const n = $derived(menu.sectors.length);
  /** The name in the hole, shortened to fit. */
  const name = $derived((menu.name ?? '').length > 14 ? `${menu.name!.slice(0, 13)}…` : (menu.name ?? ''));

  function onkeydown(e: KeyboardEvent, k: number) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onselect(k);
    } else if (e.key === 'ArrowRight' || e.key === 'ArrowDown' || e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
      e.preventDefault();
      const to = (k + (e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : -1) + n) % n;
      onselect(to);
      (e.currentTarget as SVGElement).parentElement?.querySelectorAll<SVGElement>('.sector')[to]?.focus();
    }
  }
</script>

<svg class="ring" viewBox="-106 -106 212 212" role="group" aria-label={menu.name || t('radial.title')}>
  {#each menu.sectors as s, k (k)}
    {@const tone = s.color || 'var(--accent)'}
    {@const [cx, cy] = sectorCentre(k, n, (OUTER + INNER) / 2)}
    {@const Icon = s.icon && !isImageIcon(s.icon) ? RADIAL_ICONS[s.icon] : isImageIcon(s.icon) ? Image : undefined}
    <g
      class="sector"
      class:on={selected === k}
      role="button"
      tabindex="0"
      aria-pressed={selected === k}
      aria-label={s.label || t('radial.sector_n', { n: k + 1 })}
      style:--tone={tone}
      onclick={() => onselect(k)}
      onkeydown={(e) => onkeydown(e, k)}
    >
      <path d={sectorPath(k, n, INNER, OUTER)} />
      <g transform="translate({cx}, {cy})">
        {#if Icon}
          <g transform="translate(-8, {s.label ? -14 : -8})" class="icon"><Icon size={16} /></g>
        {/if}
        <text y={Icon ? (s.label ? 12 : 0) : 3.5} text-anchor="middle" class="label">
          {s.label || (Icon ? '' : k + 1)}{s.submenu ? ' ›' : ''}
        </text>
      </g>
    </g>
  {/each}
  <circle r={INNER - 4} class="hole" />
  <text y="3" text-anchor="middle" class="name">{name}</text>
</svg>

<style>
  .ring {
    width: 100%;
    max-width: 340px;
    aspect-ratio: 1;
    display: block;
    overflow: visible;
  }
  .sector {
    cursor: pointer;
    outline: none;
  }
  .sector path {
    fill: color-mix(in srgb, var(--tone) 14%, var(--bg-2));
    stroke: var(--line-strong);
    stroke-width: 1.2;
    transition:
      fill 0.12s,
      stroke 0.12s;
  }
  .sector:hover path {
    fill: color-mix(in srgb, var(--tone) 28%, var(--bg-2));
  }
  .sector.on path {
    fill: color-mix(in srgb, var(--tone) 40%, var(--bg-2));
    stroke: var(--accent-hover);
    stroke-width: 2.4;
  }
  .sector:focus-visible path {
    stroke: var(--accent-hover);
    stroke-width: 2.4;
  }
  .icon {
    color: var(--tone);
  }
  .sector.on .icon {
    color: var(--text);
  }
  .label {
    fill: var(--text);
    font-size: 8.5px;
    font-weight: 600;
    pointer-events: none;
  }
  .hole {
    fill: var(--bg-1);
    stroke: var(--line);
    stroke-width: 1;
  }
  .name {
    fill: var(--text-dim);
    font-size: 8px;
    font-weight: 600;
  }
</style>
