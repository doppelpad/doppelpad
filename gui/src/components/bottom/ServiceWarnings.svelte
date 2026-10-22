<script lang="ts">
  // Problems the remapping service reports (devices, hiding, applying); English from Rust.
  import { TriangleAlert } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
</script>

{#if app.live?.warnings.length}
  <ul class="service-warnings" aria-label={t('diagnostics.service')}>
    {#each app.live.warnings as w, i (i)}
      <li><TriangleAlert size={13} /><span>{w}</span></li>
    {/each}
  </ul>
{/if}

<style>
  .service-warnings {
    margin: 0;
    padding: 6px 16px;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-height: 88px;
    overflow-y: auto;
    background: color-mix(in srgb, var(--warning) 6%, transparent);
    border-top: 1px solid var(--line);
    font-size: var(--fs-s);
    user-select: text;
  }
  .service-warnings li {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    color: var(--text-dim);
  }
  .service-warnings :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--warning);
  }
</style>
