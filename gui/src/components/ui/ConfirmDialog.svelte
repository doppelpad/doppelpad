<script lang="ts">
  // The app's OK/Cancel question (`app.confirmAsk`); the system's message box is never used.
  import { CircleArrowDown, TriangleAlert } from '@lucide/svelte';
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
  {@const q = app.question}
  <Dialog title={q.title ?? t('common.confirm')} onclose={() => finish(false)} width={q.details ? 520 : 420} gap={14} showClose={false}>
    {#snippet icon()}
      {#if q.kind === 'update'}
        <span class="accent"><CircleArrowDown size={17} /></span>
      {:else}
        <span class="warn"><TriangleAlert size={17} /></span>
      {/if}
    {/snippet}
    <p>{q.message}</p>
    {#if q.details}<div class="details">{q.details}</div>{/if}
    {#snippet footer()}
      <button class="btn" onclick={() => finish(false)}>{q.cancel ?? t('common.cancel')}</button>
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn primary" onclick={() => finish(true)} autofocus>{q.ok ?? t('common.ok')}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .warn {
    display: inline-flex;
    color: var(--warning);
  }
  .accent {
    display: inline-flex;
    color: var(--accent);
  }
  p {
    margin: 0;
    color: var(--text-dim);
  }
  /* Release notes and the like: as written, scrolling when long. */
  .details {
    max-height: 260px;
    overflow-y: auto;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    background: var(--bg-2);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }
</style>
