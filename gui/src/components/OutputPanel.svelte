<script lang="ts">
  // The config's settings, a page at a time: the virtual controller and how input passes,
  // keyboard and mouse, timing, light bar and rumble, crosshairs. The list on the left picks
  // the page, as in the app's settings.
  import { Crosshair, Gamepad2, Keyboard, Lightbulb, Timer } from '@lucide/svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { OUTPUT_PAGES as PAGE_IDS, type OutputPage } from '../lib/model/views';
  import { app } from '../lib/state/app.svelte';
  import CrosshairSettings from './crosshair/CrosshairSettings.svelte';
  import InputSettings from './output/InputSettings.svelte';
  import LightSettings from './output/LightSettings.svelte';
  import MouseSettings from './output/MouseSettings.svelte';
  import ShakeSettings from './output/ShakeSettings.svelte';
  import TimingSettings from './output/TimingSettings.svelte';
  import VibrationSettings from './output/VibrationSettings.svelte';
  import VirtualPadSettings from './output/VirtualPadSettings.svelte';

  const ICONS: Record<OutputPage, typeof Gamepad2> = {
    controller: Gamepad2,
    input: Keyboard,
    timing: Timer,
    feedback: Lightbulb,
    crosshairs: Crosshair,
  };
  const OUTPUT_PAGES = PAGE_IDS.map((id) => ({ id, icon: ICONS[id] }));

  const id = $props.id();
  const page = $derived(app.outputPage);

  /** Up and down (Home, End) move through the pages, as in any vertical tab list. */
  function onListKey(e: KeyboardEvent) {
    const i = OUTPUT_PAGES.findIndex((p) => p.id === page);
    const to = { ArrowDown: i + 1, ArrowUp: i - 1, Home: 0, End: OUTPUT_PAGES.length - 1 }[e.key];
    if (to === undefined) return;
    e.preventDefault();
    const next = OUTPUT_PAGES[(to + OUTPUT_PAGES.length) % OUTPUT_PAGES.length]!;
    app.outputPage = next.id;
    document.getElementById(`${id}-tab-${next.id}`)?.focus();
  }
</script>

<div class="output">
  <h2>{t('views.output')}</h2>
  <div class="layout">
    <div class="pages" role="tablist" aria-orientation="vertical" tabindex="-1" onkeydown={onListKey}>
      {#each OUTPUT_PAGES as p (p.id)}
        <button
          type="button"
          role="tab"
          id="{id}-tab-{p.id}"
          aria-selected={page === p.id}
          aria-controls="{id}-page"
          tabindex={page === p.id ? 0 : -1}
          class:on={page === p.id}
          onclick={() => (app.outputPage = p.id)}
        >
          <p.icon size={16} />
          <span class="name">{t(`output.pages.${p.id}`)}</span>
        </button>
      {/each}
    </div>

    {#key page}
      <div class="page" role="tabpanel" id="{id}-page" aria-labelledby="{id}-tab-{page}" tabindex="-1">
        {#if page === 'controller'}
          <VirtualPadSettings />
          <ShakeSettings />
        {:else if page === 'input'}
          <InputSettings />
          <MouseSettings />
        {:else if page === 'timing'}
          <TimingSettings />
        {:else if page === 'feedback'}
          <LightSettings />
          <VibrationSettings />
        {:else}
          <CrosshairSettings />
        {/if}
      </div>
    {/key}
  </div>
</div>

<style>
  .output {
    max-width: 860px;
    margin: 0 auto;
    padding: 28px 28px 40px;
  }
  h2 {
    margin: 0 0 18px;
    font-size: 18px;
    font-weight: 600;
  }
  .layout {
    display: grid;
    grid-template-columns: 184px minmax(0, 1fr);
    gap: 28px;
    align-items: start;
  }
  .pages {
    position: sticky;
    top: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-right: 12px;
    border-right: 1px solid var(--line);
    outline: none;
  }
  .pages button {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: var(--radius-m);
    color: var(--text-dim);
    text-align: left;
  }
  .pages button:hover:not(.on) {
    color: var(--text);
    background: var(--bg-2);
  }
  .pages button.on {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .name {
    flex: 1;
  }
  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
    outline: none;
  }
  /* The page's first heading needs no line or space above it. */
  .page > :global(.section-title:first-child) {
    margin-top: 0;
    padding-top: 0;
    border-top: none;
  }
</style>
