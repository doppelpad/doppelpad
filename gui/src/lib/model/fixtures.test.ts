import { describe, expect, it } from 'vitest';
import type { Profile } from '../types';
import examples from './fixtures/examples.json';
import keys from './fixtures/keys.json';
import fixture from './fixtures/profile.json';
import { setStick, setTrigger, stickConfig, triggerConfig } from './analog';
import { defaults } from './fixtures';
import { KEY_CODE_MAP } from './keymap';
import { tidyMapping, upsertMapping } from './profile';
import { updateRoute } from './routes';

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
    for (const [name, json] of Object.entries({ ...examples, fixture })) {
      const p = json as unknown as Profile;
      let edited = p;
      for (const m of p.mappings) edited = upsertMapping(edited, structuredClone(m));
      for (const side of ['left', 'right'] as const) {
        edited = setStick(edited, side, stickConfig(p, side, defaults.stick), defaults.stick);
        edited = setTrigger(edited, side, triggerConfig(p, side, defaults.trigger), defaults.trigger);
      }
      (p.analog ?? []).forEach((_, i) => (edited = updateRoute(edited, i, {})));
      expect(edited, name).toEqual(p);
    }
  });
});
