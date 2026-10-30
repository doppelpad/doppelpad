<script lang="ts">
  // The app's questions (`app.confirmAsk`, `app.choose`) and hints with OK only (`app.inform`),
  // one at a time from the queue in `lib/state/dialogs.svelte.ts`; the system's message box is
  // never used. Those with an id offer "Don't ask (show) again", which takes effect with OK.
  import { CircleArrowDown, Info, TriangleAlert } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import type { Answer } from '../../lib/state/dialogs.svelte';
  import Dialog from './Dialog.svelte';

  let silence = $state(false);

  function finish(answer: Answer) {
    const silenced = silence;
    silence = false;
    app.dialogs.answerQuestion(answer, silenced);
  }
</script>

{#if app.dialogs.question}
  {@const q = app.dialogs.question}
  {#key q}
    <Dialog
      title={q.title ?? t(q.info ? 'common.hint' : 'common.confirm')}
      onclose={() => finish(q.info ? 'ok' : null)}
      width={q.items ? 540 : q.details ? 520 : 420}
      gap={14}
      showClose={false}
    >
      {#snippet icon()}
        {#if q.kind === 'update'}
          <span class="accent"><CircleArrowDown size={17} /></span>
        {:else if q.info}
          <span class="accent"><Info size={17} /></span>
        {:else}
          <span class="warn"><TriangleAlert size={17} /></span>
        {/if}
      {/snippet}
      <p>{q.message}</p>
      {#if q.details}<div class="details">{q.details}</div>{/if}
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
      {#if q.id}
        <label class="silence"
          ><input type="checkbox" bind:checked={silence} /> {t(q.info ? 'questions.dont_show' : 'questions.dont_ask')}</label
        >
      {/if}
      {#snippet footer()}
        {#if !q.info}<button class="btn" onclick={() => finish(null)}>{q.cancel ?? t('common.cancel')}</button>{/if}
        {#if q.alt}<button class="btn" onclick={() => finish('alt')}>{q.alt}</button>{/if}
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn primary" onclick={() => finish('ok')} autofocus>{q.ok ?? t('common.ok')}</button>
      {/snippet}
    </Dialog>
  {/key}
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
  .silence {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-s);
    color: var(--text-dim);
  }
</style>
