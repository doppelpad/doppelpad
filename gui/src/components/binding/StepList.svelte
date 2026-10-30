<script lang="ts">
  // An ordered list of action steps (a macro, a radial menu sector): per step how it presses
  // (tap, down, up, or held for a time), its parameters, pauses and breaks, and buttons to
  // move or remove it; below it the buttons that add steps (an action from the picker, a
  // pause, a rumble, a break). Narrow places (the side panel) get rows that wrap.
  import type { Snippet } from 'svelte';
  import { ArrowDown, ArrowUp, Plus, SeparatorHorizontal, Timer, Vibrate, X } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { hasPress, holdFor } from '../../lib/model/actions';
  import { actionLabel } from '../../lib/model/labels';
  import { parseNonNegInt } from '../../lib/model/numbers';
  import { tip } from '../../lib/tooltip.svelte';
  import type { Action, Press } from '../../lib/types';
  import Select from '../ui/Select.svelte';
  import ActionParams from './ActionParams.svelte';
  import ActionPicker from './ActionPicker.svelte';

  /** Steps the add buttons offer besides an action from the picker. */
  type Extra = 'pause' | 'rumble' | 'break';

  let {
    steps,
    disabled = false,
    narrow = false,
    empty,
    adds = ['pause'],
    busy = false,
    before,
    after,
    onchange,
  }: {
    steps: Action[];
    /** Read-only: nothing can be changed or added. */
    disabled?: boolean;
    /** Rows wrap, for the side panel. */
    narrow?: boolean;
    /** Shown while there are no steps. */
    empty: string;
    /** The steps the add buttons offer besides an action. */
    adds?: Extra[];
    /** Something else adds steps now (a recording): the add buttons wait. */
    busy?: boolean;
    /** More buttons in the row, before and after the add buttons. */
    before?: Snippet;
    after?: Snippet;
    onchange: (steps: Action[]) => void;
  } = $props();

  /** The action picker is open. */
  let picking = $state(false);

  const EXTRAS: Record<Extra, { step: Action; icon: typeof Timer; label: string }> = {
    pause: { step: { pause: 100 }, icon: Timer, label: 'macro.add_pause' },
    rumble: { step: { rumble: 200 }, icon: Vibrate, label: 'macro.add_rumble' },
    break: { step: { break: true }, icon: SeparatorHorizontal, label: 'macro.add_break' },
  };

  const PRESSES = $derived([
    { value: 'auto', label: t('macro.step_auto') },
    { value: 'tap', label: t('macro.step_tap') },
    { value: 'down', label: t('macro.step_down') },
    { value: 'up', label: t('macro.step_up') },
    { separator: true as const },
    { value: 'hold', label: t('macro.step_hold'), icon: Timer },
  ]);

  function move(i: number, d: -1 | 1) {
    const next = [...steps];
    const j = i + d;
    if (j < 0 || j >= next.length) return;
    [next[i], next[j]] = [next[j]!, next[i]!];
    onchange(next);
  }

  function setPress(i: number, press: Press | 'hold') {
    if (press === 'hold') onchange(holdFor(steps, i));
    else onchange(steps.map((s, j) => (j === i && hasPress(s) ? { ...s, action: press } : s)));
  }

  function setPause(i: number, v: string) {
    const n = parseNonNegInt(v);
    if (n !== null) onchange(steps.map((s, j) => (j === i ? { pause: n } : s)));
  }
</script>

<div class="steps" class:narrow>
  {#if steps.length === 0}
    <p class="empty">{empty}</p>
  {/if}
  {#each steps as s, i (i)}
    <div class="step" class:pause={'pause' in s} class:brk={'break' in s}>
      <span class="idx">{i + 1}</span>
      {#if 'pause' in s}
        <Timer size={14} />
        <input
          class="pause-in"
          type="number"
          min="0"
          step="10"
          value={s.pause}
          aria-label={t('macro.add_pause')}
          {disabled}
          onchange={(e) => setPause(i, e.currentTarget.value)}
        />
        <span class="dim">{t('common.ms')}</span>
      {:else if 'break' in s}
        <SeparatorHorizontal size={14} />
        <span class="label">{t('macro.break')}</span>
        {#if !narrow}<span class="dim">{t('macro.break_hint')}</span>{/if}
      {:else}
        <span class="label">{actionLabel({ ...s, action: undefined } as Action)}</span>
        {#if hasPress(s)}
          <Select
            size="s"
            value={('action' in s && s.action) || 'auto'}
            label={t('macro.press_label')}
            {disabled}
            options={PRESSES}
            onchange={(v) => setPress(i, v as Press | 'hold')}
          />
        {/if}
        <ActionParams action={s} {disabled} onchange={(a) => onchange(steps.map((x, j) => (j === i ? a : x)))} />
      {/if}
      {#if !disabled}
        <span class="tools">
          <button class="icon-btn" use:tip={t('macro.move_up')} disabled={i === 0} onclick={() => move(i, -1)}><ArrowUp size={14} /></button
          >
          <button class="icon-btn" use:tip={t('macro.move_down')} disabled={i === steps.length - 1} onclick={() => move(i, 1)}
            ><ArrowDown size={14} /></button
          >
          <button class="icon-btn" use:tip={t('common.remove')} onclick={() => onchange(steps.filter((_, j) => j !== i))}
            ><X size={14} /></button
          >
        </span>
      {/if}
    </div>
  {/each}
</div>

{#if !disabled}
  <div class="add">
    {@render before?.()}
    <button class="btn" class:on={picking && !busy} aria-expanded={picking && !busy} disabled={busy} onclick={() => (picking = !picking)}>
      <Plus size={14} />
      {t('binding.add_action')}
    </button>
    {#each adds as extra (extra)}
      {@const x = EXTRAS[extra]}
      <button class="btn" disabled={busy} onclick={() => onchange([...steps, x.step])}><x.icon size={14} /> {t(x.label)}</button>
    {/each}
    {@render after?.()}
  </div>
  {#if picking && !busy}
    <div class="picker" class:narrow>
      <ActionPicker onpick={(a) => onchange([...steps, a])} onclose={() => (picking = false)} />
    </div>
  {/if}
{/if}

<style>
  .add {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .add .on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .picker:not(.narrow) {
    max-width: 420px;
  }
  .steps {
    display: flex;
    flex-direction: column;
    gap: 4px;
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    padding: 6px;
    background: var(--bg-1);
    min-height: 60px;
  }
  .empty {
    color: var(--text-faint);
    margin: 12px;
  }
  .narrow .empty {
    margin: 8px;
    font-size: var(--fs-s);
  }
  .step {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 5px 8px;
    border-radius: var(--radius-s);
    background: var(--bg-2);
  }
  .narrow .step {
    flex-wrap: wrap;
    gap: 6px 8px;
  }
  .step.brk {
    background: transparent;
    border: 1px solid var(--accent);
    color: var(--text-dim);
  }
  .step.pause {
    background: transparent;
    border: 1px dashed var(--line);
    color: var(--text-dim);
  }
  .idx {
    width: 22px;
    color: var(--text-faint);
    font-size: var(--fs-xs);
    text-align: right;
  }
  .narrow .idx {
    width: auto;
    min-width: 12px;
  }
  .label {
    font-weight: 500;
    min-width: 120px;
  }
  .narrow .label {
    min-width: 0;
  }
  .pause-in {
    width: 80px;
    padding: 3px 6px;
  }
  .dim {
    color: var(--text-faint);
  }
  .tools {
    margin-left: auto;
    display: inline-flex;
  }
  .tools .icon-btn {
    width: 26px;
    height: 26px;
  }
  .tools .icon-btn:disabled {
    opacity: 0.3;
  }
</style>
