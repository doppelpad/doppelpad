<script lang="ts">
  // The app's questions (`app.confirmAsk`, `app.choose`); the system's message box is never used.
  import { TriangleAlert } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { app, type Answer } from '../../lib/state/app.svelte';
  import Dialog from './Dialog.svelte';

  function finish(answer: Answer) {
    const q = app.question;
    app.question = null;
    q?.resolve(answer);
  }
</script>

{#if app.question}
  {@const q = app.question}
  <Dialog title={q.title ?? t('common.confirm')} onclose={() => finish(null)} width={q.items ? 540 : 420} gap={14} showClose={false}>
    {#snippet icon()}<span class="warn"><TriangleAlert size={17} /></span>{/snippet}
    <p>{q.message}</p>
    {#if q.items?.length}
      <ul class="items">
        {#each q.items as item, i (i)}
          <li>
            <code>{item.code}</code>
            {#each item.notes as note, j (j)}<span class="note">{note}</span>{/each}
          </li>
        {/each}
      </ul>
    {/if}
    {#snippet footer()}
      <button class="btn" onclick={() => finish(null)}>{t('common.cancel')}</button>
      {#if q.alt}<button class="btn" onclick={() => finish('alt')}>{q.alt}</button>{/if}
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn primary" onclick={() => finish('ok')} autofocus>{q.ok ?? t('common.ok')}</button>
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
  .items {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 260px;
    overflow-y: auto;
  }
  li {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    background: var(--bg-2);
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
  }
  code {
    font-family: var(--mono);
    font-size: var(--fs-s);
    white-space: pre-wrap;
    word-break: break-all;
  }
  .note {
    font-size: var(--fs-xs);
    color: var(--text-dim);
  }
</style>
