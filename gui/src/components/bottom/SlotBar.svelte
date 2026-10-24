<script lang="ts">
  // Applying the open config to a slot (the split button picks which) and the four slot
  // circles: switch to one, unload one. Together because hovering the apply button marks the
  // slot it fills.
  import { ChevronDown, X } from '@lucide/svelte';
  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { configName } from '../../lib/model/labels';
  import { app } from '../../lib/state/app.svelte';
  import Menu from '../ui/Menu.svelte';
  import { tip } from '../../lib/tooltip.svelte';

  const live = $derived(app.live);
  const set = $derived(app.slotSet);
  const slotNames = $derived(set.slots.map((p) => (p ? configName(app.library, p) : null)));

  const slotMenu = $derived(
    [0, 1, 2, 3].map((i) => ({
      label: `${t('slots.slot', { n: i + 1 })}${slotNames[i] ? ' — ' + slotNames[i] : ''}`,
      checked: app.applySlot === i,
      onclick: () => (app.applySlot = i),
    })),
  );
  const canApply = $derived(!!app.configPath && !app.readOnly);
</script>

<div class="slot-bar">
  <!-- Hovering it marks the slot it fills. -->
  <div class="apply" class:ready={canApply}>
    <button class="btn primary" disabled={!canApply} onclick={() => app.apply()}>
      {t('remap.apply_to', { n: app.applySlot + 1 })}
    </button>
    <Menu items={slotMenu} align="right" title={t('remap.pick_slot')} iconOnly>
      {#snippet trigger()}<span class="btn primary slot-pick"><ChevronDown size={14} /></span>{/snippet}
    </Menu>
  </div>
  <div class="slots">
    {#each [0, 1, 2, 3] as i (i)}
      <div class="slot-wrap">
        <button
          class="slot"
          class:filled={!!slotNames[i]}
          class:target={app.applySlot === i}
          class:active={set.active === i && live?.remap_on && !!slotNames[i]}
          class:failed={set.active === i &&
            live?.remap_on &&
            !!slotNames[i] &&
            !set.session &&
            (set.device === null || !!live?.devices.some((d) => d.id === set.device))}
          use:tip={slotNames[i] ?? t('slots.empty')}
          onclick={() => app.attempt(api.selectSlot(i, set.device))}
        >
          {i + 1}
        </button>
        {#if slotNames[i]}
          <button class="unload" use:tip={t('slots.unload')} onclick={() => app.attempt(api.clearSlot(i, set.device))}
            ><X size={10} /></button
          >
        {/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .slot-bar {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
  }
  .apply {
    display: inline-flex;
  }
  .apply > :global(.btn.primary) {
    border-radius: var(--radius-s) 0 0 var(--radius-s);
  }
  .slot-pick {
    padding: 0 7px;
    border-radius: 0 var(--radius-s) var(--radius-s) 0;
    border-left-color: color-mix(in srgb, var(--on-accent) 30%, transparent);
  }
  .slots {
    display: flex;
    gap: 8px;
  }
  .slot-wrap {
    position: relative;
  }
  .slot {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    border: 1px dashed var(--line-strong);
    color: var(--text-faint);
    font-weight: 600;
    transition:
      background 0.12s,
      border-color 0.12s,
      box-shadow 0.12s;
  }
  .slot:hover {
    border-color: var(--text-faint);
  }
  .slot.filled {
    border-style: solid;
    color: var(--text);
    background: var(--bg-2);
  }
  .slot.active {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .slot.active.failed {
    border-color: var(--warning);
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 12%, transparent);
  }
  .slot-bar:has(.apply.ready:hover, .apply.ready:focus-within) .slot.target {
    border-style: solid;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .unload {
    position: absolute;
    top: -4px;
    right: -4px;
    width: 15px;
    height: 15px;
    border-radius: 50%;
    background: var(--bg-3);
    border: 1px solid var(--line-strong);
    display: none;
    place-items: center;
    color: var(--text-dim);
  }
  .slot-wrap:hover .unload {
    display: grid;
  }
</style>
