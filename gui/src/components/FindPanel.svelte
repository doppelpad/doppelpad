<script lang="ts">
  // Find mappings (Ctrl+F): by output, control or note across all layers, or (from a key's
  // context menu in the keyboard view) the mappings that press one key. Up and down move
  // through the results, Enter or a click goes to one, Escape or a click elsewhere closes.
  import { Search, X } from '@lucide/svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { comboLabel } from '../lib/model/labels';
  import { mappingsPressing, searchMappings, viewFor, type Found } from '../lib/model/search';
  import { app } from '../lib/state/app.svelte';
  import LayerBadge from './ui/LayerBadge.svelte';
  import { tip } from '../lib/tooltip.svelte';

  let query = $state(app.find?.query ?? '');
  let box: HTMLDivElement | undefined = $state();
  let input: HTMLInputElement | undefined = $state();

  const key = $derived(app.find?.key ?? null);
  const results = $derived<Found[]>(
    !app.profile
      ? []
      : key
        ? mappingsPressing(app.profile, key, app.layerLabel)
        : searchMappings(app.profile, query, app.family, app.layerLabel),
  );

  $effect(() => {
    input?.focus();
  });

  function close() {
    app.find = null;
  }

  function go(f: Found) {
    app.view = viewFor(f.trigger, app.view);
    app.layer = f.layer;
    app.activator = f.activator;
    app.select(f.trigger);
    close();
  }

  function onkeydown(e: KeyboardEvent) {
    if (!box) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      close();
      return;
    }
    if (e.key === 'Enter' && document.activeElement === input && results[0]) {
      e.preventDefault();
      go(results[0]);
      return;
    }
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const rows = [...box.querySelectorAll<HTMLButtonElement>('.result')];
    const i = rows.indexOf(document.activeElement as HTMLButtonElement);
    const next = e.key === 'ArrowDown' ? i + 1 : i - 1;
    if (next < 0) input?.focus();
    else rows[Math.min(next, rows.length - 1)]?.focus();
  }

  function onpointerdown(e: PointerEvent) {
    if (box && !box.contains(e.target as Node)) close();
  }
</script>

<svelte:window onpointerdowncapture={onpointerdown} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="find" role="search" data-popover bind:this={box} {onkeydown}>
  <div class="field">
    <Search size={15} />
    {#if key}
      <span class="filter">
        {t('find.pressing', { key: comboLabel(`Key:${key}`, null) })}
        <button class="icon-btn" use:tip={t('find.all')} onclick={() => app.find && (app.find = { query })}><X size={13} /></button>
      </span>
    {:else}
      <input bind:this={input} bind:value={query} placeholder={t('find.placeholder')} aria-label={t('find.title')} />
    {/if}
    <button class="icon-btn" use:tip={t('common.close')} onclick={close}><X size={15} /></button>
  </div>
  {#if key || query.trim()}
    <div class="results" role="list">
      {#each results as f (f.index)}
        <button class="result" onclick={() => go(f)}>
          <LayerBadge layer={f.layer} />
          <span class="control">{comboLabel(f.trigger, app.family)}</span>
          {#if f.activator !== 'single'}<span class="act">{t(`activators.${f.activator}`)}</span>{/if}
          <span class="sum">{f.summary}</span>
          {#if f.description}<span class="desc">{f.description}</span>{/if}
        </button>
      {:else}
        <p class="none">{t('find.none')}</p>
      {/each}
    </div>
  {/if}
</div>

<style>
  .find {
    position: fixed;
    top: 58px;
    left: 50%;
    transform: translateX(-50%);
    width: min(520px, calc(100vw - 32px));
    z-index: var(--z-menu);
    background: var(--bg-2);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow);
    padding: 6px;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 4px 0 8px;
    color: var(--text-dim);
  }
  .field input {
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    padding: 6px 0;
  }
  .field input:focus {
    outline: none;
  }
  .filter {
    flex: 1;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text);
  }
  .results {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: min(420px, 60vh);
    overflow-y: auto;
    margin-top: 6px;
    border-top: 1px solid var(--line);
    padding-top: 6px;
  }
  .result {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: var(--radius-s);
    text-align: left;
    min-width: 0;
  }
  .result:hover,
  .result:focus-visible {
    background: var(--bg-3);
  }
  .control {
    font-weight: 600;
    white-space: nowrap;
  }
  .act {
    font-size: var(--fs-xs);
    color: var(--text-dim);
    white-space: nowrap;
  }
  .sum {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-dim);
  }
  .desc {
    font-size: var(--fs-xs);
    color: var(--text-faint);
    font-style: italic;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 30%;
  }
  .none {
    margin: 4px 8px;
    color: var(--text-faint);
  }
</style>
