<script lang="ts">
  // How the touchpad becomes region controls: one pad or two trackpads (its halves), radial
  // sectors or a 3 × 3 grid, the size of the centre, and whether regions need the click.
  // For the whole config.
  import { t } from '../../lib/i18n/index.svelte';
  import { setTouchpad, touchpadConfig } from '../../lib/model/analog';
  import { app } from '../../lib/state/app.svelte';
  import type { TouchpadSettings } from '../../lib/types';
  import Slider from '../ui/Slider.svelte';
  import Toggle from '../ui/Toggle.svelte';

  const def = $derived(app.catalog?.defaults.touchpad);
  const cfg = $derived(app.profile && def ? touchpadConfig(app.profile, def) : null);
  const disabled = $derived(app.readOnly);

  function set(patch: Partial<TouchpadSettings>) {
    if (def) app.edit((p) => setTouchpad(p, patch, def));
  }
</script>

{#if cfg && def}
  <div class="touchpad">
    <Toggle checked={cfg.split} {disabled} label={t('analog.touchpad.split')} onchange={(v) => set({ split: v })} />
    <p class="hint">{t('analog.touchpad.split_hint')}</p>

    <div class="choice">
      <span class="cl">{t('analog.touchpad.zones')}</span>
      <div class="seg">
        {#each ['radial', 'grid'] as const as z (z)}
          <button class:on={cfg.zones === z} {disabled} onclick={() => set({ zones: z })}>{t(`analog.touchpad.zones_${z}`)}</button>
        {/each}
      </div>
    </div>
    <p class="hint">{t(`analog.touchpad.zones_hint_${cfg.zones}`)}</p>

    {#if cfg.zones === 'radial'}
      <Slider
        label={t('analog.touchpad.centre')}
        value={cfg.centre}
        def={def.centre}
        min={0.05}
        max={0.9}
        step={0.01}
        scale={100}
        unit="%"
        hint={t('analog.touchpad.centre_hint')}
        {disabled}
        onchange={(v) => set({ centre: v })}
      />
    {/if}

    <Toggle
      checked={cfg.click_required}
      {disabled}
      label={t('analog.touchpad.click_required')}
      onchange={(v) => set({ click_required: v })}
    />
    <p class="hint">{t('analog.touchpad.gestures_hint')}</p>
  </div>
{/if}

<style>
  .touchpad {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 560px;
  }
  .choice {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .cl {
    min-width: 120px;
    font-size: 13px;
  }
  .seg {
    display: flex;
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
    overflow: hidden;
  }
  .seg button {
    padding: 5px 12px;
    font-size: var(--fs-s);
    color: var(--text-dim);
  }
  .seg button + button {
    border-left: 1px solid var(--line);
  }
  .seg button.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .hint {
    margin: -4px 0 4px;
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
</style>
