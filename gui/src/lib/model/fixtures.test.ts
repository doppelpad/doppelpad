import { describe, expect, it } from 'vitest';
import type { Profile } from '../types';
import examples from './fixtures/examples.json';
import keys from './fixtures/keys.json';
import fixture from './fixtures/profile.json';
import { KEY_CODE_MAP } from './keymap';
import { tidyMapping, upsertMapping } from './profile';

describe('fixtures shared with the Rust tests', () => {
  it('every browser key maps to a valid key name', () => {
    const valid = new Set<string>(keys);
    const invalid = Object.values(KEY_CODE_MAP).filter((k) => !valid.has(k));
    expect(invalid).toEqual([]);
  });

  it('the editor keeps canonical mappings unchanged', () => {
    const profiles = [fixture, ...Object.values(examples)] as unknown as Profile[];
    for (const p of profiles) for (const m of p.mappings) expect(tidyMapping(m)).toEqual(m);
  });

  it('edits that change nothing keep the example profiles identical', () => {
    for (const [name, json] of Object.entries(examples)) {
      const p = json as unknown as Profile;
      let edited = p;
      for (const m of p.mappings) edited = upsertMapping(edited, structuredClone(m));
      expect(edited, name).toEqual(p);
    }
  });
});
