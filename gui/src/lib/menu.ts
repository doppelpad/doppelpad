// The rows of a menu (ui/Menu, ui/ContextMenu, ui/MenuList), for the code that builds menus.

import type { Component } from 'svelte';

/** A row that can be chosen. */
export interface MenuChoice {
  label: string;
  onclick?: () => void;
  danger?: boolean;
  disabled?: boolean;
  checked?: boolean;
  /** CSS color for a dot in front of the label. */
  color?: string;
  /** An icon in front of the label. */
  icon?: Component<{ size?: number }>;
  /** Dim text at the end of the row, e.g. a battery level. */
  detail?: string;
  /** Colors the detail: low or critical. */
  tone?: 'warning' | 'danger';
}

/** A row of a menu: a choice, a line between groups of them, or a caption over the choices after it. */
export type MenuItem = MenuChoice | { separator: true } | { heading: string };

export const isChoice = (item: MenuItem): item is MenuChoice => 'label' in item;
