<script lang="ts" module>
  import type { Component } from 'svelte';

  /** A choice of a Select. */
  export interface SelectChoice<T> {
    value: T;
    label: string;
    disabled?: boolean;
    /** Dim text at the end of the row. */
    detail?: string;
    /** CSS colour of a dot in front of the label. */
    color?: string;
    icon?: Component<{ size?: number }>;
  }

  /** A choice, a line between groups of them, or a group's caption. */
  export type SelectOption<T> = SelectChoice<T> | { separator: true } | { heading: string };

  const isChoice = <T,>(o: SelectOption<T>): o is SelectChoice<T> => 'value' in o;
</script>

<script lang="ts" generics="T">
  // A drop-down list in the app's look instead of the webview's native one: a field that shows
  // the chosen option and opens the list of options under it (see Menu).
  import { ChevronDown } from '@lucide/svelte';
  import type { MenuItem } from '../../lib/menu';
  import { nextChoice } from '../../lib/padnav';
  import Menu from './Menu.svelte';

  let {
    value,
    options,
    onchange,
    label,
    title = '',
    placeholder = '',
    disabled = false,
    size = 'm',
    wide = false,
    class: cls = '',
  }: {
    value: T;
    options: readonly SelectOption<T>[];
    onchange: (value: T) => void;
    /** The accessible name, when no visible label names the field. */
    label?: string;
    /** A tooltip that says more than the chosen option. */
    title?: string;
    /** Shown while no option has the value. */
    placeholder?: string;
    disabled?: boolean;
    /** `s`: less padding, for a row of small fields. */
    size?: 's' | 'm';
    /** As wide as the place it is in. */
    wide?: boolean;
    /** For the caller's layout (width, margins). */
    class?: string;
  } = $props();

  const current = $derived(options.filter(isChoice).find((o) => o.value === value));

  const items = $derived<MenuItem[]>(
    options.map((o) =>
      isChoice(o)
        ? {
            label: o.label,
            detail: o.detail,
            color: o.color,
            icon: o.icon,
            disabled: o.disabled,
            checked: o.value === value,
            onclick: () => o.value !== value && onchange(o.value),
          }
        : o,
    ),
  );

  /** The controller steps the value without opening the list. */
  function step(dir: 1 | -1) {
    if (disabled) return;
    const next = nextChoice(options.filter(isChoice), value, dir);
    if (next !== undefined) onchange(next);
  }
</script>

<Menu {items} {title} {disabled} {wide} {step} label={label && `${label}: ${current?.label ?? placeholder}`} class="select {size} {cls}">
  {#snippet trigger()}
    {#if current?.color}<span class="dot" style:background={current.color}></span>{/if}
    {#if current?.icon}<span class="icon"><current.icon size={14} /></span>{/if}
    <span class="value" class:placeholder={!current}>{current?.label ?? placeholder}</span>
    <ChevronDown size={14} class="chevron" />
  {/snippet}
</Menu>

<style>
  /* Looks like the text fields (base.css). */
  :global(.trigger.select) {
    gap: 6px;
    min-width: 120px;
    max-width: 100%;
    padding: 6px 8px 6px 10px;
    background: var(--bg-2);
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
    color: var(--text);
    text-align: left;
    transition: border-color 0.12s;
  }
  /* At the right of a settings row. */
  :global(.trigger.select.setting) {
    min-width: 180px;
  }
  :global(.trigger.select.s) {
    min-width: 0;
    padding: 3px 6px 3px 8px;
  }
  :global(.trigger.select:hover:not(:disabled)) {
    border-color: var(--line-strong);
  }
  :global(.trigger.select:focus-visible),
  :global(.trigger.select.open) {
    border-color: var(--accent);
    outline: none;
  }
  :global(.trigger.select:disabled) {
    opacity: 0.5;
  }
  .value {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .placeholder {
    color: var(--text-faint);
  }
  .icon {
    display: inline-flex;
    color: var(--text-dim);
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  :global(.trigger.select .chevron) {
    flex: none;
    color: var(--text-faint);
    transition: transform 0.12s;
  }
  :global(.trigger.select.open .chevron) {
    transform: rotate(180deg);
  }
</style>
