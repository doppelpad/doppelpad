import { describe, expect, it } from 'vitest';
import { patched } from './overlay';

describe('overlay content', () => {
  it('a patch changes its fields and keeps the rest', () => {
    const menu = { menu: { name: 'Tools' }, images: ['data:image/png;base64,AA'], pointed: null as number | null };
    const next = patched(menu, { pointed: 2 });
    expect(next).toEqual({ ...menu, pointed: 2 });
    expect(next?.images).toBe(menu.images);
    expect(menu.pointed).toBeNull();
  });

  it('a patch without content shows nothing', () => {
    expect(patched<{ pointed: number | null }>(null, { pointed: 1 })).toBeNull();
  });
});
