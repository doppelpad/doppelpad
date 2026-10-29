import { describe, expect, it } from 'vitest';
import { TOOLBAR_VIEWS, viewFor } from './views';

describe('views', () => {
  it('the toolbar lists every view but the macro editor, in its order', () => {
    expect(TOOLBAR_VIEWS).toEqual(['front', 'back', 'keyboard', 'mouse', 'analog', 'shortcuts', 'radial', 'output']);
  });

  it('picks the view that shows a trigger', () => {
    expect(viewFor(['LB', 'RB'], 'front')).toBe('shortcuts');
    expect(viewFor('Key:W', 'front')).toBe('keyboard');
    expect(viewFor('Mouse:Left', 'keyboard')).toBe('mouse');
    expect(viewFor('A', 'back')).toBe('back');
    expect(viewFor('A', 'keyboard')).toBe('front');
  });
});
