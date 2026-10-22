import { afterEach, describe, expect, it } from 'vitest';
import { language, setLanguage, t } from './index.svelte';

afterEach(() => setLanguage('en'));

describe('t()', () => {
  it('fills in parameters and leaves unknown ones as they are', () => {
    expect(t('slots.slot', { n: 2 })).toBe('Slot 2');
    expect(t('slots.slot')).toBe('Slot {n}');
  });

  it('speaks the chosen language, English for unknown ones', () => {
    setLanguage('de');
    expect(language()).toBe('de');
    expect(t('common.cancel')).toBe('Abbrechen');
    setLanguage('xx');
    expect(language()).toBe('en');
    expect(t('common.cancel')).toBe('Cancel');
  });

  it('shows the key of a text it does not have', () => {
    const key = 'nowhere.to_be_found';
    expect(t(key)).toBe(key);
  });
});
