<script lang="ts">
  // "Identify buttons" for a generic pad: asks for one control after another, records the raw
  // input that moves, and saves the result as the layout of this controller model.
  import { X } from '@lucide/svelte';
  import { untrack } from 'svelte';
  import { api, errorText } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { controlLabel } from '../lib/model/controls';
  import { STEPS, buildLayout, detect, modelKey, settled, type Hit, type Step } from '../lib/model/identify';
  import { app } from '../lib/state/app.svelte';
  import type { RawInput } from '../lib/types';

  let { deviceId, onclose }: { deviceId: string; onclose: () => void } = $props();

  const device = $derived(app.live?.devices.find((d) => d.id === deviceId) ?? null);
  const raw = $derived(device?.raw ?? null);
  const key = $derived(device ? modelKey(device.vendor_id, device.product_id) : '');
  const known = $derived(app.live?.layouts.includes(key) ?? false);

  let phase = $state<'intro' | 'step' | 'done'>('intro');
  let rest = $state<RawInput | null>(null);
  let index = $state(0);
  let hits = $state<(Hit | null)[]>([]);
  /** A hit was recorded: wait until everything rests before the next step. */
  let waiting = $state(false);

  function start() {
    if (!raw) return;
    rest = $state.snapshot(raw);
    hits = STEPS.map(() => null);
    index = 0;
    waiting = false;
    phase = 'step';
  }

  function next() {
    waiting = false;
    if (index + 1 >= STEPS.length) phase = 'done';
    else index += 1;
  }

  function back() {
    if (index === 0) return;
    index -= 1;
    hits[index] = null;
    waiting = false;
  }

  $effect(() => {
    const now = raw;
    untrack(() => {
      if (!now || phase !== 'step' || !rest) return;
      if (waiting) {
        if (settled(rest, now)) next();
        return;
      }
      const hit = detect(STEPS[index], rest, now);
      if (hit) {
        hits[index] = hit;
        waiting = true;
      }
    });
  });

  function prompt(step: Step): string {
    if (step.kind === 'button') return t('identify.press', { control: controlLabel(step.button, null) });
    if (step.kind === 'trigger') return t('identify.pull', { control: controlLabel(step.trigger === 'left' ? 'LT' : 'RT', null) });
    const stick = t(`controls.${step.axis.startsWith('Left') ? 'LeftStick' : 'RightStick'}`);
    return t(step.axis.endsWith('X') ? 'identify.push_right' : 'identify.push_up', { stick });
  }

  const found = $derived(hits.filter(Boolean).length);

  async function run(p: Promise<unknown>, done: string) {
    try {
      await p;
      app.toast(done, 'ok');
      onclose();
    } catch (e) {
      app.toast(errorText(e), 'error');
    }
  }

  /** Short text of the raw input, so the user sees the pad answer. */
  const rawText = $derived.by(() => {
    if (!raw) return '';
    const pressed = raw.buttons.flatMap((b, i) => (b ? [String(i + 1)] : []));
    const hats = raw.hats.map((h) => (h === null ? '·' : String(h)));
    const axes = raw.axes.map((v) => v.toFixed(1));
    return [
      `${t('identify.raw_buttons')}: ${pressed.join(' ') || '–'}`,
      hats.length ? `${t('identify.raw_hat')}: ${hats.join(' ')}` : '',
      `${t('identify.raw_axes')}: ${axes.join(' ')}`,
    ]
      .filter(Boolean)
      .join('   ');
  });
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="backdrop" role="presentation" onclick={onclose}>
  <div class="dialog" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
    <div class="head">
      <h3>{t('identify.title', { name: device?.name ?? deviceId })}</h3>
      <button class="icon-btn" onclick={onclose} title={t('common.close')}><X size={17} /></button>
    </div>

    {#if !device || !raw}
      <p class="hint">{t('identify.gone')}</p>
    {:else if phase === 'intro'}
      <p>{t('identify.intro')}</p>
      <div class="actions">
        {#if known}
          <button class="btn" onclick={() => run(api.forgetLayout(key), t('identify.forgotten'))}>{t('identify.forget')}</button>
        {/if}
        <button class="btn primary" onclick={start}>{t('identify.start')}</button>
      </div>
    {:else if phase === 'step'}
      <div class="progress">{t('identify.step', { n: index + 1, of: STEPS.length })}</div>
      <p class="prompt">{prompt(STEPS[index])}</p>
      <p class="hint">{waiting ? t('identify.release') : t('identify.waiting')}</p>
      <div class="actions">
        <button class="btn" disabled={index === 0} onclick={back}>{t('identify.back')}</button>
        <button class="btn" onclick={next}>{t('identify.skip')}</button>
      </div>
    {:else}
      <p>{t('identify.done', { n: found, of: STEPS.length })}</p>
      <div class="actions">
        <button class="btn" onclick={start}>{t('identify.again')}</button>
        <button class="btn primary" disabled={found === 0} onclick={() => run(api.setLayout(buildLayout(key, hits)), t('identify.saved'))}>
          {t('identify.save')}
        </button>
      </div>
    {/if}
    {#if raw}<p class="raw">{rawText}</p>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(5, 7, 10, 0.6);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .dialog {
    width: 460px;
    max-width: calc(100vw - 32px);
    background: var(--bg-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow);
    padding: 18px 20px 22px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  p {
    margin: 0;
  }
  .hint {
    color: var(--text-dim);
    font-size: 12px;
  }
  .progress {
    color: var(--text-dim);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .prompt {
    font-size: 18px;
    font-weight: 600;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .raw {
    border-top: 1px solid var(--line);
    padding-top: 10px;
    color: var(--text-dim);
    font-family: var(--mono);
    font-size: 11px;
    word-break: break-word;
  }
</style>
