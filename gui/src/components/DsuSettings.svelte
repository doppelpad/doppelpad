<script lang="ts">
  // The DSU ("cemuhook") motion server: emulators read the controllers' buttons, sticks and
  // gyro from it. Off by default; this computer only unless the network is allowed.
  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { parsePort } from '../lib/model/util';
  import { app } from '../lib/state/app.svelte';
  import type { DsuSettings } from '../lib/types';
  import Toggle from './ui/Toggle.svelte';

  const dsu = $derived(app.live?.dsu ?? null);

  function set(patch: Partial<DsuSettings>) {
    if (dsu) app.attempt(api.setDsu({ ...dsu.settings, ...patch }));
  }

  function setPort(v: string) {
    const port = parsePort(v);
    if (port !== null) set({ port });
  }
</script>

{#if dsu}
  <div class="section">
    <div class="section-title"><span>{t('settings.dsu.title')}</span></div>
    <p class="hint">{t('settings.dsu.hint')}</p>
    <Toggle checked={dsu.settings.enabled} label={t('settings.dsu.enabled')} onchange={(v) => set({ enabled: v })} />
    <label class="row">
      <span>{t('settings.dsu.port')}</span>
      <input type="number" min="1" max="65535" value={dsu.settings.port} onchange={(e) => setPort(e.currentTarget.value)} />
    </label>
    <Toggle checked={dsu.settings.lan} label={t('settings.dsu.lan')} onchange={(v) => set({ lan: v })} />
    {#if dsu.error}
      <p class="error-text">{dsu.error}</p>
    {:else if dsu.address}
      <p class="hint">{t('settings.dsu.status', { address: dsu.address, n: dsu.clients })}</p>
    {/if}
  </div>
{/if}

<style>
  .section {
    border-top: 1px solid var(--line);
    padding-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .hint {
    margin: 0;
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .row input {
    width: 110px;
  }
</style>
