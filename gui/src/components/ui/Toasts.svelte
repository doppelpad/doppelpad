<script lang="ts">
  import { app } from '../../lib/state/app.svelte';
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as toast (toast.id)}
    <div class="toast {toast.kind}">{toast.text}</div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 20px;
    bottom: 86px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: var(--z-toast);
    pointer-events: none;
  }
  .toast {
    max-width: 420px;
    padding: 10px 14px;
    border-radius: var(--radius-m);
    background: var(--bg-2);
    border: 1px solid var(--line-strong);
    border-left: 3px solid var(--accent);
    box-shadow: var(--shadow);
  }
  .toast.ok {
    border-left-color: var(--ok);
  }
  .toast.error {
    border-left-color: var(--danger);
  }
</style>
