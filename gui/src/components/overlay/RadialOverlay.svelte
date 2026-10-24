<script lang="ts">
  // The open radial menu, sent by the app (radial.rs, see lib/overlay.ts). The window covers
  // the primary monitor; the ring sits at the menu's position (an edge, a corner, the middle
  // or where the mouse pointer was) plus its offset.
  import { onMount } from 'svelte';
  import { overlayContent } from '../../lib/overlay';
  import { RING, ringCentre, sectorCentre, sectorPath } from '../../lib/model/radial';
  import { RADIAL_ICONS } from '../radial/icons';
  import type { RadialContent } from '../../lib/types';

  let content = $state<RadialContent | null>(null);
  let width = $state(0);
  let height = $state(0);

  onMount(() => overlayContent<RadialContent>((c) => (content = c)));

  const menu = $derived(content?.menu);
  const n = $derived(menu?.sectors.length ?? 0);
  const outer = $derived(RING * (menu?.scale ?? 1));
  const inner = $derived(outer / 3);
  const half = $derived(outer + 4);
  const pointed = $derived(content?.pointed ?? null);
  const pointedLabel = $derived(pointed !== null ? (menu?.sectors[pointed]?.label ?? '') : '');
  const centre = $derived(
    menu ? ringCentre(menu.position ?? 'center', menu.offset ?? [0, 0], [width, height], half, content?.cursor ?? null) : [0, 0],
  );
</script>

<svelte:window bind:innerWidth={width} bind:innerHeight={height} />

{#if menu && content}
  <div class="screen" style:background="rgba(0, 0, 0, {menu.tint ?? 0})">
    <svg
      class="ring"
      width={half * 2}
      height={half * 2}
      viewBox="{-half} {-half} {half * 2} {half * 2}"
      style:left="{centre[0] - half}px"
      style:top="{centre[1] - half}px"
      aria-hidden="true"
    >
      {#each menu.sectors as s, k (k)}
        {@const tone = s.color || 'var(--accent)'}
        {@const [cx, cy] = sectorCentre(k, n, (outer + inner) / 2)}
        {@const Icon = s.icon ? RADIAL_ICONS[s.icon] : undefined}
        {@const image = content.images[k]}
        {@const active = k === pointed}
        <path
          d={sectorPath(k, n, inner, outer)}
          fill={active ? tone : 'color-mix(in srgb, var(--bg-1) 90%, transparent)'}
          fill-opacity={active ? 0.85 : 1}
          stroke={active ? tone : 'var(--line-strong)'}
          stroke-width="2"
        />
        <g transform="translate({cx}, {cy})" class:active>
          {#if image}
            <image href={image} x={-18} y={-26} width="36" height="36" />
          {:else if Icon}
            <g transform="translate(-14, -22)" color={active ? 'var(--bg-0)' : tone}><Icon size={28} /></g>
          {/if}
          {#if s.label}
            <text y={image || Icon ? 24 : 5} text-anchor="middle" class="label">{s.label}{s.submenu ? ' ›' : ''}</text>
          {/if}
        </g>
      {/each}
      <circle r={inner - 6} fill="color-mix(in srgb, var(--bg-0) 92%, transparent)" stroke="var(--line-strong)" stroke-width="2" />
      <text y="-4" text-anchor="middle" class="centre-label">{pointedLabel}</text>
      {#if menu.name}<text y="18" text-anchor="middle" class="menu-name">{menu.name}</text>{/if}
    </svg>
  </div>
{/if}

<style>
  .screen {
    position: fixed;
    inset: 0;
  }
  .ring {
    position: absolute;
  }
  .label {
    fill: var(--text);
    font-size: 13px;
    font-weight: 600;
  }
  .active .label {
    fill: var(--bg-0);
  }
  .centre-label {
    fill: var(--text);
    font-size: 15px;
    font-weight: 600;
  }
  .menu-name {
    fill: var(--text-dim);
    font-size: var(--fs-s);
  }
</style>
