// Device drawings with label layouts for the mapping view.
//
// SVG conventions (all drawings, see ../generate.mjs):
// - viewBox 0 0 1000 700, stroke-only line art in `currentColor`, so the colour of a
//   control follows the CSS `color` of its group (e.g. accent on hover / selection).
// - Every clickable part is a `<g data-control="…">` whose name is a profile control
//   (`A`, `DpadUp`, `Mouse:Left`, `Wheel:Up`, …). Its first child is a `.hit` shape
//   (fill="currentColor" fill-opacity="0"): it catches clicks inside the part and can be
//   tinted on hover by raising its fill-opacity.
// - Sticks carry `data-stick="Left" | "Right"` on the same group as `LS` / `RS`; the cap
//   is wrapped in `<g class="stick-cap">` so it can be moved to show live deflection.
// - `<g class="body">` (outline and decorative details) has pointer-events="none".
// - Back views (paddles) are drawn as seen through the device from the front, so the
//   left paddles are on the left.

/** A point in the drawing's viewBox coordinates. */
export type Point = [x: number, y: number];

/** Where a label chip lies relative to its attach point. */
export type LabelSide = 'left' | 'right' | 'top' | 'bottom';

export interface LayoutControl {
  /** Profile control name; matches a `data-control` group in the SVG. */
  control: string;
  /** Where the leader line touches the control (on its outline). */
  anchor: Point;
  /** Direction in which the chip extends from `labelAt`. */
  side: LabelSide;
  /**
   * Where the leader ends and the chip attaches: the chip's right-edge centre for
   * `left`, its left-edge centre for `right`, its bottom centre for `top` and its top
   * centre for `bottom`.
   */
  labelAt: Point;
  /** Bend points; the leader runs anchor → …via → labelAt. Straight when absent. */
  via?: Point[];
}

export interface DeviceLayout {
  id: string;
  /** Raw SVG markup; inline it so the `data-control` groups stay clickable. */
  svg: string;
  viewBox: [number, number, number, number];
  /** One entry per `data-control` group in the SVG. */
  controls: LayoutControl[];
}

/**
 * Largest chip (viewBox units) the layouts leave room for: chips up to this size
 * overlap neither each other nor the drawing, and no leader runs through a chip.
 */
export const CHIP_SIZE = { width: 150, height: 28 } as const;
