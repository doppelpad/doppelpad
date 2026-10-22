<script lang="ts">
  // The activators as icon tabs (a dot on those with a mapping) and what the chosen one means.
  import { t } from '../../lib/i18n/index.svelte';
  import { ACTIVATORS, activatorOf, effectiveMappings } from '../../lib/model/profile';
  import { app } from '../../lib/state/app.svelte';
  import type { Activator, Combo } from '../../lib/types';
  import ActivatorIcon from './ActivatorIcon.svelte';

  let { combo }: { combo: Combo } = $props();

  /** Activators that have a mapping (own or inherited) for the control. */
  const used = $derived(new Set((app.profile ? effectiveMappings(app.profile, app.layer, combo).mappings : []).map(activatorOf)));
</script>

<div class="activators" role="radiogroup" aria-label={t('activators.title')}>
  {#each ACTIVATORS as a (a)}
    <button
      role="radio"
      aria-checked={app.activator === a}
      class="act"
      class:active={app.activator === a}
      class:used={used.has(a)}
      title={t(`activators.${a}`)}
      onclick={() => (app.activator = a as Activator)}
    >
      <ActivatorIcon activator={a} />
      <span class="dot"></span>
    </button>
  {/each}
</div>
<div class="act-desc">
  <strong>{t(`activators.${app.activator}`)}</strong> — {t(`activators.desc.${app.activator}`)}
</div>

<style>
  .activators {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 4px;
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    padding: 4px;
  }
  .act {
    position: relative;
    display: grid;
    place-items: center;
    height: 38px;
    border-radius: var(--radius-s);
    color: var(--text-faint);
  }
  .act:hover {
    color: var(--text);
    background: var(--bg-2);
  }
  .act.used {
    color: var(--text-dim);
  }
  .act.active {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .act .dot {
    position: absolute;
    bottom: 4px;
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: transparent;
  }
  .act.used .dot {
    background: var(--accent);
  }
  .act-desc {
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
  .act-desc strong {
    color: var(--text);
    font-weight: 600;
  }
</style>
