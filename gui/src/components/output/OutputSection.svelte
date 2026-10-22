<script lang="ts">
  // A section of the output page with number fields: a line above the title, a hint, the
  // fields in a grid, and whatever follows them (the children).
  import type { Snippet } from 'svelte';

  let { title, hint, fields, children }: { title: string; hint: string; fields: Snippet; children?: Snippet } = $props();
</script>

<div class="section-title rule">{title}</div>
<p class="hint">{hint}</p>
<div class="fields">{@render fields()}</div>
{@render children?.()}

<style>
  .rule {
    margin-top: 10px;
    padding-top: 16px;
    border-top: 1px solid var(--line);
  }
  .hint {
    margin: -6px 0 0;
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 8px 20px;
  }
</style>
