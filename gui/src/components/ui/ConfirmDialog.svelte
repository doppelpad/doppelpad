<script lang="ts">
  // The app's OK/Cancel question (`app.confirmAsk`); the system's message box is never used.
  import { TriangleAlert } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import Dialog from './Dialog.svelte';

  function finish(ok: boolean) {
    const q = app.question;
    app.question = null;
    q?.resolve(ok);
  }
</script>

{#if app.question}
  <Dialog title={t('common.confirm')} onclose={() => finish(false)} width={420} gap={14} showClose={false}>
    {#snippet icon()}<span class="warn"><TriangleAlert size={17} /></span>{/snippet}
    <p>{app.question.message}</p>
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
</style>
