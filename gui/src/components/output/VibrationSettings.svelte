<script lang="ts">
  // How the game's rumble reaches the controller: each motor's strength, a swap, and the
  // shortest gap between rumble commands. Rumble that mappings play is not changed.
  import { t } from '../../lib/i18n/index.svelte';
  import { setOrDrop } from '../../lib/model/util';
  import { app } from '../../lib/state/app.svelte';
  import type { Vibration } from '../../lib/types';
  import Slider from '../ui/Slider.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import { vibrationConfig } from '../../lib/model/output';

  const def = $derived(app.catalog?.defaults.vibration ?? { left: 1, right: 1, swap: false, min_delay_ms: 20 });
  const v = $derived(vibrationConfig(app.profile?.vibration, def));
  const disabled = $derived(app.readOnly);

  function set(patch: Partial<Vibration>) {
    const next = { ...v, ...patch };
    const same = (Object.keys(def) as (keyof Vibration)[]).every((k) => next[k] === def[k]);
    app.edit((p) => setOrDrop(p, 'vibration', same ? undefined : next));
  }
</script>

<div class="section-title">{t('vibration.title')}</div>
<p class="hint">{t('vibration.hint')}</p>
<div class="fields">
  <Slider
    label={t('vibration.left')}
    value={v.left}
    def={def.left}
    min={0}
    max={1}
    step={0.05}
    scale={100}
    unit="%"
    {disabled}
    onchange={(x) => set({ left: x })}
  />
  <Slider
    label={t('vibration.right')}
    value={v.right}
    def={def.right}
    min={0}
    max={1}
    step={0.05}
    scale={100}
    unit="%"
    {disabled}
    onchange={(x) => set({ right: x })}
  />
  <Slider
    label={t('vibration.gap')}
    value={v.min_delay_ms}
    def={def.min_delay_ms}
    min={10}
    max={200}
    step={5}
    unit={t('common.ms')}
    hint={t('vibration.gap_hint')}
    {disabled}
    onchange={(x) => set({ min_delay_ms: Math.round(x) })}
  />
</div>
<Toggle checked={v.swap} {disabled} label={t('vibration.swap')} onchange={(x) => set({ swap: x })} />

<style>
  .section-title {
    margin-top: 10px;
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 14px 24px;
  }
</style>
