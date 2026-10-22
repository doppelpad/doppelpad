<script lang="ts">
  // Edit one mapping as a macro: run mode, repeat settings and an ordered list of steps.
  // Steps can be picked or recorded from the keyboard or the controller (presses and
  // releases with the pauses between), and break steps cut the sequence into segments.
  import { ArrowLeft, ArrowUp, ArrowDown, X, Circle, Square, Plus, Timer, Vibrate, Gamepad2, SeparatorHorizontal } from '@lucide/svelte';
  import { onDestroy } from 'svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { layerColor } from '../../lib/theme/layers';
  import { comboLabel } from '../../lib/model/labels';
  import { keyFromEvent } from '../../lib/model/keymap';
  import { recordedSteps } from '../../lib/model/actions';
  import { actionLabel, modeOf, segmentCount } from '../../lib/model/profile';
  import { app } from '../../lib/state/app.svelte';
  import type { Action, ControlName, Mapping, Mode, Press } from '../../lib/types';
  import ActionParams from '../binding/ActionParams.svelte';
  import ActionPicker from '../binding/ActionPicker.svelte';

  const target = $derived(app.macroTarget);
  const at = $derived<{ own?: Mapping; inherited?: Mapping }>(
    target ? app.mappingAt(target.layer, target.trigger, target.activator) : {},
  );
  const m = $derived<Mapping | undefined>(at.own ?? at.inherited);
  const steps = $derived<Action[]>(m?.actions ?? []);
  const mode = $derived<Mode>(m ? modeOf(m) : 'hold');
  const timing = $derived({ ...app.catalog?.defaults.timing, ...(app.profile?.timing ?? {}) });
  const keyDelay = $derived(m?.key_delay_ms ?? timing.key_delay_ms ?? 0);

  let picking = $state(false);
  let recording = $state(false);
  let lastEventAt = 0;

  const MODES: { id: Mode; key: string }[] = [
    { id: 'hold', key: 'mode_hold' },
    { id: 'once', key: 'mode_once' },
    { id: 'repeat', key: 'mode_repeat' },
    { id: 'while_held', key: 'mode_while' },
  ];

  function edit(change: (m: Mapping) => void) {
    if (target) app.editMapping(target.layer, target.trigger, target.activator, change);
  }

  function setSteps(next: Action[]) {
    edit((x) => (x.actions = next));
  }

  function move(i: number, d: -1 | 1) {
    const next = [...steps];
    const j = i + d;
    if (j < 0 || j >= next.length) return;
    [next[i], next[j]] = [next[j], next[i]];
    setSteps(next);
  }

  function setPress(i: number, action: Press) {
    const next = steps.map((s, j) => (j === i ? ({ ...s, action } as Action) : s));
    setSteps(next);
  }

  function setPause(i: number, v: string) {
    const n = parseInt(v, 10);
    if (!(n >= 0)) return;
    setSteps(steps.map((s, j) => (j === i ? { pause: n } : s)));
  }

  function hasPress(a: Action): boolean {
    return 'key' in a || 'mouse' in a || 'pad' in a || 'stick' in a || 'trigger' in a;
  }

  function num(v: string): number | undefined {
    const n = parseInt(v, 10);
    return Number.isFinite(n) && n >= 0 ? n : undefined;
  }

  /** Rough run time: explicit pauses plus the delay the engine inserts between steps. */
  const total = $derived(
    steps.reduce((sum, s) => sum + ('pause' in s ? s.pause : 'rumble' in s ? 0 : keyDelay), 0),
  );

  // ----- recording -------------------------------------------------------------------------

  function record(e: KeyboardEvent, action: Press) {
    if (!recording || e.repeat) return;
    const key = keyFromEvent(e);
    if (!key) return;
    e.preventDefault();
    e.stopPropagation();
    const now = performance.now();
    const add: Action[] = [];
    if (lastEventAt > 0) {
      const gap = Math.round(now - lastEventAt);
      if (gap > keyDelay + 5) add.push({ pause: gap });
    }
    lastEventAt = now;
    add.push({ key, action });
    setSteps([...steps, ...add]);
  }

  function toggleRecording() {
    recording = !recording;
    padRecording = false;
    lastEventAt = 0;
    picking = false;
  }

  // Controller recording follows the live snapshots of the controller in use.
  let padRecording = $state(false);
  let padBefore = new Set<ControlName>();
  let releasePresses = () => {};

  function togglePadRecording() {
    padRecording = !padRecording;
    recording = false;
    picking = false;
    padBefore = new Set(app.pressed);
    lastEventAt = 0;
    // Presses must not select controls in the drawing meanwhile.
    if (padRecording) releasePresses = app.capturePresses();
    else releasePresses();
  }

  $effect(() => {
    const now = app.pressed;
    if (!padRecording) return;
    const at = performance.now();
    const add = recordedSteps(padBefore, now, lastEventAt > 0 ? at - lastEventAt : null, keyDelay);
    padBefore = new Set(now);
    if (add.length === 0) return;
    lastEventAt = at;
    setSteps([...steps, ...add]);
  });

  function replace(i: number, a: Action) {
    setSteps(steps.map((s, j) => (j === i ? a : s)));
  }

  const segments = $derived(segmentCount(steps));

  onDestroy(() => {
    recording = false;
    if (padRecording) releasePresses();
  });
</script>

<svelte:window onkeydowncapture={(e) => record(e, 'down')} onkeyupcapture={(e) => record(e, 'up')} />

{#if target}
  <div class="macro">
    <div class="head">
      <button class="btn" onclick={() => (app.view = 'front')}><ArrowLeft size={14} /> {t('macro.back')}</button>
      <h2>
        {t('macro.title', {
          control: comboLabel(target.trigger, app.family),
        })}
      </h2>
      <span class="layer" style="--c: {layerColor(target.layer)}">{app.layerLabel(target.layer)}</span>
    </div>

    <div class="modes">
      {#each MODES as md (md.id)}
        <button class:active={mode === md.id} disabled={app.readOnly} onclick={() => edit((x) => (x.mode = md.id))}>
          {t(`macro.${md.key}`)}
        </button>
      {/each}
    </div>

    <div class="params">
      {#if mode === 'repeat'}
        <label>
          {t('macro.times')}
          <input type="number" min="1" value={m?.repeat ?? 1} disabled={app.readOnly} onchange={(e) => edit((x) => (x.repeat = num(e.currentTarget.value)))} />
        </label>
      {/if}
      {#if mode === 'repeat' || mode === 'while_held'}
        <label>
          {t('macro.pause_between')}
          <input type="number" min="0" step="10" value={m?.repeat_pause_ms ?? ''} placeholder="0" disabled={app.readOnly} onchange={(e) => edit((x) => (x.repeat_pause_ms = num(e.currentTarget.value)))} />
          {t('common.ms')}
        </label>
        <label title={t('binding.random_hint')}>
          {t('binding.random')}
          <input type="number" min="0" max="10000" step="10" value={m?.random_ms ?? ''} placeholder="0" disabled={app.readOnly} onchange={(e) => edit((x) => (x.random_ms = num(e.currentTarget.value) || undefined))} />
          {t('common.ms')}
        </label>
      {/if}
      <label>
        {t('macro.key_delay')}
        <input type="number" min="0" step="5" value={m?.key_delay_ms ?? ''} placeholder={String(timing.key_delay_ms ?? '')} disabled={app.readOnly} onchange={(e) => edit((x) => (x.key_delay_ms = num(e.currentTarget.value)))} />
        {t('common.ms')}
      </label>
      <span class="total">{t('macro.total', { ms: total })}</span>
    </div>

    <div class="steps">
      {#if steps.length === 0}
        <p class="empty">{t('macro.empty')}</p>
      {/if}
      {#each steps as s, i (i)}
        <div class="step" class:pause={'pause' in s} class:brk={'break' in s}>
          <span class="idx">{i + 1}</span>
          {#if 'pause' in s}
            <Timer size={14} />
            <input class="pause-in" type="number" min="0" step="10" value={s.pause} disabled={app.readOnly} onchange={(e) => setPause(i, e.currentTarget.value)} />
            <span class="dim">{t('common.ms')}</span>
          {:else if 'break' in s}
            <SeparatorHorizontal size={14} />
            <span class="label">{t('macro.break')}</span>
            <span class="dim">{t('macro.break_hint')}</span>
          {:else}
            <span class="label">{actionLabel({ ...s, action: undefined } as Action)}</span>
            {#if hasPress(s)}
              <select
                value={('action' in s && s.action) || 'auto'}
                disabled={app.readOnly}
                onchange={(e) => setPress(i, e.currentTarget.value as Press)}
              >
                <option value="auto">{t('macro.step_auto')}</option>
                <option value="tap">{t('macro.step_tap')}</option>
                <option value="down">{t('macro.step_down')}</option>
                <option value="up">{t('macro.step_up')}</option>
              </select>
            {/if}
            <ActionParams action={s} disabled={app.readOnly} onchange={(a) => replace(i, a)} />
          {/if}
          {#if !app.readOnly}
            <span class="tools">
              <button class="icon-btn" disabled={i === 0} onclick={() => move(i, -1)}><ArrowUp size={14} /></button>
              <button class="icon-btn" disabled={i === steps.length - 1} onclick={() => move(i, 1)}><ArrowDown size={14} /></button>
              <button class="icon-btn" onclick={() => setSteps(steps.filter((_, j) => j !== i))}><X size={14} /></button>
            </span>
          {/if}
        </div>
      {/each}
    </div>

    {#if !app.readOnly}
      <div class="add">
        <button class="btn rec" class:on={recording} onclick={toggleRecording} disabled={padRecording}>
          {#if recording}<Square size={13} /> {t('macro.stop')}{:else}<Circle size={13} /> {t('macro.record')}{/if}
        </button>
        <button class="btn rec" class:on={padRecording} onclick={togglePadRecording} disabled={recording || !app.primary}>
          {#if padRecording}<Square size={13} /> {t('macro.stop')}{:else}<Gamepad2 size={13} /> {t('macro.record_pad')}{/if}
        </button>
        <button class="btn" onclick={() => (picking = !picking)} disabled={recording || padRecording}><Plus size={14} /> {t('binding.add_action')}</button>
        <button class="btn" onclick={() => setSteps([...steps, { pause: 100 }])} disabled={recording || padRecording}><Timer size={14} /> {t('macro.add_pause')}</button>
        <button class="btn" onclick={() => setSteps([...steps, { rumble: 200 }])} disabled={recording || padRecording}><Vibrate size={14} /> {t('macro.add_rumble')}</button>
        <button class="btn" onclick={() => setSteps([...steps, { break: true }])} disabled={recording || padRecording}><SeparatorHorizontal size={14} /> {t('macro.add_break')}</button>
      </div>
      {#if recording}<p class="hint">{t('macro.recording')}</p>{/if}
      {#if padRecording}<p class="hint">{t('macro.recording_pad')}</p>{/if}
      {#if segments > 1}
        <p class="parts">
          {t('macro.segments', { n: segments })}
          {#if mode === 'repeat' || mode === 'while_held'}{t('macro.segments_no_repeat')}{/if}
        </p>
      {/if}
      {#if picking}
        <div class="picker"><ActionPicker onpick={(a) => setSteps([...steps, a])} onclose={() => (picking = false)} /></div>
      {/if}
    {/if}
  </div>
{/if}

<style>
  .macro {
    max-width: 760px;
    margin: 0 auto;
    padding: 22px 24px 40px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }
  .layer {
    font-size: 11px;
    padding: 1px 8px;
    border-radius: 99px;
    border: 1px solid var(--c);
    color: var(--c);
  }
  .modes {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    overflow: hidden;
  }
  .modes button {
    padding: 9px 6px;
    color: var(--text-dim);
  }
  .modes button + button {
    border-left: 1px solid var(--line);
  }
  .modes button.active {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .params {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 16px;
    color: var(--text-dim);
  }
  .params label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .params input {
    width: 76px;
  }
  .total {
    margin-left: auto;
    color: var(--text-faint);
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
  .step {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 5px 8px;
    border-radius: var(--radius-s);
    background: var(--bg-2);
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
    font-size: 11px;
    text-align: right;
  }
  .label {
    font-weight: 500;
    min-width: 120px;
  }
  .pause-in {
    width: 80px;
    padding: 3px 6px;
  }
  .dim {
    color: var(--text-faint);
  }
  .step select {
    padding: 3px 6px;
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
  .add {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .rec :global(svg) {
    color: var(--danger);
    fill: var(--danger);
  }
  .rec.on {
    border-color: var(--danger);
    color: var(--danger);
  }
  .parts {
    margin: 0;
    color: var(--text-dim);
    font-size: 12px;
  }
  .hint {
    margin: 0;
    color: var(--danger);
  }
  .picker {
    max-width: 420px;
  }
</style>
