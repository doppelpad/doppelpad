// The editor's views: which there are, in the toolbar's order, what each one shows, and the
// view that shows a place in the config.

import { parseControl } from './controls';
import { comboList } from './profile';
import type { Activator, Combo } from '../types';

export type View = 'front' | 'back' | 'keyboard' | 'mouse' | 'shortcuts' | 'macro' | 'output' | 'analog' | 'radial';

interface ViewInfo {
  /** Its tile in the toolbar: before the layer chips, after them, or none (the macro editor opens from a mapping). */
  tile: 'before' | 'after' | null;
  /** Shows what controls do: notes can stand in for the actions, and the live output opens over it. */
  controls: boolean;
  /** Shows the current layer, so a layer chosen elsewhere is seen in it. */
  layer: boolean;
  /** A control selected in it shows in the binding panel beside it. */
  selection: boolean;
}

/** Every view, the toolbar's tiles in their order. */
export const VIEWS: Record<View, ViewInfo> = {
  front: { tile: 'before', controls: true, layer: true, selection: true },
  back: { tile: 'before', controls: true, layer: true, selection: true },
  keyboard: { tile: 'before', controls: true, layer: true, selection: true },
  mouse: { tile: 'before', controls: true, layer: true, selection: true },
  analog: { tile: 'after', controls: false, layer: true, selection: true },
  shortcuts: { tile: 'after', controls: false, layer: true, selection: true },
  radial: { tile: 'after', controls: false, layer: false, selection: false },
  output: { tile: 'after', controls: false, layer: false, selection: true },
  macro: { tile: null, controls: false, layer: false, selection: false },
};

/** A view with a tile in the toolbar. */
export type ToolbarView = Exclude<View, 'macro'>;

/** The views with a tile in the toolbar, in its order (the bumpers step through them so). */
export const TOOLBAR_VIEWS = (Object.keys(VIEWS) as View[]).filter((v): v is ToolbarView => VIEWS[v].tile !== null);

/** The pages of the analog view, in their order. */
export const ANALOG_TABS = ['left', 'right', 'triggers', 'gyro', 'touchpad', 'routes'] as const;
export type AnalogTab = (typeof ANALOG_TABS)[number];

/** The pages of the config settings, in the order of the list on the left. */
export const OUTPUT_PAGES = ['controller', 'input', 'timing', 'feedback', 'crosshairs'] as const;
export type OutputPage = (typeof OUTPUT_PAGES)[number];

/** A place in the config the editor can show: a mapping, a layer, or a radial menu (and a sector of it). */
export type Place =
  | { kind: 'mapping'; layer: number; trigger: Combo; activator: Activator }
  | { kind: 'layer'; layer: number }
  | { kind: 'radial'; menu: number; sector: number | null };

/**
 * The view that shows a trigger: shortcuts for combos, the keyboard or mouse for theirs, else
 * the controller (the side shown now, the front from other views).
 */
export function viewFor(trigger: Combo, current: View): 'front' | 'back' | 'keyboard' | 'mouse' | 'shortcuts' {
  const list = comboList(trigger);
  if (list.length > 1) return 'shortcuts';
  const p = parseControl(list[0] ?? '');
  // `null`: mouse motion without a direction.
  if (!p || p.kind === 'mouse' || p.kind === 'wheel' || p.kind === 'mouse_move' || p.kind === 'mouse_zone') return 'mouse';
  if (p.kind === 'key') return 'keyboard';
  return current === 'back' ? 'back' : 'front';
}
