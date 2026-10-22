import { describe, expect, it } from 'vitest';
import { movedPath } from './paths';

describe('paths in a moved folder', () => {
  it('follow the folder, with either separator', () => {
    expect(movedPath('C:\\a\\profiles\\game\\x.toml', 'C:\\a\\profiles', 'D:\\Sync\\pads')).toBe('D:\\Sync\\pads\\game\\x.toml');
    expect(movedPath('/home/u/profiles/x.toml', '/home/u/profiles/', '/sync/pads')).toBe('/sync/pads/x.toml');
    expect(movedPath('C:\\a\\profiles', 'C:\\a\\profiles', 'D:\\pads\\')).toBe('D:\\pads');
  });

  it('leave paths outside the folder alone', () => {
    expect(movedPath('C:\\a\\profiles2\\x.toml', 'C:\\a\\profiles', 'D:\\pads')).toBe('C:\\a\\profiles2\\x.toml');
    expect(movedPath('C:\\b\\x.toml', 'C:\\a\\profiles', 'D:\\pads')).toBe('C:\\b\\x.toml');
  });
});
