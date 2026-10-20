<script lang="ts">
  // A device drawing with a label chip per control, joined by leader lines. Clicking a
  // part or chip selects the control; sticks ask which direction (or the click) to edit.
  // Live input highlights pressed controls and moves the stick caps.
  import { SlidersHorizontal } from '@lucide/svelte';
  import { DEVICE_LAYOUTS, type DeviceId, type LayoutControl } from '../../assets/devices/layouts';
  import { t } from '../../lib/i18n/index.svelte';
  import { controlShort } from '../../lib/model/controls';
  import { effectiveMappings, mappingSummary, shortcutMembers } from '../../lib/model/profile';
  import { app } from '../../lib/state/app.svelte';
  import type { ControlName } from '../../lib/types';

  let { device }: { device: DeviceId } = $props();

  const layout = $derived(DEVICE_LAYOUTS[device]);
  const [vx, vy, vw, vh] = $derived(layout.viewBox);

  let drawing: HTMLDivElement | undefined = $state();
  let stickMenu = $state<{ stick: 'Left' | 'Right'; click: ControlName; x: number; y: number } | null>(null);

  const DIRS = ['Up', 'Down', 'Left', 'Right'] as const;
  const ARROW: Record<string, string> = { Up: '↑', Down: '↓', Left: '←', Right: '→' };

  const combos = $derived(app.profile ? shortcutMembers(app.profile, app.layer) : new Set<string>());

  function summaryOf(c: ControlName): { text: string; inherited: boolean } | null {
    if (!app.profile) return null;
    const eff = effectiveMappings(app.profile, app.layer, c);
    if (eff.mappings.length === 0) return null;
    return { text: eff.mappings.map((m) => mappingSummary(m, app.layerLabel)).join(' · '), inherited: eff.inherited };
  }

  /** Stick name for a stick-click control (`LS` → `LeftStick`). */
  function stickOf(c: ControlName): string | null {
    return c === 'LS' ? 'LeftStick' : c === 'RS' ? 'RightStick' : null;
  }

  interface Chip {
    lc: LayoutControl;
    label: string;
    summary: string;
    mapped: boolean;
    inherited: boolean;
    selected: boolean;
    pressed: boolean;
    combo: boolean;
  }

  const chips = $derived<Chip[]>(
    layout.controls.map((lc) => {
      const c = lc.control;
      const own = summaryOf(c);
      const stick = stickOf(c);
      let summary = own?.text ?? '';
      let mapped = !!own;
      let selected = app.selectedKey === c;
      // app.pressed is also what press-to-select reacts to, so highlight and selection agree.
      let isPressed = app.pressed.has(c);
      if (stick) {
        // Stick chips also show the four directions.
        const parts: string[] = [];
        for (const d of DIRS) {
          const s = summaryOf(stick + d);
          if (s) parts.push(`${ARROW[d]} ${s.text}`);
          if (app.selectedKey === stick + d) selected = true;
          if (app.pressed.has(stick + d)) isPressed = true;
        }
        if (parts.length) {
          summary = [own ? `● ${own.text}` : '', ...parts].filter(Boolean).join('  ');
          mapped = true;
        }
      }
      return {
        lc,
        label: controlShort(c, app.family),
        summary,
        mapped,
        inherited: !!own?.inherited,
        selected,
        pressed: isPressed,
        combo: combos.has(c),
      };
    }),
  );

  const pct = (x: number, y: number) => ({ left: ((x - vx) / vw) * 100, top: ((y - vy) / vh) * 100 });

  function chipStyle(lc: LayoutControl): string {
    const { left, top } = pct(lc.labelAt[0], lc.labelAt[1]);
    const shift = { left: '-100%, -50%', right: '0, -50%', top: '-50%, -100%', bottom: '-50%, 0' }[lc.side];
    return `left: ${left}%; top: ${top}%; transform: translate(${shift});`;
  }

  function leader(lc: LayoutControl): string {
    return [lc.anchor, ...(lc.via ?? []), lc.labelAt].map((p) => p.join(',')).join(' ');
  }

  function choose(c: ControlName, at?: [number, number]) {
    const stick = stickOf(c);
    if (stick && at) {
      stickMenu = { stick: stick === 'LeftStick' ? 'Left' : 'Right', click: c, x: at[0], y: at[1] };
      return;
    }
    stickMenu = null;
    app.select(c);
  }

  function onDrawingClick(e: MouseEvent) {
    const g = (e.target as Element).closest('[data-control]');
    if (!g || !drawing) {
      stickMenu = null;
      return;
    }
    const c = g.getAttribute('data-control')!;
    const box = drawing.getBoundingClientRect();
    const x = ((e.clientX - box.left) / box.width) * vw + vx;
    const y = ((e.clientY - box.top) / box.height) * vh + vy;
    choose(c, [x, y]);
  }

  // Reflect state on the drawing's parts: mapped / selected / pressed, and stick deflection.
  $effect(() => {
    if (!drawing) return;
    const byControl = new Map(chips.map((ch) => [ch.lc.control, ch]));
    for (const g of drawing.querySelectorAll<SVGGElement>('[data-control]')) {
      const ch = byControl.get(g.getAttribute('data-control') ?? '');
      g.classList.toggle('mapped', !!ch?.mapped);
      g.classList.toggle('selected', !!ch?.selected);
      g.classList.toggle('pressed', !!ch?.pressed);
    }
    const a = app.primary?.state.axes ?? [0, 0, 0, 0];
    const caps = drawing.querySelectorAll<SVGGElement>('[data-stick]');
    for (const g of caps) {
      const left = g.getAttribute('data-stick') === 'Left';
      const [x, y] = left ? [a[0], a[1]] : [a[2], a[3]];
      const cap = g.querySelector<SVGGElement>('.stick-cap');
      cap?.setAttribute('transform', `translate(${(x * 14).toFixed(1)} ${(-y * 14).toFixed(1)})`);
    }
  });
</script>

<div class="stage">
  <div class="device" style="aspect-ratio: {vw} / {vh}">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="drawing" bind:this={drawing} onclick={onDrawingClick}>
      {@html layout.svg}
    </div>

    <svg class="leaders" viewBox="{vx} {vy} {vw} {vh}" aria-hidden="true">
      {#each chips as ch (ch.lc.control)}
        <polyline
          points={leader(ch.lc)}
          class:mapped={ch.mapped}
          class:selected={ch.selected}
        />
        <circle cx={ch.lc.anchor[0]} cy={ch.lc.anchor[1]} r="3" class:mapped={ch.mapped} class:selected={ch.selected} />
      {/each}
    </svg>

    {#each chips as ch (ch.lc.control)}
      <button
        class="chip side-{ch.lc.side}"
        class:mapped={ch.mapped}
        class:inherited={ch.inherited}
        class:selected={ch.selected}
        class:pressed={ch.pressed}
        style={chipStyle(ch.lc)}
        title={ch.summary ? `${ch.label}: ${ch.summary}` : ch.label}
        onclick={(e) => {
          e.stopPropagation();
          choose(ch.lc.control, stickOf(ch.lc.control) ? ch.lc.labelAt : undefined);
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

    {#if stickMenu}
      {@const p = pct(stickMenu.x, stickMenu.y)}
      {@const name = stickMenu.stick === 'Left' ? 'LeftStick' : 'RightStick'}
      <div class="stick-menu" style="left: {p.left}%; top: {p.top}%">
        <div class="sm-title">{t('binding.stick_choose')}</div>
        <div class="sm-grid">
          <span></span>
          <button onclick={() => choose(`${name}Up`)}>↑</button>
          <span></span>
          <button onclick={() => choose(`${name}Left`)}>←</button>
          <button class="click" onclick={() => choose(stickMenu!.click)}>{t('binding.click')}</button>
          <button onclick={() => choose(`${name}Right`)}>→</button>
          <span></span>
          <button onclick={() => choose(`${name}Down`)}>↓</button>
          <span></span>
        </div>
        <button class="sm-settings" onclick={() => app.openAnalog(stickMenu!.stick === 'Left' ? 'left' : 'right')}>
          <SlidersHorizontal size={13} />
          {t('binding.stick_settings')}
        </button>
      </div>
    {/if}
  </div>
</div>

<svelte:window onkeydown={(e) => e.key === 'Escape' && (stickMenu = null)} />

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
    font-size: 11px;
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
  .combo {
    flex: none;
    margin-left: 2px;
    color: var(--layer-1);
    font-weight: 700;
  }
  .stick-menu {
    position: absolute;
    transform: translate(-50%, 12px);
    background: var(--bg-2);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow);
    padding: 8px;
    z-index: 10;
  }
  .sm-title {
    font-size: 11px;
    color: var(--text-dim);
    margin-bottom: 6px;
    white-space: nowrap;
  }
  .sm-grid {
    display: grid;
    grid-template-columns: repeat(3, 40px);
    grid-auto-rows: 32px;
    gap: 4px;
    justify-content: center;
  }
  .sm-grid button {
    border-radius: var(--radius-s);
    background: var(--bg-3);
    border: 1px solid var(--line);
    font-size: 15px;
  }
  .sm-grid button:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .sm-grid .click {
    font-size: 10px;
  }
  .sm-settings {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    width: 100%;
    margin-top: 6px;
    padding: 5px 6px;
    border-radius: var(--radius-s);
    border: 1px solid var(--line);
    background: var(--bg-3);
    font-size: 11px;
    color: var(--text-dim);
    white-space: nowrap;
  }
  .sm-settings:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
