<script lang="ts">
  // Which keyboard and mouse input the config reads, and how held keys of the virtual keyboard
  // repeat.
  import { t } from '../../lib/i18n/index.svelte';
  import { setVirtualDevice } from '../../lib/model/layers';
  import { app } from '../../lib/state/app.svelte';
  import type { KbmSource, KeyRepeat } from '../../lib/types';

  const KBM_SOURCES: KbmSource[] = ['all', 'physical', 'scripts'];

  const repeat = $derived<KeyRepeat>(app.profile?.virtual?.keyboard?.repeat ?? 'native');
  const REPEATS = ['native', 'fast', 'slow', 'off', 'custom'] as const;

  function setRepeat(r: KeyRepeat) {
    app.edit((p) => setVirtualDevice(p, 'keyboard', r === 'native' ? undefined : { repeat: r }));
  }
</script>

<label class="field">
  <span class="fl">{t('output.kbm_source')}</span>
  <select
    value={app.profile?.kbm_source ?? 'all'}
    disabled={app.readOnly}
    onchange={(e) => {
      const v = e.currentTarget.value as KbmSource;
      app.edit((p) => ({ ...p, kbm_source: v === 'all' ? undefined : v }));
    }}
  >
    {#each KBM_SOURCES as s (s)}<option value={s}>{t(`output.kbm_sources.${s}`)}</option>{/each}
  </select>
</label>
<p class="hint">{t('output.kbm_source_hint')}</p>

<label class="field">
  <span class="fl">{t('output.key_repeat')}</span>
  <select
    value={typeof repeat === 'number' ? 'custom' : repeat}
    disabled={app.readOnly}
    onchange={(e) => {
      const v = e.currentTarget.value;
      setRepeat(v === 'custom' ? 30 : (v as KeyRepeat));
    }}
  >
    {#each REPEATS as r (r)}
      <option value={r}>{t(`output.repeat.${r}`)}</option>
    {/each}
  </select>
  {#if typeof repeat === 'number'}
    <input
      type="number"
      min="1"
      max="1000"
      value={repeat}
      disabled={app.readOnly}
      onchange={(e) => {
        const n = parseInt(e.currentTarget.value, 10);
        if (n >= 1 && n <= 1000) setRepeat(n);
      }}
    />
    <span class="unit">{t('common.ms')}</span>
  {/if}
</label>
<p class="hint">{t(`output.repeat_hint.${typeof repeat === 'number' ? 'custom' : repeat}`)}</p>

<style>
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
  }
  .fl {
    flex: none;
  }
  select {
    min-width: 200px;
  }
  input {
    width: 80px;
  }
  .unit {
    color: var(--text-dim);
  }
</style>
