import { describe, expect, it } from 'vitest';
import { setLanguage } from '../i18n/index.svelte';
import type { Profile } from '../types';
import { bindingsSheet } from './sheet';

setLanguage('en');

const source: Profile = {
  name: 'Racing <1>',
  layers: [{ id: 2, name: 'Drive', inherit: false }],
  mappings: [
    { trigger: 'A', actions: [{ key: 'Space' }] },
    { trigger: 'B', layer: 2, actions: [{ key: 'W' }], description: 'gas' },
  ],
  analog: [{ layer: 2, source: 'gyro', target: 'mouse' }],
};

describe('bindings sheet', () => {
  it('lists every layer with escaped text', () => {
    const html = bindingsSheet(source, null);
    expect(html).toContain('<title>Racing &#60;1&#62;</title>');
    expect(html.match(/<h2>/g)).toHaveLength(2);
    expect(html).toContain('gas');
    expect(html).not.toContain('<1>');
  });

  it("lists a layer's analog routes after its mappings", () => {
    const html = bindingsSheet(source, null);
    const drive = html.slice(html.indexOf('<h2>Drive</h2>'));
    expect(drive).toContain('<td class="control">Gyro</td><td>→ Mouse</td>');
    expect(drive.indexOf('gas')).toBeLessThan(drive.indexOf('→ Mouse'));
  });
});
