<script lang="ts">
  // A mapping's actions as chips: add one from the picker, remove one, or edit an action's
  // parameters below the chips. Mounted per control and activator, so the picker closes when
  // they change.
  import { Ban, Plus } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { hasParameters } from '../../lib/model/actions';
  import { actionLabel } from '../../lib/model/labels';
  import { isBlocked } from '../../lib/model/profile';
  import type { Action, Mapping } from '../../lib/types';
  import Chip from '../ui/Chip.svelte';
  import ActionParams from './ActionParams.svelte';
  import ActionPicker from './ActionPicker.svelte';
  import type { EditMapping } from './edit';
  import { tip } from '../../lib/tooltip.svelte';

  let { mapping, readOnly, edit }: { mapping: Mapping | undefined; readOnly: boolean; edit: EditMapping } = $props();

  let picking = $state(false);
  /** Action whose parameters are shown below the chips. */
  let editingIdx = $state<number | null>(null);

  function addAction(a: Action) {
    edit((m) => (m.actions = [...(m.actions ?? []), a]));
  }

  function removeAction(i: number) {
    edit((m) => (m.actions = (m.actions ?? []).filter((_, j) => j !== i)));
  }
</script>

{#if isBlocked(mapping)}
  <div class="blocked"><Ban size={14} /> {t('binding.unmapped')}</div>
{:else}
  <div class="chips">
    {#each mapping?.actions ?? [] as a, i (i)}
      <Chip active={editingIdx === i} onremove={readOnly ? undefined : () => removeAction(i)}>
        {#if hasParameters(a)}
          <button class="chip-label" use:tip={t('binding.edit_params')} onclick={() => (editingIdx = editingIdx === i ? null : i)}>
            {actionLabel(a)}
          </button>
        {:else}
          {actionLabel(a)}
        {/if}
      </Chip>
    {/each}
    {#if !readOnly}
      <button class="add" class:on={picking} onclick={() => (picking = !picking)}>
        <Plus size={13} />
        {t('binding.add_action')}
      </button>
    {/if}
  </div>
  {#if (mapping?.actions ?? []).length === 0 && !picking}
    <p class="muted">{t('binding.no_actions')}</p>
  {/if}
  {#if editingIdx !== null && mapping?.actions?.[editingIdx]}
    {@const idx = editingIdx}
    <div class="params-box">
      <ActionParams
        action={mapping.actions[idx]}
        disabled={readOnly}
        onchange={(a) => edit((m) => (m.actions = (m.actions ?? []).map((x, j) => (j === idx ? a : x))))}
      />
    </div>
  {/if}
  {#if picking}
    <ActionPicker onpick={addAction} onclose={() => (picking = false)} />
  {/if}
{/if}

<style>
  .blocked {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--warning);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip-label {
    color: inherit;
    font: inherit;
    text-decoration: underline dotted;
    text-underline-offset: 3px;
  }
  .add {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 5px 10px;
    border-radius: var(--radius-pill);
    border: 1px dashed var(--line-strong);
    color: var(--accent);
    font-weight: 500;
  }
  .add.on {
    border-style: solid;
    background: var(--accent-soft);
  }
  .muted {
    color: var(--text-faint);
    margin: 0;
  }
  .params-box {
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    background: var(--bg-1);
  }
</style>
