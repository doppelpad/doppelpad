<script lang="ts">
  // Problems the validator found in the open config (from loading or saving).
  import { CircleAlert, TriangleAlert, ChevronDown } from '@lucide/svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';

  let open = $state(false);

  const shown = $derived(app.diagnostics.filter((d) => d.severity !== 'info'));
  const errors = $derived(shown.filter((d) => d.severity === 'error').length);
  const warnings = $derived(shown.length - errors);
</script>

{#if shown.length}
  <div class="diag" class:error={errors > 0}>
    <button class="summary" onclick={() => (open = !open)}>
      {#if errors}<CircleAlert size={15} />{:else}<TriangleAlert size={15} />{/if}
      {#if errors}{t('diagnostics.errors', { n: errors })}{/if}
      {#if errors && warnings}·{/if}
      {#if warnings}{t('diagnostics.warnings', { n: warnings })}{/if}
      <ChevronDown size={14} class={open ? 'flip' : ''} />
    </button>
    {#if open}
      <ul>
        {#each shown as d, i (i)}
          <li class={d.severity}>
            {#if d.path}<code>{d.path}</code>{/if}
            {d.message}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .diag {
    border-bottom: 1px solid var(--line);
    background: rgba(245, 158, 11, 0.06);
    color: var(--warning);
    padding: 6px 16px;
  }
  .diag.error {
    background: rgba(244, 63, 94, 0.07);
    color: var(--danger);
  }
  .summary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 500;
  }
  .summary :global(.flip) {
    transform: rotate(180deg);
  }
  ul {
    margin: 6px 0 2px;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-height: 140px;
    overflow-y: auto;
    color: var(--text-dim);
    user-select: text;
  }
  li.error {
    color: var(--danger);
  }
  code {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--text-faint);
    margin-right: 6px;
  }
</style>
