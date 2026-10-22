<script lang="ts">
  // How a mapping plays: turbo (with its period and random extra), toggle, unmapped, or a
  // macro (opens the macro editor).
  import { Clapperboard, Repeat, Zap } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { applyMode, isBlocked, isMacroMode, modeOf, nativeAction, removeMapping } from '../../lib/model/profile';
  import { app } from '../../lib/state/app.svelte';
  import type { Combo, Mapping, Mode } from '../../lib/types';
  import Toggle from '../ui/Toggle.svelte';
  import { configTiming, ms, type EditMapping } from './edit';
  import MsInput from './MsInput.svelte';

  let {
    mapping,
    combo,
    readOnly,
    edit,
  }: { mapping: Mapping | undefined; combo: Combo; readOnly: boolean; edit: EditMapping } = $props();

  const mode = $derived<Mode>(mapping ? modeOf(mapping) : 'hold');
  const blocked = $derived(isBlocked(mapping));
  const isMacro = $derived(isMacroMode(mode));
  /** The control's own output, which turbo/toggle fall back to when nothing is assigned. */
  const native = $derived(nativeAction(combo));
  /** Turbo/toggle can always be switched off; on needs outputs: assigned ones or the control's own. */
  const canEnableMode = $derived(!blocked && ((mapping?.actions ?? []).length > 0 || !!native));
  const timing = $derived(configTiming());

  function setMode(next: Mode) {
    edit((m) => applyMode(m, next, native));
  }

  function setBlocked(on: boolean) {
    if (on) {
      edit((m) => {
        m.actions = [];
        m.jump = undefined;
        m.mode = 'hold';
      }, true);
    } else {
      app.edit((p) => removeMapping(p, app.layer, combo, app.activator));
    }
  }

  function openMacro() {
    app.macroTarget = { layer: app.layer, trigger: combo, activator: app.activator };
    app.view = 'macro';
  }
</script>

<div class="modes">
  <div class="mode-row">
    <Toggle
      checked={mode === 'turbo'}
      disabled={readOnly || (mode !== 'turbo' && !canEnableMode)}
      label={t('binding.turbo')}
      onchange={(v) => setMode(v ? 'turbo' : 'hold')}
    />
    <Zap size={14} />
    {#if mode === 'turbo'}
      <label class="inline">
        {t('binding.period')}
        <MsInput
          value={mapping?.turbo_ms}
          placeholder={String(timing.turbo_ms ?? '')}
          min={10}
          onchange={(v) => edit((m) => (m.turbo_ms = ms(v)))}
        />
      </label>
      <label class="inline" title={t('binding.random_hint')}>
        {t('binding.random')}
        <MsInput
          value={mapping?.random_ms}
          placeholder="0"
          max={10000}
          onchange={(v) => edit((m) => (m.random_ms = ms(v) || undefined))}
        />
      </label>
    {/if}
  </div>
  <div class="mode-row">
    <Toggle
      checked={mode === 'toggle'}
      disabled={readOnly || (mode !== 'toggle' && !canEnableMode)}
      label={t('binding.toggle')}
      onchange={(v) => setMode(v ? 'toggle' : 'hold')}
    />
    <Repeat size={14} />
  </div>
  <div class="mode-row">
    <Toggle checked={blocked} disabled={readOnly} label={t('binding.unmap')} onchange={setBlocked} />
  </div>
  <button class="btn macro" class:on={isMacro} onclick={openMacro} disabled={readOnly && !isMacro}>
    <Clapperboard size={14} />
    {t('binding.macro')}
    {#if isMacro}<span class="tag">{t(`macro.mode_${mode === 'while_held' ? 'while' : mode}`)}</span>{/if}
  </button>
</div>

<style>
  .modes {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 0 2px;
    border-top: 1px solid var(--line);
    margin-top: 4px;
  }
  .mode-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 26px;
  }
  .mode-row > :global(svg) {
    color: var(--text-faint);
  }
  .inline {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-dim);
    font-size: var(--fs-s);
    margin-left: auto;
  }
  .macro {
    align-self: flex-start;
  }
  .macro.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .tag {
    font-size: var(--fs-xs);
    background: var(--accent-soft);
    border-radius: var(--radius-pill);
    padding: 0 6px;
  }
</style>
