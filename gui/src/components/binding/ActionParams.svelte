<script lang="ts">
  // Inline fields for the parameters of one action: stick amount and mode, trigger value, wheel
  // clicks, mouse speed, rumble length and strength, launch path and arguments.
  import { parseDecimal, parsePercent } from '../../lib/model/numbers';
  import { t } from '../../lib/i18n/index.svelte';
  import { stickAmount, withStickAmount } from '../../lib/model/actions';
  import type { Action } from '../../lib/types';

  let { action, disabled = false, onchange }: { action: Action; disabled?: boolean; onchange: (a: Action) => void } =
    $props();

  const pct = (v: number) => Math.round(v * 100);

  const percent = parsePercent;

  function positive(raw: string, min = 0): number | null {
    const n = parseDecimal(raw);
    return n !== null && n >= min ? n : null;
  }

  function set(patch: Record<string, unknown>) {
    onchange({ ...action, ...patch } as Action);
  }
</script>

{#snippet field(label: string, value: number | string, unit: string, change: (raw: string) => void, wide = false)}
  <label class="param" class:wide>
    <span>{label}</span>
    <input type={wide ? 'text' : 'number'} {value} {disabled} onchange={(e) => change(e.currentTarget.value)} />
    {#if unit}<span class="unit">{unit}</span>{/if}
  </label>
{/snippet}

<span class="params">
  {#if 'stick' in action}
    {@render field(t('params.amount'), pct(stickAmount(action)), '%', (raw) => {
      const v = percent(raw);
      if (v !== null && 'stick' in action) onchange(withStickAmount(action, v));
    })}
    <label class="param" title={t('params.absolute_hint')}>
      <input
        type="checkbox"
        checked={action.absolute ?? false}
        {disabled}
        onchange={(e) => set({ absolute: e.currentTarget.checked || undefined })}
      />
      <span>{t('params.absolute')}</span>
    </label>
  {:else if 'trigger' in action}
    {@render field(t('params.amount'), pct(action.value ?? 1), '%', (raw) => {
      const v = percent(raw);
      if (v !== null) set({ value: v });
    })}
  {:else if 'wheel' in action}
    {@render field(t('params.clicks'), action.clicks ?? 1, '', (raw) => {
      const v = positive(raw, 0.1);
      if (v !== null) set({ clicks: v });
    })}
  {:else if 'mouse_move' in action}
    {@render field(t('params.speed'), action.speed ?? 800, t('params.px_s'), (raw) => {
      const v = positive(raw, 1);
      if (v !== null) set({ speed: Math.round(v) });
    })}
    {@render field(t('params.acceleration'), action.acceleration ?? 0, '×/s', (raw) => {
      const v = positive(raw, 0);
      if (v !== null) set({ acceleration: v > 0 ? Math.min(v, 100) : undefined });
    })}
  {:else if 'mouse_to' in action}
    {@const at = action.mouse_to}
    {@render field('X', pct(at[0]), '%', (raw) => {
      const v = percent(raw);
      if (v !== null) set({ mouse_to: [v, at[1]] });
    })}
    {@render field('Y', pct(at[1]), '%', (raw) => {
      const v = percent(raw);
      if (v !== null) set({ mouse_to: [at[0], v] });
    })}
  {:else if 'mouse_clip' in action}
    {@const r = action.mouse_clip}
    {#each ['X1', 'Y1', 'X2', 'Y2'] as name, i (name)}
      {@render field(name, pct(r[i]), '%', (raw) => {
        const v = percent(raw);
        if (v !== null) set({ mouse_clip: r.map((x, j) => (j === i ? v : x)) });
      })}
    {/each}
  {:else if 'touch' in action}
    {@render field(t('params.length'), action.ms ?? '', t('common.ms'), (raw) => {
      const v = positive(raw, 10);
      set({ ms: v === null ? undefined : Math.min(Math.round(v), 10000) });
    })}
  {:else if 'turn' in action}
    {@render field(t('params.degrees'), action.turn, '°', (raw) => {
      const n = parseDecimal(raw);
      if (n !== null && Math.abs(n) <= 3600) set({ turn: n });
    })}
  {:else if 'rumble' in action}
    {@render field(t('params.length'), action.rumble, t('common.ms'), (raw) => {
      const v = positive(raw, 1);
      if (v !== null) set({ rumble: Math.round(v) });
    })}
    {@render field(t('params.low'), pct(action.low ?? 0.8), '%', (raw) => {
      const v = percent(raw);
      if (v !== null) set({ low: v });
    })}
    {@render field(t('params.high'), pct(action.high ?? 0.8), '%', (raw) => {
      const v = percent(raw);
      if (v !== null) set({ high: v });
    })}
  {:else if 'command' in action && typeof action.command === 'object' && 'launch' in action.command}
    {@const launch = action.command.launch}
    {@render field(t('params.path'), launch.path, '', (raw) => {
      if (raw.trim()) onchange({ command: { launch: { ...launch, path: raw.trim() } } });
    }, true)}
    {@render field(t('params.args'), launch.args ?? '', '', (raw) => {
      onchange({ command: { launch: { ...launch, args: raw } } });
    }, true)}
  {/if}
</span>

<style>
  .params {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
  }
  .param {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--text-dim);
    font-size: 12px;
  }
  .param input:not([type='checkbox']) {
    width: 64px;
  }
  .param.wide input {
    width: 180px;
  }
  .unit {
    color: var(--text-faint);
  }
</style>
