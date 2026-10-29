// Where a popup (a menu, a tooltip, a context menu) goes next to what it belongs to: below it,
// or above when it does not fit below and there is more room above, and always inside the
// bounds (the window, or a part of it).

export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface Placement {
  left: number;
  top: number;
  /** It opens above the anchor. */
  up: boolean;
  /** The room on that side; a taller popup scrolls. */
  maxHeight: number;
}

export interface PlaceOptions {
  /** Between the anchor and the popup. */
  gap?: number;
  /** Kept free between the popup and the bounds. */
  edge?: number;
  /** Lined up with the anchor's left edge, its centre or its right edge. */
  align?: 'start' | 'center' | 'end';
  /** The least `maxHeight` gets, however little room there is. */
  minHeight?: number;
}

/** The place of a popup of `size` at `anchor` (a box, or a point as an empty one) inside `bounds`. */
export function placePopup(
  anchor: Box,
  size: { width: number; height: number },
  bounds: Box,
  { gap = 4, edge = 8, align = 'start', minHeight = 0 }: PlaceOptions = {},
): Placement {
  const below = bounds.bottom - anchor.bottom - gap - edge;
  const above = anchor.top - bounds.top - gap - edge;
  const up = size.height > below && above > below;
  const maxHeight = Math.max(minHeight, up ? above : below);
  const top = up ? anchor.top - gap - Math.min(size.height, maxHeight) : anchor.bottom + gap;
  const x = align === 'start' ? anchor.left : align === 'end' ? anchor.right - size.width : (anchor.left + anchor.right - size.width) / 2;
  // Too wide for the bounds: it starts at their left edge.
  const left = Math.max(bounds.left + edge, Math.min(x, bounds.right - edge - size.width));
  return { left, top, up, maxHeight };
}

/** The window as bounds. */
export const windowBox = (): Box => ({ left: 0, top: 0, right: window.innerWidth, bottom: window.innerHeight });

/** A point as an anchor (a right click). */
export const pointBox = (x: number, y: number): Box => ({ left: x, top: y, right: x, bottom: y });
