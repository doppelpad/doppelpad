import { describe, expect, it } from 'vitest';
import { addProgram, hasProgram } from './games';

describe('game programs', () => {
  it('a program is there once, whatever its case', () => {
    expect(hasProgram(['Game.exe'], 'game.EXE')).toBe(true);
    expect(hasProgram(['Game.exe'], ' Game.exe ')).toBe(true);
    expect(hasProgram(['Game.exe'], 'Other.exe')).toBe(false);
    expect(addProgram(['Game.exe'], 'GAME.exe')).toEqual(['Game.exe']);
    expect(addProgram(['Game.exe'], ' Other.exe ')).toEqual(['Game.exe', 'Other.exe']);
    expect(addProgram(['Game.exe'], '  ')).toEqual(['Game.exe']);
  });
});
