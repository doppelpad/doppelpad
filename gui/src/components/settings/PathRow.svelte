<script lang="ts">
  // A file or folder path, with a button that shows it in Explorer.
  import { FolderOpen } from '@lucide/svelte';
  import { api } from '../../lib/api';
  import { app } from '../../lib/state/app.svelte';

  let { path, revealLabel }: { path: string; /** Tooltip of the button. */ revealLabel: string } = $props();
</script>

<div class="path">
  <code>{path}</code>
  <button class="icon-btn" title={revealLabel} onclick={() => path && app.attempt(api.reveal(path))}>
    <FolderOpen size={15} />
  </button>
</div>

<style>
  .path {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  code {
    flex: 1;
    font-family: var(--mono);
    font-size: var(--fs-s);
    color: var(--text-dim);
    background: var(--bg-2);
    border-radius: var(--radius-s);
    padding: 6px 8px;
    overflow-wrap: anywhere;
    user-select: text;
  }
</style>
