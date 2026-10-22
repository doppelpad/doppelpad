<script lang="ts">
  // One of the desktop app's controller drawings with label chips and leader lines, as in its
  // controller view. Hovering a part or a chip highlights both. `demo` presses the chipped
  // controls one after another, the way live input lights them up in the app. On narrow
  // screens the chips give way to a closer view of the controller.
  import { DEVICE_LAYOUTS, type DeviceId, type LayoutControl } from '../../../gui/src/assets/devices/layouts';
  import { polylinePoints } from '../../../gui/src/lib/svg';

  let {
    device,
    names,
    label,
    chips,
    demo = false,
    trim = [0, 0],
  }: {
    device: DeviceId;
    /** Control → name, as the app shows it (lib/controls.ts). */
    names: Record<string, string>;
    /** Accessible name of the drawing. */
    label: string;
    /** A mapping per control, shown in place of its name; only these controls get a chip. */
    chips?: Record<string, string>;
    demo?: boolean;
    /** Empty space to cut off above and below the drawing, in its viewBox units. */
    trim?: [top: number, bottom: number];
  } = $props();

  const layout = $derived(DEVICE_LAYOUTS[device]);
  const [vx, vy, vw, vh] = $derived(layout.viewBox);
  const frameStyle = $derived.by(() => {
    const h = vh - trim[0] - trim[1];
    return `--frame-ratio: ${vw} / ${h}; --canvas-top: ${(-trim[0] / h) * 100}%; --canvas-height: ${(vh / h) * 100}%;`;
  });
  const shown = $derived(layout.controls.filter((lc) => !chips || lc.control in chips));

  let drawing: HTMLDivElement | undefined = $state();
  let hovered = $state<string | null>(null);
  let pressed = $state<string | null>(null);

  const nameOf = (c: string) => names[c] ?? c;
  const textOf = (c: string) => chips?.[c] ?? nameOf(c);

  function chipStyle(lc: LayoutControl): string {
    const left = ((lc.labelAt[0] - vx) / vw) * 100;
    const top = ((lc.labelAt[1] - vy) / vh) * 100;
    const shift = { left: '-100%, -50%', right: '0, -50%', top: '-50%, -100%', bottom: '-50%, 0' }[lc.side];
    return `left: ${left}%; top: ${top}%; transform: translate(${shift});`;
  }

  const leader = (lc: LayoutControl) => polylinePoints([lc.anchor, ...(lc.via ?? []), lc.labelAt]);

  function onPointerOver(e: PointerEvent) {
    hovered = (e.target as Element).closest('[data-control]')?.getAttribute('data-control') ?? null;
  }

  // Reflect mapped, hovered and pressed controls on the drawing's parts; a pressed stick leans.
  $effect(() => {
    if (!drawing) return;
    void layout;
    for (const g of drawing.querySelectorAll<SVGGElement>('[data-control]')) {
      const c = g.getAttribute('data-control') ?? '';
      g.classList.toggle('mapped', !!chips && c in chips);
      g.classList.toggle('hovered', c === hovered);
      g.classList.toggle('pressed', c === pressed);
      const cap = g.querySelector<SVGGElement>('.stick-cap');
      if (cap) cap.style.transform = c === pressed ? 'translate(10px, -10px)' : '';
    }
  });

  $effect(() => {
    if (!demo || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const order = shown.map((lc) => lc.control);
    let i = 0;
    let release: ReturnType<typeof setTimeout> | undefined;
    const timer = setInterval(() => {
      pressed = order[i++ % order.length];
      release = setTimeout(() => (pressed = null), 650);
    }, 1400);
    return () => {
      clearInterval(timer);
      clearTimeout(release);
    };
  });
</script>

<figure class="pad" aria-label={label}>
  <div class="frame" style={frameStyle}>
    <div class="canvas">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="drawing" bind:this={drawing} onpointerover={onPointerOver} onpointerleave={() => (hovered = null)}>
        {@html layout.svg}
      </div>

      <svg class="leaders" viewBox="{vx} {vy} {vw} {vh}" aria-hidden="true">
        {#each shown as lc (lc.control)}
          {@const on = lc.control === hovered || lc.control === pressed}
          <polyline points={leader(lc)} class:mapped={!!chips} class:on />
          <circle cx={lc.anchor[0]} cy={lc.anchor[1]} r="3" class:mapped={!!chips} class:on />
        {/each}
      </svg>

      {#each shown as lc (lc.control)}
        <!-- Hovering only highlights; there is nothing to operate. -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span
          class="chip"
          class:mapped={!!chips}
          class:on={lc.control === hovered}
          class:pressed={lc.control === pressed}
          style={chipStyle(lc)}
          title={chips ? `${nameOf(lc.control)}: ${textOf(lc.control)}` : undefined}
          onpointerenter={() => (hovered = lc.control)}
          onpointerleave={() => (hovered = null)}
        >
          {textOf(lc.control)}
        </span>
      {/each}
    </div>
  </div>
</figure>

<style>
  .pad {
    margin: 0;
    container-type: inline-size;
  }
  .frame {
    position: relative;
    aspect-ratio: var(--frame-ratio);
    overflow: hidden;
    color: var(--pad-line);
  }
  .canvas {
    position: absolute;
    left: 0;
    width: 100%;
    top: var(--canvas-top);
    height: var(--canvas-height);
  }
  .drawing,
  .leaders {
    position: absolute;
    inset: 0;
  }
  .drawing :global(svg) {
    width: 100%;
    height: 100%;
    display: block;
  }
  .drawing :global([data-control]) {
    color: var(--pad-line);
    transition: color 0.15s;
  }
  .drawing :global([data-control] .hit) {
    transition: fill-opacity 0.15s;
  }
  .drawing :global(.stick-cap) {
    transition: transform 0.25s ease-out;
  }
  .drawing :global([data-control].mapped) {
    color: color-mix(in srgb, var(--pad-accent) 70%, var(--pad-line));
  }
  .drawing :global([data-control].hovered) {
    color: var(--pad-accent);
  }
  .drawing :global([data-control].hovered .hit) {
    fill-opacity: 0.12;
  }
  .drawing :global([data-control].pressed) {
    color: var(--pad-accent-hover);
  }
  .drawing :global([data-control].pressed .hit) {
    fill-opacity: 0.35;
  }
  .leaders {
    pointer-events: none;
    overflow: visible;
  }
  .leaders polyline {
    fill: none;
    stroke: var(--pad-line-soft);
    stroke-width: 1.5;
    transition: stroke 0.15s;
  }
  .leaders circle {
    fill: var(--pad-line);
  }
  .leaders polyline.mapped {
    stroke: color-mix(in srgb, var(--pad-accent) 45%, var(--pad-line-soft));
  }
  .leaders circle.mapped {
    fill: var(--pad-accent);
  }
  .leaders polyline.on {
    stroke: var(--pad-accent);
    stroke-width: 2;
  }
  .leaders circle.on {
    fill: var(--pad-accent);
  }
  .chip {
    position: absolute;
    /* The widest chip the layouts leave room for: 150 of 1000. */
    max-width: 15%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 0.15em 0.6em;
    border-radius: 6px;
    background: var(--pad-chip);
    border: 1px solid var(--pad-line-soft);
    font-size: clamp(9px, 1.3cqw, 13px);
    font-weight: 600;
    line-height: 1.35;
    color: var(--pad-text-dim);
    transition:
      border-color 0.15s,
      box-shadow 0.15s;
  }
  .chip.mapped {
    font-weight: 500;
    color: var(--pad-accent);
  }
  .chip.on {
    border-color: var(--pad-accent);
    color: var(--pad-text);
    z-index: 1;
  }
  .chip.pressed {
    box-shadow: 0 0 0 2px var(--pad-accent);
  }

  /* Too narrow for readable chips: show the controller up close instead. */
  @container (max-width: 34rem) {
    .frame {
      aspect-ratio: 560 / 440;
    }
    .canvas {
      left: -39.29%;
      top: -38.64%;
      width: 178.57%;
      height: 159.09%;
    }
    .chip,
    .leaders {
      display: none;
    }
  }
</style>
