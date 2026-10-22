<script lang="ts">
  // The app's OK/Cancel question (`app.confirmAsk`), or a hint with OK only (`app.inform`); the
  // system's message box is never used. Those with an id offer "Don't ask (show) again", which
  // takes effect with OK.
  import { Info, TriangleAlert } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import Dialog from './Dialog.svelte';

  let silence = $state(false);

  function finish(ok: boolean) {
    const q = app.question;
    app.question = null;
    if (ok && silence && q?.id) app.silence(q.id);
    silence = false;
    q?.resolve(ok);
  }
</script>

{#if app.question}
  {@const info = app.question.info}
  <Dialog title={t(info ? 'common.hint' : 'common.confirm')} onclose={() => finish(!!info)} width={420} gap={14} showClose={false}>
    {#snippet icon()}
      {#if info}<span class="info"><Info size={17} /></span>{:else}<span class="warn"><TriangleAlert size={17} /></span>{/if}
    {/snippet}
    <p>{app.question.message}</p>
    {#if app.question.id}
      <label class="silence"
        ><input type="checkbox" bind:checked={silence} /> {t(info ? 'questions.dont_show' : 'questions.dont_ask')}</label
      >
    {/if}
    {#snippet footer()}
      {#if !info}<button class="btn" onclick={() => finish(false)}>{t('common.cancel')}</button>{/if}
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn primary" onclick={() => finish(true)} autofocus>{t('common.ok')}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .warn {
    display: inline-flex;
    color: var(--warning);
  }
  .info {
    display: inline-flex;
    color: var(--accent);
  }
  p {
    margin: 0;
    color: var(--text-dim);
  }
  .silence {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-s);
    color: var(--text-dim);
  }
</style>
