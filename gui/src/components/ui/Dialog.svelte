<script lang="ts">
  // A modal dialog over the window: a title row with an optional close button, the content,
  // and an optional footer. Not a native <dialog>: its top layer would cover the window's
  // caption buttons, which stay above backdrops on purpose (WindowControls).
  import { X } from '@lucide/svelte';
  import { onMount, type Snippet } from 'svelte';
  import { dialogStack as stack } from '../../lib/dom';
  import { t } from '../../lib/i18n/index.svelte';
  import { tip } from '../../lib/tooltip.svelte';

  let {
    title,
    icon,
    onclose,
    closeLabel,
    showClose = true,
    backdropClose = true,
    escapeClose = true,
    width = 460,
    gap = 12,
    scroll = false,
    level = 'dialog',
    children,
    footer,
  }: {
    title: string;
    /** Shown before the title. */
    icon?: Snippet;
    onclose?: () => void;
    /** Tooltip of the close button (default "Close"). */
    closeLabel?: string;
    showClose?: boolean;
    backdropClose?: boolean;
    escapeClose?: boolean;
    /** In px. */
    width?: number;
    /** Between the parts of the content, in px. */
    gap?: number;
    /** The dialog scrolls when it is taller than the window. */
    scroll?: boolean;
    /** The welcome guide sits below the other dialogs. */
    level?: 'dialog' | 'guide';
    children: Snippet;
    footer?: Snippet;
  } = $props();

  const id = $props.id();
  const me = Symbol();
  let box: HTMLDivElement;

  onMount(() => {
    stack.push(me);
    const before = document.activeElement as HTMLElement | null;
    (box.querySelector<HTMLElement>('[autofocus]') ?? box).focus();
    return () => {
      stack.splice(stack.indexOf(me), 1);
      before?.focus?.();
    };
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !escapeClose || !onclose || stack.at(-1) !== me) return;
    e.preventDefault();
    onclose();
  }
</script>

<svelte:window {onkeydown} />

<div
  class="backdrop"
  class:guide={level === 'guide'}
  role="presentation"
  onclick={(e) => backdropClose && e.target === e.currentTarget && onclose?.()}
>
  <div
    class="dialog"
    class:scroll
    role="dialog"
    aria-modal="true"
    aria-labelledby="{id}-title"
    tabindex="-1"
    style:width="{width}px"
    style:gap="{gap}px"
    bind:this={box}
  >
    <div class="head">
      <h3 id="{id}-title">
        {#if icon}{@render icon()}{/if}
        {title}
      </h3>
      {#if showClose && onclose}
        <button class="icon-btn" onclick={onclose} use:tip={closeLabel ?? t('common.close')}><X size={17} /></button>
      {/if}
    </div>
    {@render children()}
    {#if footer}
      <div class="footer">{@render footer()}</div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--backdrop);
    display: grid;
    place-items: center;
    z-index: var(--z-dialog);
  }
  .backdrop.guide {
    z-index: var(--z-guide);
  }
  .dialog {
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 48px);
    background: var(--bg-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow);
    padding: 18px 20px 22px;
    display: flex;
    flex-direction: column;
    outline: none;
  }
  .dialog.scroll {
    overflow-y: auto;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
