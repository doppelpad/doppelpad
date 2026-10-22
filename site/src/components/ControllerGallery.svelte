<script lang="ts">
  // The supported controllers as tabs, each with the app's front and back drawings and a
  // line on how far its support goes.
  import type { Controller } from '../lib/controllers';
  import ControllerDrawing from './ControllerDrawing.svelte';

  let {
    controllers,
    sides,
  }: { controllers: Controller[]; sides: { front: string; back: string; label: string } } = $props();

  let current = $state(0);
  let side = $state<'front' | 'back'>('front');
  const c = $derived(controllers[current]);

  function onTabKey(e: KeyboardEvent) {
    const step = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
    if (!step) return;
    e.preventDefault();
    current = (current + step + controllers.length) % controllers.length;
    (e.currentTarget as HTMLElement).parentElement?.querySelectorAll<HTMLElement>('[role="tab"]')[current]?.focus();
  }
</script>

<div class="gallery not-content">
  <div class="tabs" role="tablist" aria-label={sides.label}>
    {#each controllers as ctl, i (ctl.name)}
      <button
        role="tab"
        id="pad-tab-{i}"
        aria-selected={i === current}
        aria-controls="pad-panel"
        tabindex={i === current ? 0 : -1}
        onclick={() => (current = i)}
        onkeydown={onTabKey}
      >
        {ctl.name}
      </button>
    {/each}
  </div>

  <div class="panel" id="pad-panel" role="tabpanel" aria-labelledby="pad-tab-{current}">
    <div class="sides" role="group" aria-label={c.name}>
      <button aria-pressed={side === 'front'} onclick={() => (side = 'front')}>{sides.front}</button>
      <button aria-pressed={side === 'back'} onclick={() => (side = 'back')}>{sides.back}</button>
    </div>
    <ControllerDrawing
      device={side === 'front' ? c.front : c.back}
      names={c.names}
      label="{c.name}, {side === 'front' ? sides.front : sides.back}"
    />
    <p class="note">{c.note}</p>
  </div>
</div>

<style>
  .gallery {
    margin-top: 1.5rem;
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  button {
    font: inherit;
    font-size: var(--sl-text-sm);
    cursor: pointer;
    padding: 0.35rem 0.8rem;
    border-radius: 999px;
    border: 1px solid var(--sl-color-gray-5);
    background: transparent;
    color: var(--sl-color-gray-2);
    transition:
      background-color 0.15s,
      border-color 0.15s,
      color 0.15s;
  }
  button:hover {
    border-color: var(--sl-color-gray-3);
    color: var(--sl-color-white);
  }
  button[aria-selected='true'],
  button[aria-pressed='true'] {
    background: var(--sl-color-accent-low);
    border-color: var(--sl-color-accent);
    color: var(--sl-color-white);
  }
  button:focus-visible {
    outline: 2px solid var(--sl-color-accent);
    outline-offset: 2px;
  }
  .panel {
    position: relative;
    margin-top: 1rem;
    padding: 1rem;
    border: 1px solid var(--sl-color-gray-5);
    border-radius: 0.75rem;
    background: var(--sl-color-gray-6);
  }
  .sides {
    position: absolute;
    top: 1rem;
    right: 1rem;
    z-index: 2;
    display: flex;
    gap: 0.3rem;
  }
  .sides button {
    font-size: var(--sl-text-xs);
    padding: 0.2rem 0.65rem;
  }
  .note {
    margin: 0.5rem 0 0;
    font-size: var(--sl-text-sm);
    color: var(--sl-color-gray-3);
    text-align: center;
  }
  /* Narrow: the drawing fills the panel, so the side switch goes above it. */
  @media (max-width: 40rem) {
    .panel {
      padding: 0.75rem;
    }
    .sides {
      position: static;
      justify-content: flex-end;
      margin-bottom: 0.5rem;
    }
  }
</style>
