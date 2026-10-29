// The rows of a menu (ui/Menu, ui/ContextMenu, ui/MenuList), for the code that builds menus.

import type { Component } from 'svelte';

export interface MenuItem {
  label: string;
  onclick?: () => void;
  danger?: boolean;
  disabled?: boolean;
  checked?: boolean;
  separator?: boolean;
  /** A caption over the items after it (the label), not an item. */
  heading?: boolean;
  /** CSS color for a dot in front of the label. */
  color?: string;
  /** An icon in front of the label. */
  icon?: Component<{ size?: number }>;
  /** Dim text at the end of the row, e.g. a battery level. */
  detail?: string;
  /** Colors the detail: low or critical. */
  tone?: 'warning' | 'danger';
}
