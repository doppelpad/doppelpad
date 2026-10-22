<script lang="ts">
  // A control as a tile: its name and what it does here ("—" for nothing); marked when mapped,
  // selected or held down.
  import { controlMenu } from '../binding/controlMenu';
  import type { Combo } from '../../lib/types';

  let {
    name,
    summary,
    control,
    selected = false,
    pressed = false,
    onclick,
  }: {
    name: string;
    summary: { text: string; inherited: boolean } | null | undefined;
    /** The control the tile stands for: its context menu copies, pastes and clears it. */
    control: Combo;
    selected?: boolean;
    pressed?: boolean;
    onclick: () => void;
  } = $props();
</script>

<button class="tile" class:selected class:pressed class:mapped={!!summary} {onclick} use:controlMenu={control}>
  <span class="name">{name}</span>
  <span class="sum" class:inh={summary?.inherited}>{summary?.text ?? '—'}</span>
</button>

<style>
  .tile {
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
</style>
