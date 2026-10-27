<script lang="ts">
  // Problems the remapping service reports (devices, hiding, applying); English from Rust.
  // A controller another program held may be free again: Look for controllers retries at once.
  import { RefreshCw, TriangleAlert } from '@lucide/svelte';
  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import { tip } from '../../lib/tooltip.svelte';
</script>

{#if app.live?.warnings.length}
  <div class="service-warnings">
    <ul aria-label={t('diagnostics.service')}>
      {#each app.live.warnings as w, i (i)}
        <li><TriangleAlert size={13} /><span>{w}</span></li>
      {/each}
    </ul>
    <button class="btn" onclick={() => app.attempt(api.rescanDevices())} use:tip={t('diagnostics.rescan_hint')}>
      <RefreshCw size={13} />
      {t('diagnostics.rescan')}
    </button>
  </div>
{/if}

<style>
  .service-warnings {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 16px;
    background: color-mix(in srgb, var(--warning) 6%, transparent);
    border-top: 1px solid var(--line);
    font-size: var(--fs-s);
  }
  ul {
    flex: 1;
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-height: 88px;
    overflow-y: auto;
    user-select: text;
  }
  .service-warnings button {
    flex: none;
    padding: 3px 10px;
  }
  .service-warnings li {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    color: var(--text-dim);
  }
  .service-warnings li :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--warning);
  }
</style>
