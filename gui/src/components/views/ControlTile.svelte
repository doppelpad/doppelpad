<script lang="ts">
  // A control as a tile: its name and what it does here ("—" for nothing); marked when mapped,
  // selected or held down.
  import { controlMenu } from '../binding/controlMenu';
  import type { Combo } from '../../lib/types';

  let {
    name,
    summary,
    control,
    flagged = false,
    selected = false,
    pressed = false,
    onclick,
  }: {
    name: string;
    /** `hiddenNote`: a mapping has a note the tile does not show; it gets a small mark. */
    summary: { text: string; inherited: boolean; hiddenNote?: boolean } | null | undefined;
    /** The control the tile stands for: its context menu copies, pastes and clears it. */
    control: Combo;
    /** A mapping behind the tile has a warning. */
    flagged?: boolean;
    selected?: boolean;
    pressed?: boolean;
    onclick: () => void;
  } = $props();
</script>

<button
  class="tile"
  class:selected
  class:pressed
  class:mapped={!!summary}
  class:noted={!!summary?.hiddenNote}
  class:flagged
  {onclick}
  use:controlMenu={control}
>
  <span class="name">{name}</span>
  <span class="sum" class:inh={summary?.inherited}>{summary?.text ?? '—'}</span>
</button>

<style>
  .tile {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 9px 12px;
    text-align: left;
    border-radius: var(--radius-m);
    border: 1px solid var(--line);
    background: var(--bg-1);
    min-width: 0;
  }
  .tile:hover {
    border-color: var(--line-strong);
  }
  .tile.mapped {
    border-left: 3px solid var(--accent);
  }
  .tile.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .tile.pressed {
    box-shadow: 0 0 0 2px var(--accent);
  }
  .name {
    font-weight: 600;
  }
  .sum {
    color: var(--text-dim);
    font-size: var(--fs-s);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sum.inh {
    color: var(--text-faint);
    font-style: italic;
  }
  /* A warning about a mapping behind the tile (the diagnostics bar says which). */
  .tile.flagged::before {
    content: '';
    position: absolute;
    top: 4px;
    left: 4px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--warning);
  }
  /* A note the summary does not show: a small corner mark (Show notes shows it). */
  .tile.noted::after {
    content: '';
    position: absolute;
    top: 4px;
    right: 4px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-dim);
  }
</style>
