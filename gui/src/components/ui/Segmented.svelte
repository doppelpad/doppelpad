<script module lang="ts">
  export interface SegmentedOption<T> {
    value: T;
    label: string;
    title?: string;
    /** A dot under the label, lit when true (zones in use); none when left out. */
    marked?: boolean;
    /** A count after the label. */
    badge?: number;
  }
</script>

<script lang="ts" generics="T extends string | null">
  // One choice out of a few, as a row of radio buttons. `inline` and `fill` join the buttons in
  // one frame (as wide as their labels, or sharing the width); `chips` are separate buttons.
  let {
    options,
    value,
    onchange,
    label,
    disabled = false,
    variant = 'inline',
    size = 'm',
    hover = true,
    class: cls = '',
  }: {
    options: readonly SegmentedOption<T>[];
    value: T;
    onchange: (value: T) => void;
    /** The accessible name of the group. */
    label: string;
    disabled?: boolean;
    variant?: 'inline' | 'fill' | 'chips';
    /** `s`: inline with less padding, or small chips in two columns; `l`: a main choice (fill). */
    size?: 's' | 'm' | 'l';
    /** Unselected joined buttons light up under the pointer. */
    hover?: boolean;
    /** For the caller's layout (margins). */
    class?: string;
  } = $props();

  const marks = $derived(options.some((o) => o.marked !== undefined));
</script>

<div
  class={['seg', variant, size, cls, { joined: variant !== 'chips', hover, marks }]}
  role="radiogroup"
  aria-label={label}
>
  {#each options as o (o.value)}
    <button
      role="radio"
      aria-checked={value === o.value}
      class:btn={variant === 'chips' && size === 'm'}
      class:on={value === o.value}
      class:marked={o.marked}
      title={o.title}
      {disabled}
      onclick={() => onchange(o.value)}
    >
      {o.label}
      {#if o.badge}<span class="badge">{o.badge}</span>{/if}
      {#if o.marked !== undefined}<span class="dot"></span>{/if}
    </button>
  {/each}
</div>

<style>
  .joined {
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
    overflow: hidden;
  }
  .inline {
    display: flex;
  }
  .fill {
    display: grid;
    grid-auto-columns: 1fr;
    grid-auto-flow: column;
  }
  .joined button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    font-size: var(--fs-s);
    color: var(--text-dim);
  }
  .joined button + button {
    border-left: 1px solid var(--line);
  }
  .inline button {
    padding: 6px 12px;
  }
  .inline.s button {
    padding: 5px 12px;
  }
  .fill button {
    padding: 6px 4px;
  }
  .fill.l {
    border-radius: var(--radius-m);
  }
  .fill.l button {
    padding: 9px 6px;
    font-size: inherit;
  }
  .hover button:hover:not(:disabled, .on) {
    color: var(--text);
    background: var(--bg-2);
  }
  .joined button.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .fill.l button.on {
    font-weight: 600;
  }
  .marks button {
    position: relative;
    padding-bottom: 8px;
  }
  .dot {
    position: absolute;
    left: 50%;
    bottom: 3px;
    width: 4px;
    height: 4px;
    margin-left: -2px;
    border-radius: 50%;
  }
  .marked .dot {
    background: var(--accent);
  }
  .badge {
    min-width: 16px;
    padding: 0 5px;
    border-radius: var(--radius-pill);
    background: var(--bg-3);
    color: var(--text-dim);
    font-size: 10.5px;
    line-height: 16px;
    text-align: center;
  }
  .chips.m {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .chips.m button {
    font-size: var(--fs-s);
    padding: 4px 10px;
  }
  .chips.m button.on {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .chips.s {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
  }
  .chips.s button {
    padding: 5px 6px;
    border-radius: var(--radius-s);
    background: var(--bg-3);
    border: 1px solid var(--line);
    font-size: var(--fs-xs);
    white-space: nowrap;
  }
  .chips.s button:hover,
  .chips.s button.on {
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
