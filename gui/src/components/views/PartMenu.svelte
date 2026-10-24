<script module lang="ts">
  import type { Side } from '../../lib/model/analog';

  /** Which part menu is open, at a point of the drawing (drawing coordinates). */
  export type PartMenuAt = { kind: 'stick'; side: Side; x: number; y: number } | { kind: 'touch' | 'gyro'; x: number; y: number };
</script>

<script lang="ts">
  // The popover that asks which part of a stick, the touchpad or the gyro to edit: directions
  // (diagonals when the stick has them), the click or centre, the zones, the whole stick, the
  // touchpad's halves and gestures, and links to their settings.
  import { ClipboardPaste, Copy, Eraser, SlidersHorizontal } from '@lucide/svelte';
  import type { GroupId } from '../../lib/model/clipboard';
  import { groupItems } from '../binding/controlMenu';
  import { t } from '../../lib/i18n/index.svelte';
  import { SIDES, TOUCH_GESTURES, controlLabel } from '../../lib/model/controls';
  import { STICK_GROUPS, stickGroupOf } from '../../lib/model/stickgroups';
  import { app } from '../../lib/state/app.svelte';
  import type { ControlName } from '../../lib/types';
  import Segmented from '../ui/Segmented.svelte';
  import { tip } from '../../lib/tooltip.svelte';

  let {
    menu,
    left,
    top,
    up,
    half = $bindable(),
    el = $bindable(),
    onchoose,
    onclose,
  }: {
    menu: PartMenuAt;
    /** Position in percent of the drawing. */
    left: number;
    top: number;
    /** Opens upward. */
    up: boolean;
    /** On a split touchpad, which half the menu shows. */
    half: 'Left' | 'Right';
    el?: HTMLDivElement;
    /** Edit this control. */
    onchoose: (c: ControlName) => void;
    onclose: () => void;
  } = $props();

  const split = $derived(!!app.profile?.touchpad?.split);
  const prefix = $derived(
    menu.kind === 'stick' ? SIDES[menu.side].stick : menu.kind === 'touch' ? (split ? `Trackpad${half}` : 'Touch') : 'Gyro',
  );
  const diagonals = $derived(menu.kind === 'touch' || (menu.kind === 'stick' && !!app.profile?.sticks?.[menu.side]?.diagonals));

  /** The direction grid, row by row; the middle is the stick's click or the touchpad's centre. */
  const GRID = [
    ['UpLeft', '↖'],
    ['Up', '↑'],
    ['UpRight', '↗'],
    ['Left', '←'],
    ['centre', ''],
    ['Right', '→'],
    ['DownLeft', '↙'],
    ['Down', '↓'],
    ['DownRight', '↘'],
  ] as const;
  const CORNERS = new Set(['UpLeft', 'UpRight', 'DownLeft', 'DownRight']);
  /** Icons of the copy, paste and clear buttons for all directions (in `groupItems` order). */
  const CLIP_ICONS = [Copy, ClipboardPaste, Eraser];
  const HALVES = [
    { value: 'Left', key: 'half_left' },
    { value: 'Right', key: 'half_right' },
  ] as const;
</script>

<div class="stick-menu" class:up style="left: {left}%; top: {top}%" bind:this={el} data-popover>
  <div class="sm-title">{t(`binding.${menu.kind}_choose`)}</div>
  {#if menu.kind === 'touch' && split}
    <Segmented
      variant="chips"
      size="s"
      class="sm-pick"
      label={t('binding.touch_choose')}
      options={HALVES.map((h) => ({ value: h.value, label: t(`binding.${h.key}`) }))}
      value={half}
      onchange={(h) => (half = h)}
    />
  {/if}
  <div class="sm-grid">
    {#each GRID as [d, arrow] (d)}
      {#if d === 'centre'}
        {#if menu.kind === 'stick'}
          {@const click = SIDES[menu.side].click}
          <button class="sm-btn click" onclick={() => onchoose(click)}>{t('binding.click')}</button>
        {:else if menu.kind === 'touch'}
          <button class="sm-btn" use:tip={t('controls.touch.center')} onclick={() => onchoose(`${prefix}Center`)}>◎</button>
        {:else}
          <span></span>
        {/if}
      {:else if CORNERS.has(d) && !diagonals}
        <span></span>
      {:else}
        <button class="sm-btn" use:tip={controlLabel(`${prefix}${d}`, app.family)} onclick={() => onchoose(`${prefix}${d}`)}>{arrow}</button
        >
      {/if}
    {/each}
  </div>
  {#if menu.kind === 'stick'}
    {@const side = menu.side}
    {@const group = app.profile ? stickGroupOf(app.profile, app.layer, side) : null}
    <button class="sm-btn sm-any" onclick={() => onchoose(`${prefix}Low`)}>{t('zones.any_direction_button')}</button>
    <div class="sm-sub">{t('binding.whole_stick')}</div>
    <Segmented
      variant="chips"
      size="s"
      class="sm-pick"
      label={t('binding.whole_stick')}
      options={STICK_GROUPS.map((g) => ({ value: g, label: t(`binding.stick_groups.${g}`) }))}
      value={group}
      disabled={app.readOnly}
      onchange={(g) => {
        if (!g) return;
        onclose();
        app.setStickGroup(side, g);
      }}
    />
    <button class="sm-btn sm-settings" onclick={() => app.openAnalog(side)}>
      <SlidersHorizontal size={13} />
      {t('binding.stick_settings')}
    </button>
  {:else if menu.kind === 'touch'}
    {#if split}
      <div class="sm-lean">
        <button class="sm-btn" onclick={() => onchoose(`${prefix}Tap`)}>{t('binding.trackpad_tap')}</button>
        <button class="sm-btn" onclick={() => onchoose(`${prefix}Click`)}>{t('binding.click')}</button>
      </div>
    {:else}
      <button class="sm-btn sm-any" onclick={() => onchoose('Touchpad')}>{t('controls.Touchpad')}</button>
    {/if}
    <div class="sm-sub">{t('binding.gestures')}</div>
    <div class="sm-gestures">
      {#each TOUCH_GESTURES as g (g)}
        <button class="sm-btn" onclick={() => onchoose(`Touch${g}`)}>{t(`controls.gesture.${g}`)}</button>
      {/each}
    </div>
    <button class="sm-btn sm-settings" onclick={() => app.openAnalog('touchpad')}>
      <SlidersHorizontal size={13} />
      {t('binding.touchpad_controls_settings')}
    </button>
    <button class="sm-btn sm-settings" onclick={() => app.openAnalog('routes')}>
      <SlidersHorizontal size={13} />
      {t('binding.touch_settings')}
    </button>
  {:else}
    <div class="sm-lean">
      <button class="sm-btn" onclick={() => onchoose('GyroLeanLeft')}>{t('binding.lean_left')}</button>
      <button class="sm-btn" onclick={() => onchoose('GyroLeanRight')}>{t('binding.lean_right')}</button>
    </div>
    <button class="sm-btn sm-any" onclick={() => onchoose('GyroLow')}>{t('zones.any_direction_button')}</button>
    <button class="sm-btn sm-settings" onclick={() => app.openAnalog('gyro')}>
      <SlidersHorizontal size={13} />
      {t('binding.gyro_controls_settings')}
    </button>
    <button class="sm-btn sm-settings" onclick={() => app.openAnalog('routes')}>
      <SlidersHorizontal size={13} />
      {t('binding.gyro_settings')}
    </button>
  {/if}
  <div class="sm-clip">
    {#each groupItems(prefix as GroupId) as item, i (i)}
      {@const Icon = CLIP_ICONS[i]}
      <button
        class="sm-btn"
        class:danger={item.danger}
        use:tip={item.label}
        aria-label={item.label}
        disabled={item.disabled}
        onclick={() => {
          onclose();
          item.onclick?.();
        }}
      >
        <Icon size={13} />
      </button>
    {/each}
  </div>
</div>

<style>
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
    font-size: var(--fs-xs);
    color: var(--text-dim);
    margin-bottom: 6px;
    white-space: nowrap;
  }
  .sm-btn {
    border-radius: var(--radius-s);
    background: var(--bg-3);
    border: 1px solid var(--line);
  }
  .sm-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .sm-grid {
    display: grid;
    grid-template-columns: repeat(3, 40px);
    grid-auto-rows: 32px;
    gap: 4px;
    justify-content: center;
  }
  .sm-grid .sm-btn {
    font-size: 15px;
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
    font-size: var(--fs-xs);
    white-space: nowrap;
  }
  .sm-settings {
    color: var(--text-dim);
  }
  .sm-lean {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
    margin-top: 6px;
  }
  .sm-lean .sm-btn {
    padding: 5px 6px;
    font-size: var(--fs-xs);
    white-space: nowrap;
  }
  .stick-menu :global(.sm-pick) {
    margin-top: 6px;
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
  .sm-gestures .sm-btn {
    padding: 3px 6px;
    font-size: 10.5px;
    white-space: nowrap;
  }
  .sm-clip {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 4px;
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
  }
  .sm-clip .sm-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 5px 0;
    color: var(--text-dim);
  }
  .sm-clip .sm-btn.danger:hover:not(:disabled) {
    border-color: var(--danger);
    color: var(--danger);
  }
  .sm-clip .sm-btn:disabled {
    opacity: 0.35;
    cursor: default;
    border-color: var(--line);
    color: var(--text-dim);
  }
</style>
