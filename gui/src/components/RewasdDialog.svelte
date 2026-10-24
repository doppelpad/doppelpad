<script lang="ts">
  import { LoaderCircle, ShieldCheck, TriangleAlert } from '@lucide/svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';
  import Dialog from './ui/Dialog.svelte';
  import Toggle from './ui/Toggle.svelte';
  import { tip } from '../lib/tooltip.svelte';

  let remember = $state(false);

  async function quit() {
    if (remember) await app.setSettings({ rewasd_at_start: 'quit' });
    await app.quitRewasd();
    app.rewasdAsk = false;
  }

  async function notNow() {
    if (remember) await app.setSettings({ rewasd_at_start: 'ignore' });
    app.rewasdAsk = false;
  }

  function dismiss() {
    if (!app.rewasdQuitting) app.rewasdAsk = false;
  }
</script>

{#if app.rewasdAsk && app.rewasd}
  <Dialog title={t('rewasd.title')} onclose={dismiss} width={480} showClose={false}>
    {#snippet icon()}<span class="warn"><TriangleAlert size={17} /></span>{/snippet}
    <p>{t('rewasd.body')}</p>
    <ul>
      {#if app.rewasd.service}
        <li>
          <span>{app.rewasd.service} (jlserv)</span>
          <span class="admin" use:tip={t('rewasd.admin_hint')}><ShieldCheck size={13} /> {t('rewasd.admin')}</span>
        </li>
      {/if}
      {#each app.rewasd.programs as name (name)}
        <li><span>{name}</span></li>
      {/each}
    </ul>
    <p class="hint">
      {#if app.rewasd.service}{t('rewasd.admin_hint')}{/if}
      {t('rewasd.restart_hint')}
    </p>
    <div class="foot">
      <Toggle checked={remember} label={t('rewasd.remember')} disabled={app.rewasdQuitting} onchange={(v) => (remember = v)} />
      <div class="buttons">
        <button class="btn" disabled={app.rewasdQuitting} onclick={notNow}>{t('rewasd.not_now')}</button>
        <button class="btn primary" disabled={app.rewasdQuitting} onclick={quit}>
          {#if app.rewasdQuitting}
            <span class="spin"><LoaderCircle size={14} /></span>
            {t('rewasd.quitting')}
          {:else}
            {t('rewasd.quit')}
          {/if}
        </button>
      </div>
    </div>
  </Dialog>
{/if}

<style>
  .warn {
    display: inline-flex;
    color: var(--warning);
  }
  p {
    margin: 0;
    color: var(--text-dim);
  }
  ul {
    margin: 0;
    padding: 8px 10px;
    list-style: none;
    background: var(--bg-2);
    border-radius: var(--radius-s);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .admin {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--fs-xs);
    color: var(--text-faint);
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: 4px;
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
  .spin {
    display: inline-flex;
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
