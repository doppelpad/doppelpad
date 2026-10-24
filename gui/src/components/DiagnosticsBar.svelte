<script lang="ts">
  // Problems in the open config: the validator's findings (from loading, saving and after each
  // change) and mappings that clash with a controller hotkey. A finding about a mapping or a
  // layer leads there when clicked.
  import { CircleAlert, TriangleAlert, ChevronDown } from '@lucide/svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { diagTarget } from '../lib/model/lints';
  import { app } from '../lib/state/app.svelte';
  import type { DiagMsg } from '../lib/types';

  let open = $state(false);

  const shown = $derived(app.problems.filter((d) => d.severity !== 'info'));
  const errors = $derived(shown.filter((d) => d.severity === 'error').length);
  const warnings = $derived(shown.length - errors);

  const targetOf = (d: DiagMsg) => (app.profile ? diagTarget(app.profile, d) : null);

  function go(d: DiagMsg) {
    const target = targetOf(d);
    if (!target) return;
    if (target.kind === 'radial') {
      app.view = 'radial';
      app.radialMenu = target.menu;
      app.radialSector = target.sector;
      return;
    }
    if (app.view === 'macro' || app.view === 'output' || app.view === 'analog' || app.view === 'radial') app.view = 'front';
    app.layer = target.layer;
    if (target.kind === 'mapping') app.select(target.trigger);
  }
</script>

{#if shown.length}
  <div class="diag" class:error={errors > 0}>
    <button class="summary" aria-expanded={open} onclick={() => (open = !open)}>
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
            {#if targetOf(d)}
              <button class="go" onclick={() => go(d)}>
                {#if d.path}<code>{d.path}</code>{/if}
                {d.message}
              </button>
            {:else}
              {#if d.path}<code>{d.path}</code>{/if}
              {d.message}
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .diag {
    border-bottom: 1px solid var(--line);
    background: color-mix(in srgb, var(--warning) 6%, transparent);
    color: var(--warning);
    padding: 6px 16px;
  }
  .diag.error {
    background: color-mix(in srgb, var(--danger) 7%, transparent);
    color: var(--danger);
  }
  .summary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-s);
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
  .go {
    text-align: left;
    color: inherit;
  }
  .go:hover {
    text-decoration: underline;
  }
  code {
    font-family: var(--mono);
    font-size: var(--fs-xs);
    color: var(--text-faint);
    margin-right: 6px;
  }
</style>
