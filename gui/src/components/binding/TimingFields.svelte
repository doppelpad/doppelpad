<script lang="ts">
  // The mapping's own timing behind a disclosure: the activator's press time and the delay
  // between keys; empty fields use the config's values (shown as placeholders).
  import { ChevronRight } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import type { Mapping } from '../../lib/types';
  import { configTiming, ms, type EditMapping, type MappingTiming } from './edit';
  import MsInput from './MsInput.svelte';

  let { mapping, readOnly, edit }: { mapping: Mapping | undefined; readOnly: boolean; edit: EditMapping } = $props();

  let open = $state(false);
  const timing = $derived(configTiming());
  const fields = $derived<MappingTiming[]>(
    [
      app.activator === 'long' ? 'long_press_ms' : null,
      app.activator === 'double' ? 'double_tap_ms' : null,
      app.activator === 'triple' ? 'triple_tap_ms' : null,
      'key_delay_ms',
    ].filter(Boolean) as MappingTiming[],
  );
</script>

<button class="disclosure" onclick={() => (open = !open)}>
  <ChevronRight size={14} class={open ? 'rot' : ''} />
  {t('binding.advanced')}
</button>
{#if open}
  <div class="timing">
    {#each fields as f (f)}
      <label class="inline">
        <span class="tl">{t(`binding.timing.${f}`)}</span>
        <MsInput
          value={mapping?.[f]}
          placeholder={String(timing[f] ?? '')}
          disabled={readOnly}
          onchange={(v) => edit((m) => (m[f] = ms(v)))}
        />
      </label>
    {/each}
  </div>
{/if}

<style>
  .disclosure {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text-dim);
    align-self: flex-start;
    font-size: var(--fs-s);
  }
  .disclosure :global(.rot) {
    transform: rotate(90deg);
  }
  .timing {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .inline {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
  .tl {
    flex: 1;
    min-width: 150px;
  }
</style>
