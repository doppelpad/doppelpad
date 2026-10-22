<script lang="ts">
  // The local HTTP API for stream decks and scripts (see docs/API.md): off by default; this
  // computer only unless the network is allowed, and then only with the token.
  import { Copy, RefreshCw } from '@lucide/svelte';
  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { parsePort } from '../../lib/model/util';
  import { app } from '../../lib/state/app.svelte';
  import type { HttpSettings } from '../../lib/types';
  import Toggle from '../ui/Toggle.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingsSection from './SettingsSection.svelte';

  const http = $derived(app.live?.http ?? null);

  const set = (patch: Partial<HttpSettings>) => http && app.attempt(api.setHttp({ ...http.settings, ...patch }));

  function setPort(v: string) {
    const port = parsePort(v);
    if (port !== null) set({ port });
  }

  function copyToken() {
    if (http) app.attempt(navigator.clipboard.writeText(http.settings.token), t('settings.http.copied'));
  }
</script>

{#if http}
  <SettingsSection title={t('settings.http.title')} hint={t('settings.http.hint')}>
    <Toggle checked={http.settings.enabled} label={t('settings.http.enabled')} onchange={(v) => set({ enabled: v })} />
    <SettingRow label={t('settings.dsu.port')}>
      <input type="number" min="1" max="65535" value={http.settings.port} onchange={(e) => setPort(e.currentTarget.value)} />
    </SettingRow>
    <Toggle checked={http.settings.lan} label={t('settings.http.lan')} onchange={(v) => set({ lan: v })} />
    {#if http.settings.token}
      <div class="token">
        <span>{t('settings.http.token')}</span>
        <code>{http.settings.token}</code>
        <button class="icon-btn" title={t('settings.http.copy')} onclick={copyToken}><Copy size={14} /></button>
        <button class="icon-btn" title={t('settings.http.new_token')} onclick={() => app.attempt(api.newHttpToken())}><RefreshCw size={14} /></button>
      </div>
    {/if}
    {#if http.error}
      <p class="error-text">{http.error}</p>
    {:else if http.address}
      <p class="hint">{t('settings.http.status', { address: http.address })}</p>
    {/if}
  </SettingsSection>
{/if}

<style>
  input {
    width: 110px;
  }
  .token {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  code {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: var(--fs-xs);
  }
</style>
