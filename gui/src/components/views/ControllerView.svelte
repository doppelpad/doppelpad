<script lang="ts">
  // A device drawing with a label chip per control, joined by leader lines. Clicking a
  // part or chip selects the control; sticks ask which direction (or the click) to edit.
  // Live input highlights pressed controls and moves the stick caps.
  import { Rotate3d } from '@lucide/svelte';
  import { DEVICE_LAYOUTS, type DeviceId, type LayoutControl } from '../../assets/devices/layouts';
  import { t } from '../../lib/i18n/index.svelte';
  import { hasMotion, moveStickCaps } from '../../lib/model/devices';
  import { buildChips, clickSide, gyroChipOf, pressedChips, type ChipView } from '../../lib/model/chips';
  import { polylinePoints } from '../../lib/svg';
  import { app } from '../../lib/state/app.svelte';
  import type { ControlName } from '../../lib/types';
  import PartMenu, { type PartMenuAt } from './PartMenu.svelte';

  let { device }: { device: DeviceId } = $props();

  const layout = $derived(DEVICE_LAYOUTS[device]);
  const [vx, vy, vw, vh] = $derived(layout.viewBox);
  const motion = $derived(hasMotion(device));

  let drawing: HTMLDivElement | undefined = $state();
  /** Direction picker of a stick, the touchpad regions or the gyro directions, at a point of the drawing. */
  let menu = $state<PartMenuAt | null>(null);
  /** On a split touchpad, which half the touch menu shows. */
  let half = $state<'Left' | 'Right'>('Left');
  let stageEl: HTMLDivElement | undefined = $state();
  let deviceEl: HTMLDivElement | undefined = $state();
  let menuEl: HTMLDivElement | undefined = $state();
  /** The menu opens upward when it would run past the bottom of the view and fits above. */
  let menuUp = $state(false);
  $effect(() => {
    if (!menu || !menuEl || !deviceEl || !stageEl) return;
    const h = menuEl.offsetHeight;
    const d = deviceEl.getBoundingClientRect();
    const s = stageEl.getBoundingClientRect();
    const y = d.top + ((menu.y - vy) / vh) * d.height;
    menuUp = y + 12 + h > s.bottom && y - 12 - h >= s.top;
  });
  const chipView = $derived<ChipView>({
    profile: app.profile,
    layer: app.layer,
    family: app.family,
    selectedKey: app.selectedKey,
    stick: app.stick,
    layerLabel: app.layerLabel,
  });
  const chips = $derived(buildChips(layout.controls, chipView));
  /** Drawn controls held right now; apart from the chips, which do not change with every press. */
  const pressed = $derived(pressedChips(layout.controls, app.pressed));
  const gyroChip = $derived(gyroChipOf(chipView));

  const pct = (x: number, y: number) => ({ left: ((x - vx) / vw) * 100, top: ((y - vy) / vh) * 100 });

  function chipStyle(lc: LayoutControl): string {
    const { left, top } = pct(lc.labelAt[0], lc.labelAt[1]);
    const shift = { left: '-100%, -50%', right: '0, -50%', top: '-50%, -100%', bottom: '-50%, 0' }[lc.side];
    return `left: ${left}%; top: ${top}%; transform: translate(${shift});`;
  }

  const leader = (lc: LayoutControl) => polylinePoints([lc.anchor, ...(lc.via ?? []), lc.labelAt]);

  function choose(c: ControlName, at?: [number, number]) {
    const side = clickSide(c);
    if (side && at) {
      menu = { kind: 'stick', side, x: at[0], y: at[1] };
      return;
    }
    if (c === 'Touchpad' && at) {
      menu = { kind: 'touch', x: at[0], y: at[1] };
      return;
    }
    menu = null;
    app.select(c);
  }

  function openGyroMenu(e: MouseEvent) {
    e.stopPropagation();
    menu = menu?.kind === 'gyro' ? null : { kind: 'gyro', x: vx + vw * 0.12, y: vy + vh * 0.06 };
  }

  function onDrawingClick(e: MouseEvent) {
    const g = (e.target as Element).closest('[data-control]');
    if (!g || !drawing) {
      menu = null;
      return;
    }
    const c = g.getAttribute('data-control')!;
    const box = drawing.getBoundingClientRect();
    const x = ((e.clientX - box.left) / box.width) * vw + vx;
    const y = ((e.clientY - box.top) / box.height) * vh + vy;
    choose(c, [x, y]);
  }

  // Reflect state on the drawing's parts: mapped and selected ...
  $effect(() => {
    if (!drawing) return;
    const byControl = new Map(chips.map((ch) => [ch.lc.control, ch]));
    for (const g of drawing.querySelectorAll<SVGGElement>('[data-control]')) {
      const ch = byControl.get(g.getAttribute('data-control') ?? '');
      g.classList.toggle('mapped', !!ch?.mapped);
      g.classList.toggle('selected', !!ch?.selected);
    }
  });
  // ... held ...
  $effect(() => {
    if (!drawing) return;
    for (const g of drawing.querySelectorAll<SVGGElement>('[data-control]')) {
      g.classList.toggle('pressed', pressed.has(g.getAttribute('data-control') ?? ''));
    }
  });
  // ... and the sticks' deflection (again when another drawing comes in).
  $effect(() => {
    void layout;
    if (drawing) moveStickCaps(drawing, app.primary?.state.axes ?? [0, 0, 0, 0]);
  });
</script>

<div class="stage" bind:this={stageEl}>
  <div class="device" style="aspect-ratio: {vw} / {vh}" bind:this={deviceEl}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="drawing" bind:this={drawing} onclick={onDrawingClick}>
      {@html layout.svg}
    </div>

    <svg class="leaders" viewBox="{vx} {vy} {vw} {vh}" aria-hidden="true">
      {#each chips as ch (ch.lc.control)}
        <polyline points={leader(ch.lc)} class:mapped={ch.mapped} class:selected={ch.selected} />
        <circle cx={ch.lc.anchor[0]} cy={ch.lc.anchor[1]} r="3" class:mapped={ch.mapped} class:selected={ch.selected} />
      {/each}
    </svg>

    {#each chips as ch (ch.lc.control)}
      <button
        class="chip side-{ch.lc.side}"
        class:mapped={ch.mapped}
        class:inherited={ch.inherited}
        class:selected={ch.selected}
        class:pressed={pressed.has(ch.lc.control)}
        style={chipStyle(ch.lc)}
        title={ch.summary ? `${ch.label}: ${ch.summary}` : ch.label}
        onclick={(e) => {
          e.stopPropagation();
          const c = ch.lc.control;
          choose(c, clickSide(c) || c === 'Touchpad' ? ch.lc.labelAt : undefined);
        }}
      >
        {#if ch.summary}
          <span class="sum">{ch.summary}</span>
        {:else}
          <span class="name">{ch.label}</span>
        {/if}
        {#if ch.combo}<span class="combo" title={t('views.shortcuts')}>+</span>{/if}
      </button>
    {/each}

    {#if motion}
      <button
        class="chip gyro"
        class:mapped={!!gyroChip.summary}
        class:selected={gyroChip.selected || menu?.kind === 'gyro'}
        title={gyroChip.summary ? `${t('binding.gyro')}: ${gyroChip.summary}` : t('binding.gyro')}
        onclick={openGyroMenu}
      >
        <Rotate3d size={13} />
        {#if gyroChip.summary}
          <span class="sum">{gyroChip.summary}</span>
        {:else}
          <span class="name">{t('binding.gyro')}</span>
        {/if}
      </button>
    {/if}

    {#if menu}
      {@const p = pct(menu.x, menu.y)}
      <PartMenu
        {menu}
        left={p.left}
        top={p.top}
        up={menuUp}
        bind:half
        bind:el={menuEl}
        onchoose={(c) => choose(c)}
        onclose={() => (menu = null)}
      />
    {/if}
  </div>
</div>

<svelte:window onkeydown={(e) => e.key === 'Escape' && (menu = null)} />

<style>
  .stage {
    height: 100%;
    display: grid;
    place-items: center;
    padding: 12px 20px;
    container-type: size;
  }
  .device {
    position: relative;
    width: min(100cqw - 8px, (100cqh - 8px) * 1000 / 700);
    color: var(--text-faint);
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
    cursor: pointer;
    color: var(--line-strong);
    transition: color 0.12s;
  }
  .drawing :global([data-control] .hit) {
    transition: fill-opacity 0.12s;
  }
  .drawing :global([data-control]:hover) {
    color: var(--text-dim);
  }
  .drawing :global([data-control]:hover .hit) {
    fill-opacity: 0.08;
  }
  .drawing :global([data-control].mapped) {
    color: color-mix(in srgb, var(--accent) 70%, var(--line-strong));
  }
  .drawing :global([data-control].selected) {
    color: var(--accent);
  }
  .drawing :global([data-control].selected .hit) {
    fill-opacity: 0.16;
  }
  .drawing :global([data-control].pressed) {
    color: var(--accent-hover);
  }
  .drawing :global([data-control].pressed .hit) {
    fill-opacity: 0.35;
  }
  .drawing :global(.body) {
    color: var(--line-strong);
  }
  .leaders {
    pointer-events: none;
    overflow: visible;
  }
  .leaders polyline {
    fill: none;
    stroke: var(--line);
    stroke-width: 1.5;
  }
  .leaders circle {
    fill: var(--line-strong);
  }
  .leaders .mapped {
    stroke: color-mix(in srgb, var(--accent) 45%, var(--line));
  }
  .leaders circle.mapped {
    fill: var(--accent);
    stroke: none;
  }
  .leaders .selected {
    stroke: var(--accent);
    stroke-width: 2;
  }
  .leaders circle.selected {
    fill: var(--accent);
  }
  .chip {
    position: absolute;
    /* The largest chip the layouts leave room for: generate.mjs' CHIP, 150 of 1000. */
    max-width: 15%;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px 7px;
    border-radius: var(--radius-s);
    background: var(--bg-1);
    border: 1px solid var(--line);
    text-align: left;
    line-height: 1.25;
    white-space: nowrap;
    z-index: 1;
  }
  .chip:hover {
    border-color: var(--line-strong);
    z-index: 2;
  }
  .name {
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-dim);
  }
  .sum {
    font-size: 11.5px;
    font-weight: 500;
    color: var(--accent);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chip.mapped .name {
    color: var(--text);
  }
  .chip.inherited .sum {
    color: var(--text-faint);
    font-style: italic;
  }
  .chip.selected {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, var(--bg-1));
  }
  .chip.pressed {
    box-shadow: 0 0 0 2px var(--accent);
  }
  .chip.gyro {
    left: 0;
    top: 0;
    gap: 5px;
    max-width: 30%;
    color: var(--text-dim);
  }
  .chip.gyro :global(svg) {
    flex: none;
  }
  .combo {
    flex: none;
    margin-left: 2px;
    color: var(--layer-1);
    font-weight: 700;
  }
</style>
