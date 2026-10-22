<script lang="ts">
  // An analog editor's two columns: the settings, and their previews at the right; one column
  // when the analog page is narrow.
  import type { Snippet } from 'svelte';

  let { controls, previews }: { controls: Snippet; previews: Snippet } = $props();
</script>

<div class="columns">
  <div class="controls">{@render controls()}</div>
  <div class="previews">{@render previews()}</div>
</div>

<style>
  .columns {
    display: grid;
    grid-template-columns: minmax(240px, 1fr) minmax(260px, 320px);
    gap: 28px;
    align-items: start;
  }
  @container analog (max-width: 620px) {
    .columns {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .controls {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }
  .previews {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
</style>
