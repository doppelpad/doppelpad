// What the keyboard shortcuts do (the table is in `lib/state/shortcuts.ts`).

import { groupOf, type GroupId } from '../lib/model/clipboard';
import { comboList, layerIds } from '../lib/model/profile';
import { app } from '../lib/state/app.svelte';
import type { ShortcutId } from '../lib/state/shortcuts';
import type { Combo } from '../lib/types';
import { clearAt, copyAt, copyGroupAt, drawnGroup, pasteAt, pasteGroupAt } from './binding/controlMenu';

/** The direction group the selection stands for: a whole stick in the side panel, or the selected control's group. */
function selectedGroup(c: Combo | null): GroupId | null {
  if (app.stick) return app.stick === 'left' ? 'LeftStick' : 'RightStick';
  const list = c ? comboList(c) : [];
  if (list.length !== 1) return null;
  return drawnGroup(list[0]!) ?? groupOf(list[0]!)?.group ?? null;
}

/** Run a shortcut; `false` when it has nothing to act on (the key then keeps its usual effect). */
export function runShortcut(id: ShortcutId, digit?: number): boolean {
  const c = app.selected;
  switch (id) {
    case 'save':
      if (app.dirty) app.save();
      return true;
    case 'undo':
      app.undo();
      return true;
    case 'redo':
      app.redo();
      return true;
    case 'copy': {
      // Selected page text is copied as text, as usual.
      if (document.getSelection()?.toString()) return false;
      if (c) copyAt(c);
      else if (app.stick) copyGroupAt(selectedGroup(null)!);
      return !!(c || app.stick);
    }
    case 'paste': {
      const g = selectedGroup(c);
      if (app.clipboard?.kind === 'group' && g) pasteGroupAt(g);
      else if (app.clipboard?.kind === 'control' && c) pasteAt(c);
      else return false;
      return true;
    }
    case 'clear':
      if (!c) return false;
      clearAt(c);
      return true;
    case 'deselect':
      if (!c && !app.stick && app.view !== 'macro') return false;
      if (app.view === 'macro') app.view = 'front';
      app.select(null);
      return true;
    case 'layer': {
      const id = app.profile ? layerIds(app.profile)[(digit ?? 1) - 1] : undefined;
      if (id === undefined) return false;
      app.layer = id;
      return true;
    }
    case 'rename':
      app.renameConfig();
      return true;
    case 'duplicate':
      app.duplicateConfig();
      return true;
    case 'help':
      app.shortcutsOpen = true;
      return true;
  }
}
