<script lang="ts">
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import Dialog from './Dialog.svelte';

  let value = $state('');

  $effect(() => {
    if (app.prompt) value = app.prompt.value;
  });

  function finish(ok: boolean) {
    const p = app.prompt;
    app.prompt = null;
    p?.resolve(ok ? value.trim() || null : null);
  }
</script>

{#if app.prompt}
  <Dialog title={app.prompt.title} onclose={() => finish(false)} width={420} gap={14} showClose={false}>
    {#if app.prompt.multiline}
      <!-- svelte-ignore a11y_autofocus -->
      <textarea bind:value rows="5" autofocus></textarea>
    {:else}
      <!-- Enter in the field confirms; on a button it presses that button. -->
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:value autofocus onkeydown={(e) => e.key === 'Enter' && finish(true)} />
    {/if}
    {#snippet footer()}
      <button class="btn" onclick={() => finish(false)}>{t('common.cancel')}</button>
      <button class="btn primary" onclick={() => finish(true)}>{t('common.ok')}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  input,
  textarea {
    width: 100%;
    resize: vertical;
  }
</style>
