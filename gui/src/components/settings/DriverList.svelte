<script module lang="ts">
  import type { LiveState } from '../../lib/types';

  /** The drivers Doppelpad uses and whether each is installed (none before the first live state). */
  export function driverStates(live: LiveState | null) {
    return live
      ? [
          { key: 'virtual_pads' as const, state: live.drivers.virtual_pads },
          { key: 'device_hiding' as const, state: live.drivers.device_hiding },
        ]
      : [];
  }
</script>

<script lang="ts">
  // The drivers and whether they are installed. `icon` (settings): a green or warning icon,
  // "missing" at the right and the error as tooltip; `row` (welcome guide): a missing driver's
  // row in the warning colour, with a download button.
  import { CircleAlert, CircleCheck } from '@lucide/svelte';
  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import { tip } from '../../lib/tooltip.svelte';

  let { tone }: { tone: 'icon' | 'row' } = $props();

  const drivers = $derived(driverStates(app.live));
</script>

{#each drivers as d (d.key)}
  <div class="driver {tone}" class:missing={!d.state.available} use:tip={tone === 'icon' ? (d.state.error ?? '') : undefined}>
    {#if d.state.available}<CircleCheck size={15} />{:else}<CircleAlert size={15} />{/if}
    <span class="name">{t(`drivers.${d.key}`)}</span>
    {#if !d.state.available}
      {#if tone === 'icon'}
        <span class="state">{t('drivers.missing')}</span>
      {:else}
        <button class="btn" onclick={() => app.attempt(api.openDriverPage(d.key))}>{t('drivers.download')}</button>
      {/if}
    {/if}
  </div>
{/each}

<style>
  .driver {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .icon {
    color: var(--text);
  }
  .icon :global(svg) {
    color: var(--ok);
  }
  .icon.missing :global(svg) {
    color: var(--warning);
  }
  .state {
    margin-left: auto;
    color: var(--warning);
    font-size: var(--fs-s);
  }
  .row.missing {
    color: var(--warning);
  }
  .row .name {
    flex: 1;
  }
</style>
