import { describe, expect, it } from 'vitest';
import { errorText } from './api';
import { setLanguage } from './i18n/index.svelte';

describe('errorText', () => {
  it("words the app's own errors in the window's language", () => {
    setLanguage('de');
    expect(errorText({ code: 'portable_autostart', detail: '' })).toBe('Eine portable Kopie startet nicht mit dem System.');
    setLanguage('en');
    expect(errorText({ code: 'sheet_not_html', detail: '' })).toBe('A bindings sheet is saved as an .html file.');
  });

  it('shows other errors in their own words', () => {
    setLanguage('en');
    expect(errorText({ code: 'failed', detail: 'C:\\x.toml: access denied' })).toBe('C:\\x.toml: access denied');
    expect(errorText({ code: 'from_a_newer_app', detail: 'disk full' })).toBe('disk full');
    expect(errorText('invalid args `dir` for command `game_update`')).toBe('invalid args `dir` for command `game_update`');
    expect(errorText(new Error('boom'))).toBe('boom');
  });
});
