<script lang="ts">
  // The DSU ("cemuhook") motion server: emulators read the controllers' buttons, sticks and
  // gyro from it. Off by default; this computer only unless the network is allowed.
  import { api, errorText } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';
  import type { DsuSettings } from '../lib/types';
  import Toggle from './ui/Toggle.svelte';

  const dsu = $derived(app.live?.dsu ?? null);

  async function set(patch: Partial<DsuSettings>) {
    if (!dsu) return;
    try {
      await api.setDsu({ ...dsu.settings, ...patch });
    } catch (e) {
      app.toast(errorText(e), 'error');
    }
  }

  function setPort(v: string) {
    const port = Number(v);
    if (Number.isInteger(port) && port > 0 && port < 65536) set({ port });
  }
</script>

{#if dsu}
  <div class="section">
    <div class="section-head"><span>{t('settings.dsu.title')}</span></div>
    <p class="hint">{t('settings.dsu.hint')}</p>
    <Toggle checked={dsu.settings.enabled} label={t('settings.dsu.enabled')} onchange={(v) => set({ enabled: v })} />
    <label class="row">
      <span>{t('settings.dsu.port')}</span>
      <input type="number" min="1" max="65535" value={dsu.settings.port} onchange={(e) => setPort(e.currentTarget.value)} />
    </label>
    <Toggle checked={dsu.settings.lan} label={t('settings.dsu.lan')} onchange={(v) => set({ lan: v })} />
    {#if dsu.error}
      <p class="error">{dsu.error}</p>
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
  .section-head {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-dim);
  }
  .hint {
    margin: 0;
    color: var(--text-dim);
    font-size: 12px;
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: 12px;
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
