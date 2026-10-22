<script lang="ts">
  // One setting on a line: its name, and at the right the control it labels (the children).
  import type { Snippet } from 'svelte';

  let { label, children }: { label: string; children: Snippet } = $props();
</script>

<label class="row">
  <span>{label}</span>
  {@render children()}
</label>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
</style>
