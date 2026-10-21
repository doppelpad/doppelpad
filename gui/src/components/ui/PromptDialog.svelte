<script lang="ts">
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';

  let value = $state('');
  let input: HTMLInputElement | HTMLTextAreaElement | undefined = $state();

  $effect(() => {
    if (app.prompt) {
      value = app.prompt.value;
      queueMicrotask(() => input?.focus());
    }
  });

  function finish(ok: boolean) {
    const p = app.prompt;
    app.prompt = null;
    p?.resolve(ok ? value.trim() || null : null);
  }
</script>

{#if app.prompt}
  <div class="backdrop" role="presentation" onclick={() => finish(false)}>
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => {
        if (e.key === 'Escape') finish(false);
        // Enter in the field confirms; on a button it presses that button.
        if (e.key === 'Enter' && !app.prompt?.multiline && e.target === input) finish(true);
      }}
    >
      <h3>{app.prompt.title}</h3>
      {#if app.prompt.multiline}
        <textarea bind:this={input} bind:value rows="5"></textarea>
      {:else}
        <input bind:this={input} bind:value />
      {/if}
      <div class="buttons">
        <button class="btn" onclick={() => finish(false)}>{t('common.cancel')}</button>
        <button class="btn primary" onclick={() => finish(true)}>{t('common.ok')}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(5, 7, 10, 0.6);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .dialog {
    width: 420px;
    background: var(--bg-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow);
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  input,
  textarea {
    width: 100%;
    resize: vertical;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
