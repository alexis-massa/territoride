import { describe, expect, it } from 'vitest';

import { covers, grow, layerQuery } from './view';

describe('viewport loading', () => {
  it('grows the viewport by a margin and stays inside the world', () => {
    expect(grow([0, 0, 10, 10], 0.25)).toEqual([-2.5, -2.5, 12.5, 12.5]);
    expect(grow([-179, -89, 179, 89], 0.25)).toEqual([-180, -90, 180, 90]);
  });

  it('reloads when the view leaves the loaded area or zooms in a level', () => {
    const loaded = { bbox: [0, 0, 10, 10] as [number, number, number, number], zoom: 8 };
    expect(covers(loaded, [1, 1, 9, 9], 8)).toBe(true);
    expect(covers(loaded, [1, 1, 9, 9], 8.9)).toBe(true);
    expect(covers(loaded, [1, 1, 9, 9], 9)).toBe(false);
    expect(covers(loaded, [-1, 1, 9, 9], 8)).toBe(false);
    expect(covers(null, [1, 1, 9, 9], 8)).toBe(false);
  });

  it('builds the query', () => {
    expect(layerQuery([5.5, 45, 6.5, 46], false)).toBe('?bbox=5.5%2C45%2C6.5%2C46');
    expect(layerQuery([5.5, 45, 6.5, 46], true)).toBe('?bbox=5.5%2C45%2C6.5%2C46&mine=1');
  });
});
