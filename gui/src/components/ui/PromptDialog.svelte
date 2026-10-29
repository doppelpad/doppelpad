<script lang="ts">
  // The app's prompts for a text (`app.ask`), one at a time from the queue in
  // `lib/state/dialogs.svelte.ts`.
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import Dialog from './Dialog.svelte';

  const finish = (ok: boolean) => app.dialogs.answerPrompt(ok);
</script>

{#if app.dialogs.prompt}
  {@const p = app.dialogs.prompt}
  {#key p}
    <Dialog title={p.title} onclose={() => finish(false)} width={420} gap={14} showClose={false}>
      {#if p.multiline}
        <!-- svelte-ignore a11y_autofocus -->
        <textarea bind:value={p.value} rows="5" autofocus></textarea>
      {:else}
        <!-- Enter in the field confirms; on a button it presses that button. -->
        <!-- svelte-ignore a11y_autofocus -->
        <input bind:value={p.value} autofocus onkeydown={(e) => e.key === 'Enter' && finish(true)} />
      {/if}
      {#snippet footer()}
        <button class="btn" onclick={() => finish(false)}>{t('common.cancel')}</button>
        <button class="btn primary" onclick={() => finish(true)}>{t('common.ok')}</button>
      {/snippet}
    </Dialog>
  {/key}
{/if}

<style>
  input,
  textarea {
    width: 100%;
    resize: vertical;
  }
</style>
