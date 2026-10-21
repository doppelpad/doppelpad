<script lang="ts">
  // A crosshair drawn as SVG: the outline first (wider strokes), then the shape. Fills its
  // box; `size` logical pixels of crosshair map to `px` pixels on screen.
  import { crosshairParts, lineWidth, type Part } from '../../lib/model/crosshair';
  import type { CrosshairFull } from '../../lib/types';

  let { crosshair: c, px }: { crosshair: CrosshairFull; px?: number } = $props();

  const w = $derived(lineWidth(c));
  const parts = $derived(crosshairParts(c.shape, c.size, w));
  /** Half the box: the shape, its outline and a pixel of room. */
  const half = $derived(c.size / 2 + c.outline + w);
  const box = $derived(px ?? half * 2);

  const points = (p: Part) => ('points' in p ? p.points.map(([x, y]) => `${x},${y}`).join(' ') : '');
</script>

<svg width={box} height={box} viewBox="{-half} {-half} {half * 2} {half * 2}" opacity={c.opacity} aria-hidden="true">
  {#each [true, false] as under (under)}
    {#if !under || c.outline > 0}
      {@const color = under ? c.outline_color : c.color}
      {@const extra = under ? c.outline * 2 : 0}
      {#each parts as p, i (i)}
        {#if 'points' in p}
          <polyline
            points={points(p)}
            fill="none"
            stroke={color}
            stroke-width={w + extra}
            stroke-linecap={under ? 'square' : 'butt'}
            stroke-linejoin="miter"
          />
        {:else if p.fill}
          <circle r={p.r + extra / 2} fill={color} />
        {:else}
          <circle r={p.r} fill="none" stroke={color} stroke-width={w + extra} />
        {/if}
      {/each}
    {/if}
  {/each}
</svg>
