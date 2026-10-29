// The app's own tooltips, instead of the webview's native `title` ones: `use:tip={text}` on
// an element shows `text` in one shared bubble (components/ui/Tooltip.svelte) after the
// pointer rests on it, or at once when it gets the keyboard focus. Once a tooltip has shown,
// the next one follows quickly while the pointer moves along a row of buttons.
// An element without any text of its own (an icon button) gets the text as its accessible
// name; one with text gets it as its description.

/** The tooltip on screen: its text and the box of the element it belongs to. */
export const tooltip = $state<{ current: { text: string; anchor: DOMRect; id: number } | null }>({ current: null });

/** Pointer rest before the first tooltip shows, and before the next one while they are "warm". */
const DELAY = 550;
const WARM_DELAY = 80;
/** How long after a tooltip hides the next one still comes quickly. */
const WARM_FOR = 400;

let timer: ReturnType<typeof setTimeout> | undefined;
let hiddenAt = -Infinity;
let shownFor: Element | null = null;
/** The element whose tooltip waits to show. */
let pendingFor: Element | null = null;
let ids = 0;

/** Show `node`'s tooltip after `delay`, with its text then. */
function show(node: Element, text: () => string, delay: number) {
  clearTimeout(timer);
  pendingFor = null;
  if (!text()) return;
  pendingFor = node;
  timer = setTimeout(() => {
    pendingFor = null;
    if (!node.isConnected || !text()) return;
    shownFor = node;
    tooltip.current = { text: text(), anchor: node.getBoundingClientRect(), id: ++ids };
  }, delay);
}

/** Hide the tooltip, also one waiting to show (only `node`'s, when given). */
export function hideTooltip(node?: Element) {
  if (!node || pendingFor === node) {
    clearTimeout(timer);
    pendingFor = null;
  }
  if (node && shownFor !== node) return;
  if (tooltip.current) hiddenAt = performance.now();
  tooltip.current = null;
  shownFor = null;
}

/** The element has text a screen reader reads: its own label or text content. */
function hasText(node: Element): boolean {
  return (node.textContent ?? '').trim() !== '';
}

export function tip(node: HTMLElement | SVGElement, text: string | null | undefined) {
  let current = text ?? '';
  /** The accessible name or description this action set (it removes only what it set). */
  let setAttr: 'aria-label' | 'aria-description' | null = null;

  function label() {
    if (setAttr) node.removeAttribute(setAttr);
    setAttr = null;
    if (!current) return;
    if (!hasText(node)) {
      if (node.hasAttribute('aria-label')) return;
      setAttr = 'aria-label';
    } else {
      setAttr = 'aria-description';
    }
    node.setAttribute(setAttr, current);
  }

  const textNow = () => current;
  const enter = () => show(node, textNow, performance.now() - hiddenAt < WARM_FOR || tooltip.current ? WARM_DELAY : DELAY);
  const leave = () => hideTooltip(node);
  const focus = () => {
    // Only for keyboard (and controller) focus; a click focuses too.
    if (node.matches(':focus-visible')) show(node, textNow, 0);
  };

  node.addEventListener('pointerenter', enter);
  node.addEventListener('pointerleave', leave);
  node.addEventListener('pointerdown', leave);
  node.addEventListener('focus', focus);
  node.addEventListener('blur', leave);
  label();

  return {
    update(next: string | null | undefined) {
      current = next ?? '';
      label();
      if (shownFor === node) {
        if (current) tooltip.current = { text: current, anchor: node.getBoundingClientRect(), id: ++ids };
        else hideTooltip(node);
      }
    },
    destroy() {
      node.removeEventListener('pointerenter', enter);
      node.removeEventListener('pointerleave', leave);
      node.removeEventListener('pointerdown', leave);
      node.removeEventListener('focus', focus);
      node.removeEventListener('blur', leave);
      hideTooltip(node);
    },
  };
}
