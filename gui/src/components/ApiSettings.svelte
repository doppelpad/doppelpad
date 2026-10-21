<script lang="ts">
  // The local HTTP API for stream decks and scripts (see docs/API.md): off by default; this
  // computer only unless the network is allowed, and then only with the token.
  import { Copy, RefreshCw } from '@lucide/svelte';
  import { api, errorText } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';
  import type { HttpSettings } from '../lib/types';
  import Toggle from './ui/Toggle.svelte';

  const http = $derived(app.live?.http ?? null);

  async function run(p: Promise<unknown>) {
    try {
      await p;
      return true;
    } catch (e) {
      app.toast(errorText(e), 'error');
      return false;
    }
  }

  const set = (patch: Partial<HttpSettings>) => http && run(api.setHttp({ ...http.settings, ...patch }));

  function setPort(v: string) {
    const port = Number(v);
    if (Number.isInteger(port) && port > 0 && port < 65536) set({ port });
  }

  async function copyToken() {
    if (!http) return;
    if (await run(navigator.clipboard.writeText(http.settings.token))) app.toast(t('settings.http.copied'), 'ok');
  }
</script>

{#if http}
  <div class="section">
    <div class="section-head"><span>{t('settings.http.title')}</span></div>
    <p class="hint">{t('settings.http.hint')}</p>
    <Toggle checked={http.settings.enabled} label={t('settings.http.enabled')} onchange={(v) => set({ enabled: v })} />
    <label class="row">
      <span>{t('settings.dsu.port')}</span>
      <input type="number" min="1" max="65535" value={http.settings.port} onchange={(e) => setPort(e.currentTarget.value)} />
    </label>
    <Toggle checked={http.settings.lan} label={t('settings.http.lan')} onchange={(v) => set({ lan: v })} />
    {#if http.settings.token}
      <div class="row">
        <span>{t('settings.http.token')}</span>
        <code>{http.settings.token}</code>
        <button class="icon-btn" title={t('settings.http.copy')} onclick={copyToken}><Copy size={14} /></button>
        <button class="icon-btn" title={t('settings.http.new_token')} onclick={() => run(api.newHttpToken())}><RefreshCw size={14} /></button>
      </div>
    {/if}
    {#if http.error}
      <p class="error">{http.error}</p>
    {:else if http.address}
      <p class="hint">{t('settings.http.status', { address: http.address })}</p>
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
    gap: 10px;
  }
  .row input {
    width: 110px;
  }
  code {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 11px;
  }
</style>
