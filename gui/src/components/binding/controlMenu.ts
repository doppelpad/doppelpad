// Copy, paste and clear of what a control, a shortcut or a direction group does on the current
// layer: the actions behind the context menus of the controller drawing, the lists, the
// keyboard and mouse views, the shortcuts and the binding panel.

import { t } from '../../lib/i18n/index.svelte';
import {
  clearControl,
  clearGroup,
  copyControl,
  copyGroup,
  groupHasOwn,
  groupOf,
  hasOwn,
  pasteControl,
  pasteGroup,
  type GroupId,
} from '../../lib/model/clipboard';
import { SIDES } from '../../lib/model/controls';
import { comboLabel } from '../../lib/model/labels';
import { comboList } from '../../lib/model/profile';
import { app } from '../../lib/state/app.svelte';
import type { Combo, ControlName } from '../../lib/types';
import type { MenuItem } from '../../lib/menu';

const nameOf = (c: Combo) => comboLabel(c, app.family);
const groupName = (g: GroupId) => t(`clipboard.groups.${g}`);
const copiedName = () => {
  const c = app.clipboard;
  if (!c) return '';
  return c.kind === 'control' ? nameOf(c.source) : groupName(c.group);
};

function pasted(name: string, dropped = 0) {
  app.toast(dropped ? t('clipboard.pasted_dropped', { name, n: dropped }) : t('clipboard.pasted', { name }), 'ok');
}

export function copyAt(c: Combo) {
  if (!app.profile) return;
  app.clipboard = copyControl(app.profile, app.layer, c);
  app.toast(t('clipboard.copied', { name: nameOf(c) }), 'info');
}

/** Whether the clipboard holds something `pasteAt` can put on a control. */
export const canPaste = () => app.clipboard?.kind === 'control' && !!app.profile && !app.readOnly;

export function pasteAt(c: Combo) {
  const copied = app.clipboard;
  if (copied?.kind !== 'control' || !canPaste()) return;
  app.edit((p) => pasteControl(p, app.layer, c, copied));
  pasted(nameOf(c));
}

export const canClear = (c: Combo) => !!app.profile && !app.readOnly && hasOwn(app.profile, app.layer, c);

export function clearAt(c: Combo) {
  if (canClear(c)) app.edit((p) => clearControl(p, app.layer, c));
}

export function copyGroupAt(g: GroupId) {
  if (!app.profile) return;
  app.clipboard = copyGroup(app.profile, app.layer, g);
  app.toast(t('clipboard.copied', { name: groupName(g) }), 'info');
}

export const canPasteGroup = () => app.clipboard?.kind === 'group' && !!app.profile && !app.readOnly;

export function pasteGroupAt(g: GroupId) {
  const copied = app.clipboard;
  if (copied?.kind !== 'group' || !canPasteGroup()) return;
  let dropped = 0;
  app.edit((p) => {
    const out = pasteGroup(p, app.layer, g, copied);
    dropped = out.dropped;
    return out.profile;
  });
  pasted(groupName(g), dropped);
}

export const canClearGroup = (g: GroupId) => !!app.profile && !app.readOnly && groupHasOwn(app.profile, app.layer, g);

export function clearGroupAt(g: GroupId) {
  if (canClearGroup(g)) app.edit((p) => clearGroup(p, app.layer, g));
}

/** Copy, paste and clear of one control or shortcut; `named` puts its name into the labels. */
export function controlItems(c: Combo, named = false): MenuItem[] {
  const name = nameOf(c);
  const from = copiedName();
  return [
    { label: named ? t('clipboard.copy_named', { name }) : t('clipboard.copy'), onclick: () => copyAt(c), disabled: !app.profile },
    {
      label: app.clipboard?.kind === 'control' ? t('clipboard.paste_from', { name: from }) : t('clipboard.paste'),
      onclick: () => pasteAt(c),
      disabled: !canPaste(),
    },
    {
      label: named ? t('clipboard.clear_named', { name }) : t('clipboard.clear'),
      onclick: () => clearAt(c),
      danger: true,
      disabled: !canClear(c),
    },
  ];
}

/** Copy, paste and clear of every direction of a group. */
export function groupItems(g: GroupId): MenuItem[] {
  return [
    { label: t('clipboard.copy_group'), onclick: () => copyGroupAt(g), disabled: !app.profile },
    {
      label: app.clipboard?.kind === 'group' ? t('clipboard.paste_group_from', { name: copiedName() }) : t('clipboard.paste_group'),
      onclick: () => pasteGroupAt(g),
      disabled: !canPasteGroup(),
    },
    { label: t('clipboard.clear_group'), onclick: () => clearGroupAt(g), danger: true, disabled: !canClearGroup(g) },
  ];
}

/** The direction group a drawn control stands for: a stick's click its stick, the touchpad's click the touchpad. */
export function drawnGroup(c: ControlName): GroupId | null {
  if (c === SIDES.left.click) return 'LeftStick';
  if (c === SIDES.right.click) return 'RightStick';
  if (c === 'Touchpad' && !app.profile?.touchpad?.split) return 'Touch';
  return null;
}

/** The context menu of a control or shortcut: its own entries, and its direction group's. */
export function menuItems(c: Combo): MenuItem[] {
  const list = comboList(c);
  const single = list.length === 1 ? list[0]! : null;
  const stands = single ? drawnGroup(single) : null;
  const member = single ? (groupOf(single)?.group ?? null) : null;
  const group = stands ?? member;
  const own = controlItems(c, !!stands);
  const items = group ? [...own, { label: '', separator: true }, ...groupItems(group)] : own;
  // A key of the keyboard view: which mappings press it.
  if (single?.startsWith('Key:')) {
    const key = single.slice(4);
    items.push(
      { label: '', separator: true },
      { label: t('find.who_presses'), onclick: () => (app.find = { query: '', key }), disabled: !app.profile },
    );
  }
  return items;
}

/** A key press that asks for the context menu: the menu key or Shift+F10. */
export const isMenuKey = (e: KeyboardEvent) => e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10');

/**
 * Gives an element the context menu of a control (`use:controlMenu={combo}`): on a right click
 * at the pointer, on the menu key or Shift+F10 below the element.
 */
export function controlMenu(node: HTMLElement, combo: Combo) {
  let current = combo;
  /** When a key press opened the menu: the browser's own context menu event that follows is ignored. */
  let byKey = -Infinity;
  const onContextMenu = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (performance.now() - byKey < 500) return;
    // Some browsers send the keyboard's context menu event without a pointer position.
    const keyboard = e.clientX === 0 && e.clientY === 0;
    app.openContextMenu(menuItems(current), keyboard ? null : { x: e.clientX, y: e.clientY }, node);
  };
  const onKeydown = (e: KeyboardEvent) => {
    if (!isMenuKey(e)) return;
    e.preventDefault();
    e.stopPropagation();
    byKey = performance.now();
    app.openContextMenu(menuItems(current), null, node);
  };
  node.addEventListener('contextmenu', onContextMenu);
  node.addEventListener('keydown', onKeydown);
  return {
    update(next: Combo) {
      current = next;
    },
    destroy() {
      node.removeEventListener('contextmenu', onContextMenu);
      node.removeEventListener('keydown', onKeydown);
    },
  };
}
