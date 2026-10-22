<script lang="ts">
  import { LoaderCircle, ShieldCheck, TriangleAlert } from '@lucide/svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';
  import Toggle from './ui/Toggle.svelte';

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
  <div class="backdrop" role="presentation" onclick={dismiss}>
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="rewasd-title"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.key === 'Escape' && dismiss()}
    >
      <h3 id="rewasd-title"><TriangleAlert size={17} /> {t('rewasd.title')}</h3>
      <p>{t('rewasd.body')}</p>
      <ul>
        {#if app.rewasd.service}
          <li>
            <span>{app.rewasd.service} (jlserv)</span>
            <span class="admin" title={t('rewasd.admin_hint')}><ShieldCheck size={13} /> {t('rewasd.admin')}</span>
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
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--backdrop);
    display: grid;
    place-items: center;
    z-index: var(--z-dialog);
  }
  .dialog {
    width: 480px;
    max-width: calc(100vw - 32px);
    background: var(--bg-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow);
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  h3 :global(svg) {
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
  .hint {
    font-size: var(--fs-s);
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
