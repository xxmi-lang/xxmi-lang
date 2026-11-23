import { describe, expect, it } from 'vitest';
import { compareLoadOrder } from '../src/workspace/load-order.ts';

describe('compareLoadOrder', () => {
  it('compares names case-insensitively', () => {
    expect(compareLoadOrder('a.ini', 'B.ini')).toBeLessThan(0);
    expect(compareLoadOrder('B.ini', 'a.ini')).toBeGreaterThan(0);
  });

  it('loads files before subfolders', () => {
    expect(compareLoadOrder('z.ini', 'sub/a.ini')).toBeLessThan(0);
    expect(compareLoadOrder('sub/a.ini', 'z.ini')).toBeGreaterThan(0);
  });

  it('compares folders case-insensitively', () => {
    expect(compareLoadOrder('Sub/x.ini', 'sub2/y.ini')).toBeLessThan(0);
    expect(compareLoadOrder('sub/b.ini', 'Sub/a.ini')).toBeGreaterThan(0);
  });
});
