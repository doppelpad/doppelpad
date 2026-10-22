<script lang="ts">
  // A device drawing with a label chip per control, joined by leader lines. Clicking a
  // part or chip selects the control; sticks ask which direction (or the click) to edit.
  // Live input highlights pressed controls and moves the stick caps.
  import { Rotate3d, SlidersHorizontal } from '@lucide/svelte';
  import { DEVICE_LAYOUTS, type DeviceId, type LayoutControl } from '../../assets/devices/layouts';
  import { t } from '../../lib/i18n/index.svelte';
  import {
    DIAGONALS,
    DIRS8,
    SIDES,
    SIDE_NAMES,
    TOUCH_REGIONS,
    ZONES,
    controlShort,
    fullPullTag,
    gyroControls,
    withZone,
    zoneFamily,
    zoneParts,
    zoneTag,
  } from '../../lib/model/controls';
  import { layerRoutes, sourceOf, targetOf, type Side } from '../../lib/model/analog';
  import { hasMotion, moveStickCaps } from '../../lib/model/devices';
  import { effectiveSummary } from '../../lib/model/labels';
  import { shortcutMembers } from '../../lib/model/profile';
  import { STICK_GROUPS, stickGroupOf, type StickGroup } from '../../lib/model/stickgroups';
  import { app } from '../../lib/state/app.svelte';
  import type { ControlName } from '../../lib/types';

  let { device }: { device: DeviceId } = $props();

  const layout = $derived(DEVICE_LAYOUTS[device]);
  const [vx, vy, vw, vh] = $derived(layout.viewBox);
  const motion = $derived(hasMotion(device));

  let drawing: HTMLDivElement | undefined = $state();
  /** Direction picker of a stick, the touchpad regions or the gyro directions, at a point of the drawing. */
  let menu = $state<{ kind: 'stick' | 'touch' | 'gyro'; stick?: Side; click?: ControlName; x: number; y: number } | null>(null);
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
    const y = d.top + (((menu.y - vy) / vh) * d.height);
    menuUp = y + 12 + h > s.bottom && y - 12 - h >= s.top;
  });
  const split = $derived(!!app.profile?.touchpad?.split);
  const GESTURES = ['Tap', 'TapLeft', 'TapCenter', 'TapRight', 'MultiTap', 'ZoomIn', 'ZoomOut', 'SwipeUp', 'SwipeDown', 'SwipeLeft', 'SwipeRight', 'Swipe2Up', 'Swipe2Down', 'Swipe2Left', 'Swipe2Right'] as const;

  const ARROW: Record<string, string> = {
    Up: '↑',
    Down: '↓',
    Left: '←',
    Right: '→',
    Center: '◎',
    UpLeft: '↖',
    UpRight: '↗',
    DownLeft: '↙',
    DownRight: '↘',
  };

  const combos = $derived(app.profile ? shortcutMembers(app.profile, app.layer) : new Set<string>());

  const summaryOf = (c: ControlName) => app.profile && effectiveSummary(app.profile, app.layer, c, app.layerLabel);

  /** The side of a stick-click control (`LS` → left), else null. */
  const clickSide = (c: ControlName): Side | null => SIDE_NAMES.find((s) => SIDES[s].click === c) ?? null;

  /** What the routes on this layer make of each whole stick ("Mouse", "Flick stick"). */
  const stickRoutes = $derived.by(() => {
    const out = new Map<Side, { text: string; inherited: boolean }>();
    for (const ref of app.profile ? layerRoutes(app.profile, app.layer) : []) {
      const s = sourceOf(ref.route);
      const side = SIDE_NAMES.find((x) => SIDES[x].source === s);
      if (side && !out.has(side)) out.set(side, { text: t(`analog.routes.targets.${targetOf(ref.route)}`), inherited: ref.inherited });
    }
    return out;
  });

  function wholeStick(side: Side, g: StickGroup) {
    menu = null;
    app.setStickGroup(side, g);
  }

  /** Zone tag of a control ("H"), or "" for a control without a zone. */
  function zoneSuffix(c: ControlName): string {
    const z = zoneParts(c)?.zone;
    return z ? zoneTag(z) : '';
  }

  /**
   * Controls a chip stands for besides its own, with a tag for its summary: a stick's
   * directions, their zones and its any-direction zones; a trigger's zones.
   */
  function related(c: ControlName): { c: ControlName; tag: string }[] {
    const side = clickSide(c);
    if (side) {
      const stick = SIDES[side].stick;
      return [
        ...DIRS8.flatMap((d) => zoneFamily(stick + d).map((x) => ({ c: x, tag: ARROW[d] + zoneSuffix(x) }))),
        ...zoneFamily(stick).map((x) => ({ c: x, tag: '◎' + zoneSuffix(x) })),
      ];
    }
    if (c === 'Touchpad') return [...TOUCH_REGIONS, ...DIAGONALS].map((r) => ({ c: `Touch${r}`, tag: ARROW[r] }));
    if (zoneParts(c)) {
      return [...ZONES.map((z) => ({ c: withZone(c, z), tag: zoneTag(z) })), { c: `${c}Full`, tag: fullPullTag() }];
    }
    return [];
  }

  /** The gyro chip: summaries of the gyro directions. */
  const gyroChip = $derived.by(() => {
    const parts: string[] = [];
    let selected = false;
    for (const c of gyroControls()) {
      const s = summaryOf(c);
      if (s) parts.push(`${ARROW[c.slice(4)]} ${s.text}`);
      if (app.selectedKey === c) selected = true;
    }
    return { summary: parts.join('  '), selected };
  });

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
      let summary = own?.text ?? '';
      let mapped = !!own;
      let selected = app.selectedKey === c;
      // app.pressed is also what press-to-select reacts to, so highlight and selection agree.
      let isPressed = app.pressed.has(c);
      // Stick chips also show the directions and zones, trigger chips the zones.
      const parts: string[] = [];
      // A whole-stick route comes first: it is what the stick mostly does.
      const side = clickSide(c);
      const route = side ? stickRoutes.get(side) : undefined;
      if (route) parts.push(route.text);
      if (side && app.stick === side) selected = true;
      for (const r of related(c)) {
        const s = summaryOf(r.c);
        if (s) parts.push(`${r.tag} ${s.text}`);
        if (app.selectedKey === r.c) selected = true;
        if (app.pressed.has(r.c)) isPressed = true;
      }
      if (parts.length) {
        summary = [own ? `● ${own.text}` : '', ...parts].filter(Boolean).join('  ');
        mapped = true;
      }
      return {
        lc,
        // The touchpad chip stands for its click and its regions.
        label: c === 'Touchpad' ? t('views.groups.touchpad') : controlShort(c, app.family),
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
    const side = clickSide(c);
    if (side && at) {
      menu = { kind: 'stick', stick: side, click: c, x: at[0], y: at[1] };
      return;
    }
    if (c === 'Touchpad' && at) {
      menu = { kind: 'touch', click: c, x: at[0], y: at[1] };
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
    moveStickCaps(drawing, app.primary?.state.axes ?? [0, 0, 0, 0]);
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
      {@const m = menu}
      {@const p = pct(m.x, m.y)}
      {@const prefix = m.kind === 'stick' ? SIDES[m.stick!].stick : m.kind === 'touch' ? (split ? `Trackpad${half}` : 'Touch') : 'Gyro'}
      {@const diag = m.kind === 'touch' || (m.kind === 'stick' && !!app.profile?.sticks?.[m.stick!]?.diagonals)}
      <div class="stick-menu" class:up={menuUp} style="left: {p.left}%; top: {p.top}%" bind:this={menuEl}>
        <div class="sm-title">{t(`binding.${m.kind}_choose`)}</div>
        {#if m.kind === 'touch' && split}
          <div class="sm-lean">
            {#each ['Left', 'Right'] as const as h (h)}
              <button class:on={half === h} onclick={() => (half = h)}>{t(`binding.half_${h.toLowerCase()}`)}</button>
            {/each}
          </div>
        {/if}
        {#snippet corner(d: string, arrow: string)}
          {#if diag}<button onclick={() => choose(`${prefix}${d}`)}>{arrow}</button>{:else}<span></span>{/if}
        {/snippet}
        <div class="sm-grid">
          {@render corner('UpLeft', '↖')}
          <button onclick={() => choose(`${prefix}Up`)}>↑</button>
          {@render corner('UpRight', '↗')}
          <button onclick={() => choose(`${prefix}Left`)}>←</button>
          {#if m.kind === 'gyro'}
            <span></span>
          {:else if m.kind === 'touch'}
            <button title={t('controls.touch.center')} onclick={() => choose(`${prefix}Center`)}>◎</button>
          {:else}
            <button class="click" onclick={() => choose(m.click!)}>{t('binding.click')}</button>
          {/if}
          <button onclick={() => choose(`${prefix}Right`)}>→</button>
          {@render corner('DownLeft', '↙')}
          <button onclick={() => choose(`${prefix}Down`)}>↓</button>
          {@render corner('DownRight', '↘')}
        </div>
        {#if m.kind === 'stick'}
          {@const side = m.stick!}
          {@const group = app.profile ? stickGroupOf(app.profile, app.layer, side) : null}
          <button class="sm-any" onclick={() => choose(`${prefix}Low`)}>{t('zones.any_direction_button')}</button>
          <div class="sm-sub">{t('binding.whole_stick')}</div>
          <div class="sm-lean">
            {#each STICK_GROUPS as g (g)}
              <button class:on={group === g} disabled={app.readOnly} onclick={() => wholeStick(side, g)}>
                {t(`binding.stick_groups.${g}`)}
              </button>
            {/each}
          </div>
          <button class="sm-settings" onclick={() => app.openAnalog(m.stick!)}>
            <SlidersHorizontal size={13} />
            {t('binding.stick_settings')}
          </button>
        {:else if m.kind === 'touch'}
          {#if split}
            <div class="sm-lean">
              <button onclick={() => choose(`${prefix}Tap`)}>{t('binding.trackpad_tap')}</button>
              <button onclick={() => choose(`${prefix}Click`)}>{t('binding.click')}</button>
            </div>
          {:else}
            <button class="sm-any" onclick={() => choose('Touchpad')}>{t('controls.Touchpad')}</button>
          {/if}
          <div class="sm-sub">{t('binding.gestures')}</div>
          <div class="sm-gestures">
            {#each GESTURES as g (g)}
              <button onclick={() => choose(`Touch${g}`)}>{t(`controls.gesture.${g}`)}</button>
            {/each}
          </div>
          <button class="sm-settings" onclick={() => app.openAnalog('touchpad')}>
            <SlidersHorizontal size={13} />
            {t('binding.touchpad_controls_settings')}
          </button>
          <button class="sm-settings" onclick={() => app.openAnalog('routes')}>
            <SlidersHorizontal size={13} />
            {t('binding.touch_settings')}
          </button>
        {:else}
          <div class="sm-lean">
            <button onclick={() => choose('GyroLeanLeft')}>{t('binding.lean_left')}</button>
            <button onclick={() => choose('GyroLeanRight')}>{t('binding.lean_right')}</button>
          </div>
          <button class="sm-any" onclick={() => choose('GyroLow')}>{t('zones.any_direction_button')}</button>
          <button class="sm-settings" onclick={() => app.openAnalog('gyro')}>
            <SlidersHorizontal size={13} />
            {t('binding.gyro_controls_settings')}
          </button>
          <button class="sm-settings" onclick={() => app.openAnalog('routes')}>
            <SlidersHorizontal size={13} />
            {t('binding.gyro_settings')}
          </button>
        {/if}
      </div>
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
  .stick-menu.up {
    transform: translate(-50%, calc(-100% - 12px));
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
  .sm-any,
  .sm-settings {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    width: 100%;
    margin-top: 6px;
    padding: 5px 8px;
    border-radius: var(--radius-s);
    background: var(--bg-3);
    border: 1px solid var(--line);
    font-size: 11px;
    white-space: nowrap;
  }
  .sm-settings {
    color: var(--text-dim);
  }
  .sm-any:hover,
  .sm-settings:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .sm-lean {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
    margin-top: 6px;
  }
  .sm-lean button {
    padding: 5px 6px;
    border-radius: var(--radius-s);
    background: var(--bg-3);
    border: 1px solid var(--line);
    font-size: 11px;
    white-space: nowrap;
  }
  .sm-lean button:hover,
  .sm-lean button.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .sm-sub {
    margin-top: 8px;
    font-size: 10.5px;
    color: var(--text-faint);
    text-align: center;
  }
  .sm-gestures {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 3px;
    margin-top: 4px;
    max-height: 116px;
    overflow-y: auto;
  }
  .sm-gestures button {
    padding: 3px 6px;
    border-radius: var(--radius-s);
    background: var(--bg-3);
    border: 1px solid var(--line);
    font-size: 10.5px;
    white-space: nowrap;
  }
  .sm-gestures button:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
