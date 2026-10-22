<script lang="ts">
  // The app's OK/Cancel question (`app.confirmAsk`); the system's message box is never used.
  // Questions with an id offer "Don't ask again", which takes effect when answered with OK.
  import { TriangleAlert } from '@lucide/svelte';
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
  <Dialog title={t('common.confirm')} onclose={() => finish(false)} width={420} gap={14} showClose={false}>
    {#snippet icon()}<span class="warn"><TriangleAlert size={17} /></span>{/snippet}
    <p>{app.question.message}</p>
    {#if app.question.id}
      <label class="silence"><input type="checkbox" bind:checked={silence} /> {t('questions.dont_ask')}</label>
    {/if}
    {#snippet footer()}
      <button class="btn" onclick={() => finish(false)}>{t('common.cancel')}</button>
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
